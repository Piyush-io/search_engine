import os
import shutil
import subprocess
import tarfile
import tempfile
from pathlib import Path

import modal

APP_NAME = "search-engine-remote"
DATA_VOLUME_NAME = "search-engine-data"
RESULTS_VOLUME_NAME = "search-engine-results"
REPO_URL = "https://github.com/Piyush-io/search_engine.git"
DEFAULT_GIT_REF = os.environ.get("SEARCH_ENGINE_GIT_REF", "origin/main")

REPO_DIR = "/workspace/search_engine"
DATA_DIR = "/data"
RESULTS_DIR = "/results"
REMOTE_CONFIG = f"{REPO_DIR}/config.toml"
REMOTE_MODAL_CONFIG = f"{REPO_DIR}/config.modal.toml"
FALLBACK_MODAL_CONFIG = """[crawl]
max_pages = 2_000_000
concurrency = 200
rate_limit_ms = 100

[embedding]
backend = \"cuda\"
model = \"bge-small-en-v1.5\"
dim = 384
batch_size = 256
max_length = 128
bulk_workers = 1
bulk_intra_threads = 4

[hnsw]
backend = \"hnsw\"
shards = 1
m = 16
ef_construction = 200
ef_search = 200
max_elements = 5_000_000

[chunking]
context_depth = 3
window_size = 3
window_overlap = 1

[rocksdb]
block_cache_mb = 256

[server]
port = 3000

[paths]
db_path = \"/data/crawl_data\"
index_path = \"/data/hnsw_index.bin\"
lexical_index_path = \"/data/lexical_index\"
wiki_index_path = \"/data/wiki_hnsw.bin\"
vector_delta_path = \"/data/hnsw_delta.bin\"
seeds_path = \"/workspace/seeds.md\"
"""

app = modal.App(APP_NAME)
data_volume = modal.Volume.from_name(DATA_VOLUME_NAME, create_if_missing=True)
results_volume = modal.Volume.from_name(RESULTS_VOLUME_NAME, create_if_missing=True)

base_image = (
    modal.Image.debian_slim(python_version="3.11")
    .apt_install(
        "bash",
        "build-essential",
        "ca-certificates",
        "clang",
        "cmake",
        "curl",
        "git",
        "libclang-dev",
        "libssl-dev",
        "pkg-config",
        "tar",
    )
    .run_commands("curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y")
    .env({"PATH": "/root/.cargo/bin:$PATH"})
)


def _run(cmd: str, cwd: str | None = None, env: dict | None = None) -> None:
    print(f"\n$ {cmd}")
    subprocess.run(cmd, shell=True, check=True, cwd=cwd, env=env)


def _capture(cmd: str, cwd: str | None = None, env: dict | None = None) -> str:
    print(f"\n$ {cmd}")
    out = subprocess.check_output(cmd, shell=True, cwd=cwd, env=env, text=True)
    print(out)
    return out


def _ensure_repo(git_ref: str) -> None:
    if not Path(REPO_DIR).exists():
        _run(f"git clone {REPO_URL} {REPO_DIR}")
    else:
        _run("git fetch --all --prune", cwd=REPO_DIR)
    _run(f"git reset --hard {git_ref}", cwd=REPO_DIR)



def _prepare_workspace(git_ref: str) -> dict:
    _ensure_repo(git_ref)

    if Path(REMOTE_MODAL_CONFIG).exists():
        shutil.copyfile(REMOTE_MODAL_CONFIG, REMOTE_CONFIG)
    else:
        Path(REMOTE_CONFIG).write_text(FALLBACK_MODAL_CONFIG)
    Path(DATA_DIR).mkdir(parents=True, exist_ok=True)
    Path(RESULTS_DIR).mkdir(parents=True, exist_ok=True)
    Path(f"{DATA_DIR}/crawl_data").mkdir(parents=True, exist_ok=True)
    Path(f"{RESULTS_DIR}/reports").mkdir(parents=True, exist_ok=True)

    env = os.environ.copy()
    env["MALLOC_ARENA_MAX"] = "2"
    env.setdefault("RAYON_NUM_THREADS", "6")

    onnx_dir = _capture(
        "find $HOME/.cache /root/.cache /tmp /workspace -name libonnxruntime.so -exec dirname {} \\; 2>/dev/null | head -n 1 || true"
    ).strip()
    if onnx_dir:
        env["LD_LIBRARY_PATH"] = f"{onnx_dir}:{env.get('LD_LIBRARY_PATH', '')}".rstrip(":")
        print(f"Using LD_LIBRARY_PATH={env['LD_LIBRARY_PATH']}")

    return env



