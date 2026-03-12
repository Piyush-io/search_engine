import modal

# Optimized image for Rust + CUDA + OpenSSL + RocksDB (LLVM/Clang)
image = (
    modal.Image.debian_slim()
    .apt_install(
        "curl", 
        "build-essential", 
        "pkg-config", 
        "libssl-dev", 
        "git", 
        "ca-certificates",
        "clang",              # Required for bindgen (rocksdb)
        "libclang-dev",       # Required for bindgen (rocksdb)
        "cmake"               # Often helpful for C++ based crates
    )
    .run_commands(
        "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y",
        "echo 'source $HOME/.cargo/env' >> $HOME/.bashrc"
    )
    .env({"PATH": "/root/.cargo/bin:$PATH"})
    .run_commands("rustup default stable")
)

app = modal.App("search-engine-indexer")
volume = modal.Volume.from_name("search-engine-data")

@app.function(
    image=image,
    gpu="A10G", 
    volumes={"/data": volume},
    timeout=3600 * 4,
)
def run_pipeline():
    pass
