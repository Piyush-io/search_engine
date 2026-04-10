#!/bin/bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

echo "=========================================================="
echo " Incremental Search Engine Update"
echo "=========================================================="

echo "Building binaries..."
cargo build --release --bin crawl --bin normalize_pages --bin embed --bin index --bin lexical_index

echo "1. Crawling new pages..."
./target/release/crawl

echo "2. Normalizing changed pages..."
./target/release/normalize_pages

echo "3. Embedding queued chunks..."
./target/release/embed

echo "4. Updating vector delta index..."
./target/release/index

echo "5. Updating lexical index..."
./target/release/lexical_index

echo "=========================================================="
echo " Incremental update complete"
echo "=========================================================="
