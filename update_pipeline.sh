#!/bin/bash
set -euo pipefail
exec "$(dirname "$0")/scripts/update_pipeline.sh" "$@"
