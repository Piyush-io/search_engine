import os
import re
import shlex
import shutil
import subprocess
import time
import json
import tomllib
from html import unescape
from pathlib import Path
from urllib.parse import quote_plus
from urllib.request import urlopen

import modal

APP_NAME = "search-engine-remote"
DATA_VOLUME_NAME = "search-engine-data"
RESULTS_VOLUME_NAME = "search-engine-results"
DEFAULT_GIT_REF = os.environ.get("SEARCH_ENGINE_GIT_REF", "origin/main")

REPO_DIR = "/workspace/search_engine"
DATA_DIR = "/data"
RESULTS_DIR = "/results"
REMOTE_CONFIG = f"{REPO_DIR}/config.toml"
REMOTE_MODAL_CONFIG = f"{REPO_DIR}/config.modal.toml"
REMOTE_HIGH_QUALITY_CONFIG = f"{REPO_DIR}/config.high_quality.toml"
REMOTE_RUNTIME_CONFIG = f"{REPO_DIR}/config.runtime.toml"
PERSISTENT_SEARCH_CLS_NAME = "PersistentSearchSession"
PERSISTENT_SEARCH_WEB_FUNCTION_NAME = "persistent_search_web"
PERSISTENT_SEARCH_PORT = 3000
CUDA_BASE_IMAGE = "nvidia/cuda:12.4.1-cudnn-runtime-ubuntu22.04"
CUDA_RUNTIME_PATHS = [
    "/usr/local/cuda/lib64",
    "/usr/local/cuda/targets/x86_64-linux/lib",
    "/usr/local/nvidia/lib",
    "/usr/local/nvidia/lib64",
]
FALLBACK_MODAL_CONFIG = f"""[crawl]
max_pages = 2_000_000
concurrency = 256
rate_limit_ms = 75
recrawl_days = 30

[embedding]
backend = \"cuda\"
model = \"bge-small-en-v1.5\"
dim = 384
batch_size = 512
max_length = 128
bulk_workers = 1
bulk_intra_threads = 2

[hnsw]
backend = \"hnsw\"
shards = 1
m = 8
ef_construction = 120
ef_search = 96
max_elements = 5_000_000

[chunking]
context_depth = 3
window_size = 3
window_overlap = 1

[ranking]

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
seeds_path = "{REPO_DIR}/seeds.md"
"""

app = modal.App(APP_NAME)
data_volume = modal.Volume.from_name(DATA_VOLUME_NAME, create_if_missing=True)
results_volume = modal.Volume.from_name(RESULTS_VOLUME_NAME, create_if_missing=True)
REMOTE_SOURCE_IGNORE = [
    ".git",
    ".git/**",
    ".fastembed_cache",
    ".fastembed_cache/**",
    "__pycache__",
    "__pycache__/**",
    "crawl_data",
    "crawl_data/**",
    "crawl_data.high_quality",
    "crawl_data.high_quality/**",
    "crawl_data.local_backup",
    "crawl_data.local_backup/**",
    "lexical_index",
    "lexical_index/**",
    "lexical_index.high_quality",
    "lexical_index.high_quality/**",
    "lexical_index.local_backup",
    "lexical_index.local_backup/**",
    "modal_restore_tmp",
    "modal_restore_tmp/**",
    "migration_backup_*",
    "migration_backup_*/**",
    "reports",
    "reports/**",
    "target",
    "target/**",
    "*.bin",
    "*.data",
    "*.graph",
    "*.jpg",
    "*.pdf",
    "*.png",
]


