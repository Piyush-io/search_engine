#!/bin/bash
set -euo pipefail
exec "$(dirname "$0")/scripts/start_pipeline.sh" "$@"
