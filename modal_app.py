import modal

# CUDA base image so ort can use the A10G GPU
image = (
    modal.Image.from_registry("nvidia/cuda:12.1.1-runtime-ubuntu22.04")
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
    .run_commands(
        "rustup default stable",
        "git clone https://github.com/Piyush-io/search_engine.git /search_engine",
    )
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