def _write_report(name: str, content: str) -> str:
    reports_dir = Path(RESULTS_DIR) / "reports"
    reports_dir.mkdir(parents=True, exist_ok=True)
    out = reports_dir / name
    out.write_text(content)
    return str(out)



def _copy_report_if_exists(src_rel: str, dst_name: str) -> None:
    src = Path(REPO_DIR) / src_rel
    if src.exists():
        dst = Path(RESULTS_DIR) / "reports" / dst_name
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)



def _summarize_data_dir() -> str:
    lines = []
    for p in [
        Path(f"{DATA_DIR}/hnsw_index.bin"),
        Path(f"{DATA_DIR}/hnsw_index.bin.hnsw.data"),
        Path(f"{DATA_DIR}/hnsw_index.bin.hnsw.graph"),
        Path(f"{DATA_DIR}/lexical_index"),
        Path(f"{DATA_DIR}/crawl_data"),
    ]:
        lines.append(f"{p}: exists={p.exists()} size={p.stat().st_size if p.exists() and p.is_file() else '-'}")
    summary = "\n".join(lines)
    print(summary)
    return summary


@app.function(
    image=base_image,
    cpu=4,
    memory=16384,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60,
)
def run_tests_cpu(git_ref: str = DEFAULT_GIT_REF) -> str:
    env = _prepare_workspace(git_ref)
    _run("cargo build --release --bins", cwd=REPO_DIR, env=env)
    out = _capture("cargo test --release", cwd=REPO_DIR, env=env)
    path = _write_report("cargo_test.txt", out)
    results_volume.commit()
    return path


@app.function(
    image=base_image,
    gpu="A10G",
    cpu=4,
    memory=32768,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60 * 6,
)
def bench_embed_gpu(git_ref: str = DEFAULT_GIT_REF, samples: int = 2000) -> str:
    env = _prepare_workspace(git_ref)
    _run("cargo build --release --bin bench_embed", cwd=REPO_DIR, env=env)
    out = _capture(f"./target/release/bench_embed --samples {samples}", cwd=REPO_DIR, env=env)
    path = _write_report("bench_embed.txt", out)
    results_volume.commit()
    return path


@app.function(
    image=base_image,
    cpu=8,
    memory=32768,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60,
)
def bench_query_cpu(git_ref: str = DEFAULT_GIT_REF) -> str:
    env = _prepare_workspace(git_ref)
    _run("cargo build --release --bin bench", cwd=REPO_DIR, env=env)
    out = _capture("./target/release/bench", cwd=REPO_DIR, env=env)
    _copy_report_if_exists("reports/benchmark_results.json", "benchmark_results.json")
    path = _write_report("bench_query.txt", out)
    results_volume.commit()
    return path


@app.function(
    image=base_image,
    gpu="A10G",
    cpu=8,
    memory=65536,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60 * 2,
)
def bench_ann_remote(git_ref: str = DEFAULT_GIT_REF) -> str:
    env = _prepare_workspace(git_ref)
    _run("cargo build --release --bin bench_ann", cwd=REPO_DIR, env=env)
    out = _capture("./target/release/bench_ann", cwd=REPO_DIR, env=env)
    _copy_report_if_exists("reports/bench_ann.json", "bench_ann.json")
    path = _write_report("bench_ann.txt", out)
    results_volume.commit()
    return path


@app.function(
    image=base_image,
    gpu="A10G",
    cpu=8,
    memory=65536,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60 * 8,
)
def embed_full_gpu(git_ref: str = DEFAULT_GIT_REF) -> str:
    env = _prepare_workspace(git_ref)
    _run("cargo build --release --bin embed --bin stats", cwd=REPO_DIR, env=env)
    _run("./target/release/embed --full-scan", cwd=REPO_DIR, env=env)
    out = _capture("./target/release/stats", cwd=REPO_DIR, env=env)
    path = _write_report("embed_full_stats.txt", out)
    data_volume.commit()
    results_volume.commit()
    return path


@app.function(
    image=base_image,
    cpu=8,
    memory=98304,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60 * 8,
)
def index_full_cpu(git_ref: str = DEFAULT_GIT_REF) -> str:
    env = _prepare_workspace(git_ref)
    _run("cargo build --release --bin index --bin stats", cwd=REPO_DIR, env=env)
    _run("./target/release/index --full", cwd=REPO_DIR, env=env)
    out = _capture("./target/release/stats", cwd=REPO_DIR, env=env)
    summary = _summarize_data_dir()
    path = _write_report("index_full_stats.txt", out + "\n\n" + summary)
    data_volume.commit()
    results_volume.commit()
    return path


