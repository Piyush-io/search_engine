import modal
import os

# Use the same image as our app
image = (
    modal.Image.debian_slim()
    .apt_install(
        "curl", "build-essential", "pkg-config", "libssl-dev", "git", "ca-certificates",
        "clang", "libclang-dev", "cmake"
    )
    .run_commands("curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y")
    .env({"PATH": "/root/.cargo/bin:$PATH"})
)

app = modal.App("investigator")

@app.function(image=image)
def investigate():
    print("Cloning and building to find where libonnxruntime.so goes...")
    os.system("git clone https://github.com/Piyush-io/search_engine.git /investigate")
    os.chdir("/investigate")
    # Only build embed to save time
    os.system("cargo build --release --bin embed")
    print("\nSearching for libonnxruntime.so:")
    os.system("find / -name libonnxruntime.so 2>/dev/null")

if __name__ == "__main__":
    with app.run():
        investigate.remote()
