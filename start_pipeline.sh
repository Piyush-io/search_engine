#!/bin/bash
set -euo pipefail

# A script to run the search engine processing pipeline.
# TRULY SEQUENTIAL to avoid RocksDB Lock errors.

echo "=========================================================="
echo " Starting Search Engine Processing Pipeline (Sequential)"
echo "=========================================================="

DB_PATH="./crawl_data"

# Validate RocksDB path before doing any work.
if [ -e "$DB_PATH" ] && [ ! -d "$DB_PATH" ]; then
    echo "Error: $DB_PATH exists but is not a directory."
    echo "It looks like a log/text file was written to the database path."
    echo "Move or rename that file, then recreate $DB_PATH as a directory."
    exit 1
fi

mkdir -p "$DB_PATH"

# Ensure we build them first
echo "Building binaries..."
cargo build --release --bin normalize_pages --bin embed --bin wiki_ingest --bin lexical_index --bin index --bin wiki_embed --bin wiki_index

# --- FIX FOR ONNX RUNTIME SHARED LIBRARIES ---
# ort crate downloads libonnxruntime.so into ~/.cache/ort.pyke.io/ during build.
# The binary is dynamically linked, so we must add that cache dir to LD_LIBRARY_PATH.
echo "Configuring library paths..."
ONNX_DIR=$(find "$HOME/.cache" /root/.cache /tmp . -name "libonnxruntime.so" -exec dirname {} \; 2>/dev/null | head -n 1 || true)
if [ -n "$ONNX_DIR" ]; then
    FULL_ONNX_PATH=$(realpath "$ONNX_DIR")
    export LD_LIBRARY_PATH="$FULL_ONNX_PATH${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    echo "Set LD_LIBRARY_PATH to $FULL_ONNX_PATH"
else
    echo "Warning: Could not find libonnxruntime.so — embed will fail"
fi
# ---------------------------------------------

# A RocksDB LOCK file can remain on disk even when no process is using the DB.
# Let RocksDB itself decide whether the lock is actually active.

echo "1. Normalizing Pages (HTML -> Chunks)..."
./target/release/normalize_pages

echo "2. GPU Batch Embedding (Chunks -> Vectors)..."
./target/release/embed

echo "3. Building HNSW Vector Index..."
./target/release/index

echo "4. Building Lexical (BM25) Index..."
./target/release/lexical_index

if [ -f "training/wiki_summaries.jsonl" ]; then
    echo "5. Ingesting Wikipedia..."
    ./target/release/wiki_ingest training/wiki_summaries.jsonl
    ./target/release/wiki_embed
    ./target/release/wiki_index
fi

echo "=========================================================="
echo " Pipeline Complete! Run the web server:"
echo " cargo run --release --bin search_engine"
echo "=========================================================="
