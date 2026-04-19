#!/bin/bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONFIG_PATH="${1:-$REPO_ROOT/config.high_quality.toml}"

export SEARCH_ENGINE_CONFIG_PATH="$CONFIG_PATH"

cd "$REPO_ROOT"

echo "=========================================================="
echo " Fresh High-Quality Search Corpus Build"
echo "=========================================================="
echo "Config: $SEARCH_ENGINE_CONFIG_PATH"

echo "Building binaries..."
cargo build --release --bin crawl --bin normalize_pages --bin embed --bin index --bin lexical_index --bin stats --bin queue_stats --bin index_stats --bin sample_query

echo "1. Crawling fresh high-quality pages..."
./target/release/crawl

echo "2. Normalizing pages into chunks..."
./target/release/normalize_pages

echo "3. Embedding all chunks from the clean corpus..."
./target/release/embed --full-scan

echo "4. Building a fresh base vector index..."
./target/release/index --full

echo "5. Building a fresh lexical index..."
./target/release/lexical_index --full

echo "6. Final stats..."
./target/release/stats
./target/release/queue_stats
./target/release/index_stats

echo "7. Representative sample query..."
./target/release/sample_query "what is a B-tree" 5

echo "=========================================================="
echo " Fresh high-quality corpus build complete"
echo "=========================================================="
