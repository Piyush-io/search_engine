#!/bin/bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONFIG_PATH="${1:-${SEARCH_ENGINE_CONFIG_PATH:-$REPO_ROOT/config.niche.toml}}"

export SEARCH_ENGINE_CONFIG_PATH="$CONFIG_PATH"

cd "$REPO_ROOT"

if [ ! -f "$SEARCH_ENGINE_CONFIG_PATH" ]; then
    echo "Error: config file not found: $SEARCH_ENGINE_CONFIG_PATH"
    exit 1
fi

echo "=========================================================="
echo " Starting Remote Embed Server"
echo "=========================================================="
echo "Config: $SEARCH_ENGINE_CONFIG_PATH"

cargo run --release --bin embed_server
