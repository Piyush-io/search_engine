#!/bin/bash

# A script to run the search engine processing pipeline.
# TRULY SEQUENTIAL to avoid RocksDB Lock errors.

echo "=========================================================="
echo " Starting Search Engine Processing Pipeline (Sequential)"
echo "=========================================================="

# Ensure we build them first
echo "Building binaries..."
cargo build --release --bin normalize_pages --bin embed --bin wiki_ingest --bin lexical_index --bin index --bin wiki_embed --bin wiki_index

# Check for DB lock from a forgotten crawler
if [ -f "./crawl_data/LOCK" ]; then
    echo "Check: Is the crawler or another process still running?"
fi

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