@app.function(
    image=base_image,
    cpu=8,
    memory=32768,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60 * 6,
)
def lexical_full_cpu(git_ref: str = DEFAULT_GIT_REF) -> str:
    env = _prepare_workspace(git_ref)
    _run("cargo build --release --bin lexical_index --bin stats", cwd=REPO_DIR, env=env)
    _run("./target/release/lexical_index --full", cwd=REPO_DIR, env=env)
    out = _capture("./target/release/stats", cwd=REPO_DIR, env=env)
    summary = _summarize_data_dir()
    path = _write_report("lexical_full_stats.txt", out + "\n\n" + summary)
    data_volume.commit()
    results_volume.commit()
    return path


@app.function(
    image=base_image,
    cpu=2,
    memory=8192,
    volumes={DATA_DIR: data_volume},
    timeout=60 * 60,
)
def unpack_synced_db_tar() -> str:
    _run("rm -rf /data/crawl_data && mkdir -p /data && tar -xf /tmp/crawl_data.tar -C /data", cwd="/")
    data_volume.commit()
    return "synced /data/crawl_data"


@app.function(
    image=base_image,
    cpu=2,
    memory=4096,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=30 * 60,
)
def clear_results() -> str:
    reports = Path(RESULTS_DIR) / "reports"
    if reports.exists():
        shutil.rmtree(reports)
    reports.mkdir(parents=True, exist_ok=True)
    results_volume.commit()
    return "cleared /results/reports"


@app.function(
    image=base_image,
    cpu=2,
    memory=4096,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=30 * 60,
)
def clear_data_artifacts() -> str:
    targets = [
        Path(f"{DATA_DIR}/hnsw_index.bin"),
        Path(f"{DATA_DIR}/hnsw_index.bin.hnsw.data"),
        Path(f"{DATA_DIR}/hnsw_index.bin.hnsw.graph"),
        Path(f"{DATA_DIR}/hnsw_delta.bin"),
        Path(f"{DATA_DIR}/lexical_index"),
        Path(f"{DATA_DIR}/wiki_hnsw.bin"),
        Path(f"{DATA_DIR}/wiki_hnsw.bin.hnsw.data"),
        Path(f"{DATA_DIR}/wiki_hnsw.bin.hnsw.graph"),
    ]
    for target in targets:
        if target.is_dir():
            shutil.rmtree(target, ignore_errors=True)
        elif target.exists():
            target.unlink()
    data_volume.commit()
    return "cleared derived artifacts under /data (preserved /data/crawl_data)"



def _tar_directory(src_dir: str) -> str:
    fd, tar_path = tempfile.mkstemp(suffix=".tar")
    os.close(fd)
    with tarfile.open(tar_path, "w") as tar:
        tar.add(src_dir, arcname=Path(src_dir).name)
    return tar_path


@app.local_entrypoint()
def main(
    action: str = "help",
    git_ref: str = DEFAULT_GIT_REF,
    local_db: str = "./crawl_data",
    samples: int = 2000,
):
    if action == "help":
        print(
            "Available actions: sync_db, tests, bench_embed, bench_query, bench_ann, embed_full, index_full, lexical_full, clear_results, clear_data_artifacts"
        )
        return

    if action == "sync_db":
        src = Path(local_db)
        if not src.exists() or not src.is_dir():
            raise SystemExit(f"local DB directory not found: {src}")
        tar_path = _tar_directory(str(src))
        try:
            with data_volume.batch_upload() as batch:
                batch.put_file(tar_path, "/tmp/crawl_data.tar")
            data_volume.commit()
            print("Uploaded tarball to volume: /tmp/crawl_data.tar")
            print("Now unpacking remotely...")
            print(unpack_synced_db_tar.remote())
        finally:
            if os.path.exists(tar_path):
                os.remove(tar_path)
        return

    actions = {
        "tests": lambda: print(run_tests_cpu.remote(git_ref)),
        "bench_embed": lambda: print(bench_embed_gpu.remote(git_ref, samples)),
        "bench_query": lambda: print(bench_query_cpu.remote(git_ref)),
        "bench_ann": lambda: print(bench_ann_remote.remote(git_ref)),
        "embed_full": lambda: print(embed_full_gpu.remote(git_ref)),
        "index_full": lambda: print(index_full_cpu.remote(git_ref)),
        "lexical_full": lambda: print(lexical_full_cpu.remote(git_ref)),
        "clear_results": lambda: print(clear_results.remote()),
        "clear_data_artifacts": lambda: print(clear_data_artifacts.remote()),
    }

    if action not in actions:
        raise SystemExit(f"unknown action: {action}")

    actions[action]()
