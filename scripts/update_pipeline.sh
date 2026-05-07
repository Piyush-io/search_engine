#!/bin/bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONFIG_PATH="${1:-${SEARCH_ENGINE_CONFIG_PATH:-$REPO_ROOT/config.toml}}"

export SEARCH_ENGINE_CONFIG_PATH="$CONFIG_PATH"

cd "$REPO_ROOT"

if [ ! -f "$SEARCH_ENGINE_CONFIG_PATH" ]; then
    echo "Error: config file not found: $SEARCH_ENGINE_CONFIG_PATH"
    exit 1
fi

echo "=========================================================="
echo " Incremental Search Engine Update"
echo "=========================================================="
echo "Config: $SEARCH_ENGINE_CONFIG_PATH"

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
