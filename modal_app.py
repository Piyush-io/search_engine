import os
import subprocess
from pathlib import Path

import modal

# -----------------------------------------------------------------------------
# Image
# -----------------------------------------------------------------------------
# Use Debian slim with explicit Python, then install Rust toolchain.
image = (
    modal.Image.debian_slim(python_version="3.11")
    .apt_install(
        "curl",
        "build-essential",
        "pkg-config",
        "libssl-dev",
        "git",
        "ca-certificates",
        "clang",
        "libclang-dev",
        "cmake",
    )
    .run_commands(
        "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y",
    )
    .env({"PATH": "/root/.cargo/bin:$PATH"})
)

app = modal.App("search-engine-indexer")
volume = modal.Volume.from_name("search-engine-data", create_if_missing=True)

REPO_DIR = "/search_engine"
DATA_MOUNT = "/data"


def _run(cmd: str, cwd: str | None = None, env: dict | None = None) -> None:
    print(f"\n$ {cmd}")
    subprocess.run(
        cmd,
        shell=True,
        check=True,
        cwd=cwd,
        env=env,
    )


def _ensure_repo() -> None:
    if not Path(REPO_DIR).exists():
        _run(f"git clone https://github.com/Piyush-io/search_engine.git {REPO_DIR}")
    else:
        _run("git fetch --all --prune", cwd=REPO_DIR)
        _run("git reset --hard origin/main", cwd=REPO_DIR)


def _ensure_data_layout() -> None:
    Path(f"{DATA_MOUNT}/crawl_data").mkdir(parents=True, exist_ok=True)


def _wire_paths_config() -> None:
    config_path = Path(REPO_DIR) / "config.toml"
    text = config_path.read_text()

    marker = "[paths]"
    if marker not in text:
        raise RuntimeError("config.toml missing [paths] section")

    head = text.split(marker)[0].rstrip() + "\n\n"
    new_paths = f"""[paths]
db_path = "{DATA_MOUNT}/crawl_data"
index_path = "{DATA_MOUNT}/hnsw_index.bin"
lexical_index_path = "{DATA_MOUNT}/lexical_index"
wiki_index_path = "{DATA_MOUNT}/wiki_hnsw.bin"
"""
    config_path.write_text(head + new_paths)
    print(f"Updated {config_path} paths to {DATA_MOUNT}")


@app.function(
    image=image,
    # Simplified function settings
    cpu=8,
    memory=65536,  # 64 GiB
    volumes={DATA_MOUNT: volume},
    timeout=3600 * 8,
)
def run_index_build() -> str:
    _ensure_repo()
    _ensure_data_layout()
    _wire_paths_config()

    env = os.environ.copy()
    env["MALLOC_ARENA_MAX"] = "2"
    env["RAYON_NUM_THREADS"] = "6"

    _run("cargo build --release --bin index --bin stats", cwd=REPO_DIR, env=env)
    _run("./target/release/stats", cwd=REPO_DIR, env=env)
    _run("./target/release/index", cwd=REPO_DIR, env=env)

    volume.commit()

    outputs = [
        f"{DATA_MOUNT}/hnsw_index.bin",
        f"{DATA_MOUNT}/hnsw_index.bin.hnsw.data",
        f"{DATA_MOUNT}/hnsw_index.bin.hnsw.graph",
    ]
    for p in outputs:
        path = Path(p)
        print(
            f"{p}: exists={path.exists()} size={path.stat().st_size if path.exists() else 0}"
        )

    return "Index build complete and committed to volume."


@app.local_entrypoint()
def main():
    result = run_index_build.remote()
    print(result)