def _build_image(image: modal.Image) -> modal.Image:
    return (
        image.apt_install(
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
        .run_commands(
            "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"
        )
        .env({"PATH": "/root/.cargo/bin:$PATH"})
        .add_local_dir(
            ".", remote_path=REPO_DIR, copy=True, ignore=REMOTE_SOURCE_IGNORE
        )
    )


cpu_image = _build_image(modal.Image.debian_slim(python_version="3.11"))
gpu_image = _build_image(
    modal.Image.from_registry(CUDA_BASE_IMAGE, add_python="3.11")
).env({"CUDA_PATH": "/usr/local/cuda"})


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
        raise RuntimeError(f"mounted workspace not found at {REPO_DIR}")
    print(
        f"Using mounted workspace snapshot at {REPO_DIR}; git_ref={git_ref!r} is ignored"
    )


def _run_with_output(cmd: str, cwd: str | None = None, env: dict | None = None) -> str:
    print(f"\n$ {cmd}")
    completed = subprocess.run(
        cmd,
        shell=True,
        check=True,
        cwd=cwd,
        env=env,
        text=True,
        capture_output=True,
    )
    if completed.stdout:
        print(completed.stdout)
    if completed.stderr:
        print(completed.stderr)
    return completed.stdout + completed.stderr


def _fetch_http(url: str, timeout: int = 30) -> str:
    with urlopen(url, timeout=timeout) as response:
        return response.read().decode("utf-8", errors="replace")


def _wait_for_http_ready(url: str, timeout_s: int = 300) -> None:
    deadline = time.time() + timeout_s
    last_error: Exception | None = None

    while time.time() < deadline:
        try:
            _fetch_http(url, timeout=10)
            return
        except Exception as exc:  # noqa: BLE001
            last_error = exc
            time.sleep(1)

    raise RuntimeError(f"timed out waiting for server {url}: {last_error}")


def _slugify(value: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", value.lower()).strip("-")
    return slug or "query"


def _extract_results_from_search_html(html: str, top_k: int) -> list[tuple[str, str]]:
    pattern = re.compile(
        r'<div class="result-url"><cite>(.*?)</cite></div>\s*<a class="result-title" href="[^"]+">(.*?)</a>',
        re.S,
    )
    results: list[tuple[str, str]] = []

    for url_text, title_text in pattern.findall(html):
        clean_url = re.sub(r"<[^>]+>", "", url_text)
        clean_title = re.sub(r"<[^>]+>", "", title_text)
        results.append((unescape(clean_url).strip(), unescape(clean_title).strip()))
        if len(results) >= top_k:
            break

    return results


def _prepend_env_path(env: dict, key: str, parts: list[str]) -> None:
    existing = env.get(key, "")
    merged = [part for part in parts if part]
    if existing:
        merged.append(existing)
    if merged:
        env[key] = ":".join(merged)


def _toml_value(value) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, str):
        return json.dumps(value)
    return str(value)


def _write_runtime_config(config_name: str) -> str:
    config_sources = {
        "modal": Path(REMOTE_MODAL_CONFIG),
        "high_quality": Path(REMOTE_HIGH_QUALITY_CONFIG),
    }
    source = config_sources.get(config_name)
    if source is None:
        raise RuntimeError(f"unknown config profile: {config_name}")
    if not source.exists():
        if config_name == "modal":
            Path(REMOTE_CONFIG).write_text(FALLBACK_MODAL_CONFIG)
            return REMOTE_CONFIG
        raise RuntimeError(f"required config file missing from workspace snapshot: {source}")

    with source.open("rb") as handle:
        config = tomllib.load(handle)

    paths = dict(config.get("paths", {}))
    suffixes = {
        "db_path": "crawl_data",
        "index_path": "hnsw_index.bin",
        "lexical_index_path": "lexical_index",
        "wiki_index_path": "wiki_hnsw.bin",
        "vector_delta_path": "hnsw_delta.bin",
    }
    if config_name == "high_quality":
        suffixes = {
            "db_path": "crawl_data.high_quality",
            "index_path": "hnsw_index.high_quality.bin",
            "lexical_index_path": "lexical_index.high_quality",
            "wiki_index_path": "wiki_hnsw.high_quality.bin",
            "vector_delta_path": "hnsw_delta.high_quality.bin",
        }

    for key, suffix in suffixes.items():
        paths[key] = f"{DATA_DIR}/{suffix}"

    seeds_name = Path(paths.get("seeds_path", "seeds.md")).name
    paths["seeds_path"] = f"{REPO_DIR}/{seeds_name}"
    config["paths"] = paths

    lines: list[str] = []
    for section, values in config.items():
        lines.append(f"[{section}]")
        for key, value in values.items():
            lines.append(f"{key} = {_toml_value(value)}")
        lines.append("")

    Path(REMOTE_RUNTIME_CONFIG).write_text("\n".join(lines).strip() + "\n")
    return REMOTE_RUNTIME_CONFIG


def _prepare_workspace(
    git_ref: str, use_gpu: bool = False, config_name: str = "modal"
) -> dict:
    _ensure_repo(git_ref)

    runtime_config = _write_runtime_config(config_name)
    Path(DATA_DIR).mkdir(parents=True, exist_ok=True)
    Path(RESULTS_DIR).mkdir(parents=True, exist_ok=True)
    Path(f"{DATA_DIR}/crawl_data").mkdir(parents=True, exist_ok=True)
    Path(f"{DATA_DIR}/crawl_data.high_quality").mkdir(parents=True, exist_ok=True)
    Path(f"{RESULTS_DIR}/reports").mkdir(parents=True, exist_ok=True)

    env = os.environ.copy()
    env["MALLOC_ARENA_MAX"] = "2"
    env.setdefault("RAYON_NUM_THREADS", "12")
    env.setdefault("CARGO_BUILD_JOBS", "12")
    env["SEARCH_ENGINE_CONFIG_PATH"] = runtime_config

    if use_gpu:
        env["CUDA_PATH"] = "/usr/local/cuda"
        _prepend_env_path(
            env,
            "LD_LIBRARY_PATH",
            [path for path in CUDA_RUNTIME_PATHS if Path(path).exists()],
        )

    return env


@app.cls(
    image=gpu_image,
    gpu="L40S",
    cpu=8,
    memory=32768,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60 * 12,
    min_containers=1,
    scaledown_window=60 * 60,
)
class PersistentSearchSession:
    @modal.enter()
    def start(self) -> None:
        env = _prepare_workspace(DEFAULT_GIT_REF, use_gpu=True)
        _run(
            "cargo build --release --bin search_engine --bin stats --bin queue_stats --bin index_stats",
            cwd=REPO_DIR,
            env=env,
        )
        _configure_onnxruntime_library_path(env)

        log_path = Path("/tmp/persistent_search_server.log")
        self.server_log_handle = log_path.open("a", buffering=1)
        self.server_process = subprocess.Popen(
            ["./target/release/search_engine"],
            cwd=REPO_DIR,
            env=env,
            text=True,
            stdout=self.server_log_handle,
            stderr=subprocess.STDOUT,
        )
        _wait_for_http_ready(f"http://127.0.0.1:{PERSISTENT_SEARCH_PORT}/")
        self.env = env

    @modal.exit()
    def stop(self) -> None:
        if self.server_process and self.server_process.poll() is None:
            self.server_process.terminate()
            try:
                self.server_process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                self.server_process.kill()
                self.server_process.wait(timeout=5)

        if self.server_log_handle is not None:
            self.server_log_handle.close()

    def _query_urls(self, query_text: str, top_k: int) -> dict[str, str]:
        encoded = quote_plus(query_text)
        base = f"http://127.0.0.1:{PERSISTENT_SEARCH_PORT}"
        return {
            "search": f"{base}/search?q={encoded}",
            "debug_html": f"{base}/debug/search?q={encoded}",
            "debug_json": f"{base}/debug/api/search?q={encoded}&k={top_k}",
        }

    def _write_query_reports(self, query_text: str, top_k: int) -> str:
        urls = self._query_urls(query_text, top_k)
        search_html = _fetch_http(urls["search"])
        debug_html = _fetch_http(urls["debug_html"])
        debug_json = _fetch_http(urls["debug_json"])
        debug_payload = json.loads(debug_json)
        results = _extract_results_from_search_html(search_html, top_k)

        slug = _slugify(query_text)
        html_path = _write_report(f"persistent_debug_{slug}.html", debug_html)
        json_path = _write_report(
            f"persistent_debug_{slug}.json",
            json.dumps(debug_payload, indent=2, sort_keys=True),
        )
        lines = [
            f"query={query_text!r}",
            f"top_k={top_k}",
            f"elapsed_ms={debug_payload.get('elapsed_ms', 0)}",
            f"result_count={debug_payload.get('result_count', 0)}",
            f"debug_html={html_path}",
            f"debug_json={json_path}",
            "results:",
        ]

        if results:
            for idx, (url_text, title_text) in enumerate(results, start=1):
                lines.append(f"{idx}. {title_text} — {url_text}")
        else:
            lines.append("(no parsed results)")

        txt_path = _write_report(f"persistent_query_{slug}.txt", "\n".join(lines))
        results_volume.commit()
        return f"{txt_path}\n\n" + "\n".join(lines)

    @modal.method()
    def sample_query(
        self,
        query_text: str = "what is a B-tree",
        top_k: int = 5,
    ) -> str:
        return self._write_query_reports(query_text, top_k)

    @modal.method()
    def query_suite(
        self,
        queries_text: str = "what is a B-tree||tcp three-way handshake||rust lifetime elision rules",
        top_k: int = 5,
    ) -> str:
        queries = [q.strip() for q in queries_text.split("||") if q.strip()]
        sections = []
        for query_text in queries:
            sections.append(self._write_query_reports(query_text, top_k))
        combined = "\n\n".join(sections)
        path = _write_report("persistent_query_suite.txt", combined)
        results_volume.commit()
        return f"{path}\n\n{combined}"

    @modal.method()
    def verify_state(self) -> str:
        if self.env is None:
            raise RuntimeError("persistent session environment was not initialized")

        stats_out = _capture("./target/release/stats", cwd=REPO_DIR, env=self.env)
        queue_out = _capture("./target/release/queue_stats", cwd=REPO_DIR, env=self.env)
        index_out = _capture("./target/release/index_stats", cwd=REPO_DIR, env=self.env)
        report = (
            "[stats]\n"
            f"{stats_out.strip()}\n\n"
            "[queue_stats]\n"
            f"{queue_out.strip()}\n\n"
            "[index_stats]\n"
            f"{index_out.strip()}\n"
        )
        path = _write_report("persistent_verify_state.txt", report)
        results_volume.commit()
        return f"{path}\n\n{report}"


@app.function(
    image=gpu_image,
    gpu="L40S",
    cpu=8,
    memory=32768,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60 * 12,
    min_containers=1,
)
@modal.web_server(PERSISTENT_SEARCH_PORT, startup_timeout=900.0, label="search")
def persistent_search_web() -> None:
    env = _prepare_workspace(DEFAULT_GIT_REF, use_gpu=True)
    _run("cargo build --release --bin search_engine", cwd=REPO_DIR, env=env)
    _configure_onnxruntime_library_path(env)
    subprocess.Popen(
        ["./target/release/search_engine"],
        cwd=REPO_DIR,
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.STDOUT,
    )


@app.function(
    image=cpu_image,
    cpu=6,
    memory=24576,
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
    image=gpu_image,
    gpu="L40S",
    cpu=8,
    memory=49152,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60 * 4,
)
def bench_embed_gpu(git_ref: str = DEFAULT_GIT_REF, samples: int = 2000) -> str:
    env = _prepare_workspace(git_ref, use_gpu=True)
    _run("cargo build --release --bin bench_embed", cwd=REPO_DIR, env=env)
    _configure_onnxruntime_library_path(env)
    out = _capture(
        f"./target/release/bench_embed --samples {samples}", cwd=REPO_DIR, env=env
    )
    path = _write_report("bench_embed.txt", out)
    results_volume.commit()
    return path


@app.function(
    image=cpu_image,
    cpu=12,
    memory=49152,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60,
)
def bench_query_cpu(git_ref: str = DEFAULT_GIT_REF) -> str:
    env = _prepare_workspace(git_ref)
    _run("cargo build --release --bin bench", cwd=REPO_DIR, env=env)
    _configure_onnxruntime_library_path(env)
    out = _capture("./target/release/bench", cwd=REPO_DIR, env=env)
    _copy_report_if_exists("reports/benchmark_results.json", "benchmark_results.json")
    path = _write_report("bench_query.txt", out)
    results_volume.commit()
    return path


@app.function(
    image=gpu_image,
    gpu="L40S",
    cpu=12,
    memory=65536,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60,
)
def bench_ann_remote(git_ref: str = DEFAULT_GIT_REF) -> str:
    env = _prepare_workspace(git_ref, use_gpu=True)
    _run("cargo build --release --bin bench_ann", cwd=REPO_DIR, env=env)
    _configure_onnxruntime_library_path(env)
    out = _capture("./target/release/bench_ann", cwd=REPO_DIR, env=env)
    _copy_report_if_exists("reports/bench_ann.json", "bench_ann.json")
    path = _write_report("bench_ann.txt", out)
    results_volume.commit()
    return path


@app.function(
    image=gpu_image,
    gpu="L40S",
    cpu=12,
    memory=65536,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60 * 5,
)
def embed_full_gpu(git_ref: str = DEFAULT_GIT_REF) -> str:
    env = _prepare_workspace(git_ref, use_gpu=True)
    _run("cargo build --release --bin embed --bin stats", cwd=REPO_DIR, env=env)
    _configure_onnxruntime_library_path(env)
    _run("./target/release/embed --full-scan", cwd=REPO_DIR, env=env)
    out = _capture("./target/release/stats", cwd=REPO_DIR, env=env)
    path = _write_report("embed_full_stats.txt", out)
    data_volume.commit()
    results_volume.commit()
    return path


@app.function(
    image=gpu_image,
    gpu="L40S",
    cpu=8,
    memory=32768,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=45 * 60,
)
def sample_query_remote(
    git_ref: str = DEFAULT_GIT_REF,
    query_text: str = "what is a B-tree",
    top_k: int = 5,
    config_name: str = "modal",
) -> str:
    env = _prepare_workspace(git_ref, use_gpu=True, config_name=config_name)
    _run("cargo build --release --bin sample_query", cwd=REPO_DIR, env=env)
    _configure_onnxruntime_library_path(env)
    out = _run_with_output(
        f"./target/release/sample_query {shlex.quote(query_text)} {top_k}",
        cwd=REPO_DIR,
        env=env,
    )
    path = _write_report("sample_query.txt", out)
    results_volume.commit()
    return path


@app.function(
    image=gpu_image,
    gpu="L40S",
    cpu=8,
    memory=32768,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60,
)
def query_suite_remote(
    git_ref: str = DEFAULT_GIT_REF,
    queries_text: str = "what is a B-tree||tcp three-way handshake||rust lifetime elision rules",
    top_k: int = 5,
    config_name: str = "modal",
) -> str:
    env = _prepare_workspace(git_ref, use_gpu=True, config_name=config_name)
    _run("cargo build --release --bin query_suite", cwd=REPO_DIR, env=env)
    _configure_onnxruntime_library_path(env)
    out = _run_with_output(
        f"./target/release/query_suite {top_k} {shlex.quote(queries_text)}",
        cwd=REPO_DIR,
        env=env,
    )
    path = _write_report("query_suite.txt", out)
    results_volume.commit()
    return path


@app.function(
    image=cpu_image,
    cpu=16,
    memory=122880,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60 * 4,
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
    image=cpu_image,
    cpu=12,
    memory=49152,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60 * 3,
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
    image=cpu_image,
    cpu=4,
    memory=16384,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 30,
)
def verify_remote_state(git_ref: str = DEFAULT_GIT_REF) -> str:
    env = _prepare_workspace(git_ref)
    _run(
        "cargo build --release --bin stats --bin queue_stats --bin index_stats",
        cwd=REPO_DIR,
        env=env,
    )
    stats_out = _capture("./target/release/stats", cwd=REPO_DIR, env=env)
    queue_out = _capture("./target/release/queue_stats", cwd=REPO_DIR, env=env)
    index_out = _capture("./target/release/index_stats", cwd=REPO_DIR, env=env)
    report = (
        "[stats]\n"
        f"{stats_out.strip()}\n\n"
        "[queue_stats]\n"
        f"{queue_out.strip()}\n\n"
        "[index_stats]\n"
        f"{index_out.strip()}\n"
    )
    path = _write_report("verify_remote_state.txt", report)
    results_volume.commit()
    return path


@app.function(
    image=cpu_image,
    cpu=2,
    memory=8192,
    volumes={DATA_DIR: data_volume},
    timeout=60 * 60,
)
def clear_synced_db() -> str:
    _run(f"rm -rf {DATA_DIR}/crawl_data && mkdir -p {DATA_DIR}/crawl_data", cwd="/")
    data_volume.commit()
    return "cleared /data/crawl_data"


@app.function(
    image=cpu_image,
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
    image=cpu_image,
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


@app.function(
    image=cpu_image,
    cpu=2,
    memory=8192,
    volumes={DATA_DIR: data_volume},
    timeout=60 * 60,
)
def clear_high_quality_synced_db() -> str:
    _run(
        f"rm -rf {DATA_DIR}/crawl_data.high_quality && mkdir -p {DATA_DIR}/crawl_data.high_quality",
        cwd="/",
    )
    data_volume.commit()
    return "cleared /data/crawl_data.high_quality"


@app.function(
    image=cpu_image,
    cpu=2,
    memory=4096,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=30 * 60,
)
def clear_high_quality_data_artifacts() -> str:
    targets = [
        Path(f"{DATA_DIR}/hnsw_index.high_quality.bin"),
        Path(f"{DATA_DIR}/hnsw_index.high_quality.bin.hnsw.data"),
        Path(f"{DATA_DIR}/hnsw_index.high_quality.bin.hnsw.graph"),
        Path(f"{DATA_DIR}/hnsw_delta.high_quality.bin"),
        Path(f"{DATA_DIR}/lexical_index.high_quality"),
        Path(f"{DATA_DIR}/wiki_hnsw.high_quality.bin"),
        Path(f"{DATA_DIR}/wiki_hnsw.high_quality.bin.hnsw.data"),
        Path(f"{DATA_DIR}/wiki_hnsw.high_quality.bin.hnsw.graph"),
    ]
    for target in targets:
        if target.is_dir():
            shutil.rmtree(target, ignore_errors=True)
        elif target.exists():
            target.unlink()
    data_volume.commit()
    return "cleared derived high-quality artifacts under /data (preserved /data/crawl_data.high_quality)"


@app.function(
    image=gpu_image,
    gpu="L40S",
    cpu=16,
    memory=131072,
    volumes={DATA_DIR: data_volume, RESULTS_DIR: results_volume},
    timeout=60 * 60 * 12,
)
def phase23_high_quality_remote(git_ref: str = DEFAULT_GIT_REF) -> str:
    env = _prepare_workspace(git_ref, use_gpu=True, config_name="high_quality")
    _run(
        "cargo build --release --bin normalize_pages --bin embed --bin index --bin lexical_index --bin stats --bin queue_stats --bin index_stats --bin domain_stats --bin sample_query --bin query_suite",
        cwd=REPO_DIR,
        env=env,
    )
    _configure_onnxruntime_library_path(env)

    clear_targets = [
        Path(f"{DATA_DIR}/hnsw_index.high_quality.bin"),
        Path(f"{DATA_DIR}/hnsw_index.high_quality.bin.hnsw.data"),
        Path(f"{DATA_DIR}/hnsw_index.high_quality.bin.hnsw.graph"),
        Path(f"{DATA_DIR}/hnsw_delta.high_quality.bin"),
        Path(f"{DATA_DIR}/lexical_index.high_quality"),
    ]
    for target in clear_targets:
        if target.is_dir():
            shutil.rmtree(target, ignore_errors=True)
        elif target.exists():
            target.unlink()

    sections = []
    commands = [
        ("normalize_pages", "./target/release/normalize_pages"),
        ("embed_full_scan", "./target/release/embed --full-scan"),
        ("index_full", "./target/release/index --full"),
        ("lexical_index_full", "./target/release/lexical_index --full"),
        ("stats", "./target/release/stats"),
        ("queue_stats", "./target/release/queue_stats"),
        ("index_stats", "./target/release/index_stats"),
        ("domain_stats", "./target/release/domain_stats --limit 50"),
        ("sample_query_btree", './target/release/sample_query "what is a B-tree" 5'),
        (
            "query_suite",
            './target/release/query_suite 5 "what is a B-tree||tcp three-way handshake||rust lifetime elision rules||sqlite wal checkpoint"',
        ),
    ]

    for title, command in commands:
        output = _run_with_output(command, cwd=REPO_DIR, env=env)
        sections.append(f"[{title}]\n{output.strip()}")

    report = "\n\n".join(sections) + "\n"
    path = _write_report("phase23_high_quality_remote.txt", report)
    data_volume.commit()
    results_volume.commit()
    return f"{path}\n\n{report}"


def _configure_onnxruntime_library_path(env: dict) -> None:
    candidates = [
        Path(REPO_DIR) / "target/release",
        Path(REPO_DIR) / "target/release/deps",
    ]
    onnx_dir = None
    for candidate in candidates:
        if candidate.exists() and any(candidate.glob("libonnxruntime.so*")):
            onnx_dir = str(candidate)
            break

    if onnx_dir is None:
        onnx_dir = _capture(
            "find target/release $HOME/.cache /root/.cache /tmp /workspace -name 'libonnxruntime.so*' -exec dirname {} \\; 2>/dev/null | head -n 1 || true",
            cwd=REPO_DIR,
            env=env,
        ).strip()

    if not onnx_dir:
        raise RuntimeError("libonnxruntime.so not found after cargo build")

    env["LD_LIBRARY_PATH"] = f"{onnx_dir}:{env.get('LD_LIBRARY_PATH', '')}".rstrip(":")
    print(f"Using LD_LIBRARY_PATH={env['LD_LIBRARY_PATH']}")


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
        lines.append(
            f"{p}: exists={p.exists()} size={p.stat().st_size if p.exists() and p.is_file() else '-'}"
        )
    summary = "\n".join(lines)
    print(summary)
    return summary


def _deployed_persistent_session():
    cls = modal.Cls.from_name(APP_NAME, PERSISTENT_SEARCH_CLS_NAME)
    return cls()


def _persistent_search_web_url() -> str:
    fn = modal.Function.from_name(APP_NAME, PERSISTENT_SEARCH_WEB_FUNCTION_NAME)
    url = fn.get_web_url()
    if not url:
        raise RuntimeError("persistent search web endpoint is not deployed")
    return url.rstrip("/")


def _persistent_web_query_report(query_text: str, top_k: int) -> str:
    base_url = _persistent_search_web_url()
    encoded = quote_plus(query_text)
    search_html = _fetch_http(f"{base_url}/search?q={encoded}", timeout=180)
    debug_url = f"{base_url}/debug/search?q={encoded}"
    results = _extract_results_from_search_html(search_html, top_k)

    lines = [
        f"query={query_text!r}",
        f"top_k={top_k}",
        f"debug_url={debug_url}",
        "results:",
    ]

    if results:
        for idx, (url_text, title_text) in enumerate(results, start=1):
            lines.append(f"{idx}. {title_text} — {url_text}")
    else:
        lines.append("(no parsed results)")

    return "\n".join(lines)


def _persistent_web_query_suite_report(queries_text: str, top_k: int) -> str:
    queries = [q.strip() for q in queries_text.split("||") if q.strip()]
    return "\n\n".join(
        _persistent_web_query_report(query_text, top_k) for query_text in queries
    )


@app.local_entrypoint()
def main(
    action: str = "help",
    git_ref: str = DEFAULT_GIT_REF,
    local_db: str = "./crawl_data",
    samples: int = 2000,
    query_text: str = "what is a B-tree",
    queries_text: str = "what is a B-tree||tcp three-way handshake||rust lifetime elision rules",
    top_k: int = 5,
):
    if action == "help":
        print(
            "Available actions: sync_db, sync_high_quality_db, tests, bench_embed, bench_query, bench_ann, embed_full, index_full, lexical_full, verify_state, sample_query, query_suite, persistent_query, persistent_query_suite, persistent_verify_state, phase23_high_quality, clear_results, clear_data_artifacts, clear_high_quality_data_artifacts"
        )
        return

    if action == "sync_db":
        src = Path(local_db)
        if not src.exists() or not src.is_dir():
            raise SystemExit(f"local DB directory not found: {src}")
        print(clear_synced_db.remote())
        with data_volume.batch_upload(force=True) as batch:
            batch.put_directory(str(src), "/crawl_data")
        print("Uploaded directory to volume path /crawl_data")
        return

    if action == "sync_high_quality_db":
        src = Path(local_db)
        if not src.exists() or not src.is_dir():
            raise SystemExit(f"local DB directory not found: {src}")
        print(clear_high_quality_synced_db.remote())
        with data_volume.batch_upload(force=True) as batch:
            batch.put_directory(str(src), "/crawl_data.high_quality")
        print("Uploaded directory to volume path /crawl_data.high_quality")
        return

    actions = {
        "tests": lambda: print(run_tests_cpu.remote(git_ref)),
        "bench_embed": lambda: print(bench_embed_gpu.remote(git_ref, samples)),
        "bench_query": lambda: print(bench_query_cpu.remote(git_ref)),
        "bench_ann": lambda: print(bench_ann_remote.remote(git_ref)),
        "embed_full": lambda: print(embed_full_gpu.remote(git_ref)),
        "index_full": lambda: print(index_full_cpu.remote(git_ref)),
        "lexical_full": lambda: print(lexical_full_cpu.remote(git_ref)),
        "verify_state": lambda: print(verify_remote_state.remote(git_ref)),
        "sample_query": lambda: print(
            sample_query_remote.remote(git_ref, query_text, top_k)
        ),
        "query_suite": lambda: print(
            query_suite_remote.remote(git_ref, queries_text, top_k)
        ),
        "phase23_high_quality": lambda: print(
            phase23_high_quality_remote.remote(git_ref)
        ),
        "persistent_query": lambda: print(_persistent_web_query_report(query_text, top_k)),
        "persistent_query_suite": lambda: print(
            _persistent_web_query_suite_report(queries_text, top_k)
        ),
        "persistent_verify_state": lambda: print(
            _deployed_persistent_session().verify_state.remote()
        ),
        "clear_results": lambda: print(clear_results.remote()),
        "clear_data_artifacts": lambda: print(clear_data_artifacts.remote()),
        "clear_high_quality_data_artifacts": lambda: print(
            clear_high_quality_data_artifacts.remote()
        ),
    }

    if action not in actions:
        raise SystemExit(f"unknown action: {action}")

    actions[action]()
