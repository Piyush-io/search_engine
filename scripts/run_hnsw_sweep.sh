#!/bin/bash
# HNSW Parameter Sweep Script
# Runs systematic experiments across HNSW configurations

set -e

# Configuration
REPORTS_DIR="reports/hnsw_sweep"
CONFIGS_DIR="."
QRELS_PATH="${QRELS_PATH:-eval/qrels.niche.txt}"
QUERIES_PATH="${QUERIES_PATH:-eval/queries.niche.txt}"

# HNSW Config Profiles
PROFILES=(
    "config.hnsw_baseline.toml"
    "config.hnsw_m12_ef120.toml"
    "config.hnsw_high_quality.toml"
    "config.hnsw_max_quality.toml"
)

# Profile names for display
PROFILE_NAMES=(
    "baseline"
    "m12_ef120"
    "high_quality"
    "max_quality"
)

echo "=========================================="
echo "HNSW Parameter Sweep"
echo "=========================================="
echo "Reports will be saved to: $REPORTS_DIR"
echo ""

# Create reports directory
mkdir -p "$REPORTS_DIR"

# Function to extract metrics from eval_results.json
extract_metrics() {
    local json_file="$1"
    if [[ -f "$json_file" ]]; then
        cat "$json_file"
    else
        echo '{"error": "No results file found"}'
    fi
}

# Function to run a single profile experiment
run_profile() {
    local config_file="$1"
    local profile_name="$2"
    local report_file="$REPORTS_DIR/${profile_name}.json"
    
    echo "----------------------------------------"
    echo "Profile: $profile_name"
    echo "Config: $config_file"
    echo "----------------------------------------"
    
    # Export config path for this run
    export SEARCH_ENGINE_CONFIG_PATH="$config_file"
    
    # Record start time
    local start_time=$(date +%s)
    local start_datetime=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    
    echo "[$(date +%H:%M:%S)] Step 1: Building index..."
    local index_start=$(date +%s)
    
    # Clean and rebuild index
    cargo run --release --bin index -- --full 2>&1 | tee /tmp/index_${profile_name}.log
    local index_status=${PIPESTATUS[0]}
    
    local index_end=$(date +%s)
    local build_time=$((index_end - index_start))
    
    if [[ $index_status -ne 0 ]]; then
        echo "[$(date +%H:%M:%S)] ERROR: Index build failed for $profile_name"
        cat > "$report_file" << EOF
{
  "profile": "$profile_name",
  "config": "$config_file",
  "timestamp": "$start_datetime",
  "error": "Index build failed",
  "build_time_seconds": $build_time
}
EOF
        return 1
    fi
    
    echo "[$(date +%H:%M:%S)] Step 2: Running evaluation..."
    local eval_start=$(date +%s)
    
    # Run evaluation
    if [[ -f "$QRELS_PATH" && -f "$QUERIES_PATH" ]]; then
        cargo run --release --bin eval -- \
            --qrels "$QRELS_PATH" \
            --queries "$QUERIES_PATH" \
            --k 1,3,5,10,20 2>&1 | tee /tmp/eval_${profile_name}.log
        local eval_status=${PIPESTATUS[0]}
    else
        echo "[$(date +%H:%M:%S)] Warning: Qrels or queries file not found, skipping eval"
        echo "Expected: $QRELS_PATH and $QUERIES_PATH"
        local eval_status=1
    fi
    
    local eval_end=$(date +%s)
    local eval_time=$((eval_end - eval_start))
    
    # Extract metrics
    local metrics=$(extract_metrics "reports/eval_results.json")
    
    # Get index file size
    local index_path=$(grep "index_path" "$config_file" | head -1 | cut -d'"' -f2)
    local index_size=0
    if [[ -f "$index_path" ]]; then
        index_size=$(stat -f%z "$index_path" 2>/dev/null || stat -c%s "$index_path" 2>/dev/null || echo 0)
    fi
    
    # Create report
    cat > "$report_file" << EOF
{
  "profile": "$profile_name",
  "config": "$config_file",
  "timestamp": "$start_datetime",
  "timing": {
    "build_time_seconds": $build_time,
    "eval_time_seconds": $eval_time,
    "total_time_seconds": $((index_end - start_time))
  },
  "metrics": $metrics,
  "index_size_bytes": $index_size
}
EOF
    
    echo "[$(date +%H:%M:%S)] Profile $profile_name complete"
    echo "  Build time: ${build_time}s"
    echo "  Eval time: ${eval_time}s"
    echo "  Report: $report_file"
    echo ""
    
    return 0
}

# Main sweep loop
echo "Starting HNSW sweep with ${#PROFILES[@]} profiles..."
echo ""

for i in "${!PROFILES[@]}"; do
    run_profile "${PROFILES[$i]}" "${PROFILE_NAMES[$i]}"
done

# Generate summary report
echo "=========================================="
echo "Generating summary report..."
echo "=========================================="

SUMMARY_FILE="$REPORTS_DIR/summary.md"

cat > "$SUMMARY_FILE" << 'EOF'
# HNSW Parameter Sweep Results

## Summary

| Profile | m | ef_construction | ef_search | Build Time | Index Size | MRR | NDCG@10 | Recall@10 |
|---------|---|-----------------|-----------|------------|------------|-----|---------|-----------|
EOF

for i in "${!PROFILES[@]}"; do
    profile="${PROFILE_NAMES[$i]}"
    config="${PROFILES[$i]}"
    report_file="$REPORTS_DIR/${profile}.json"
    
    if [[ -f "$report_file" ]]; then
        # Extract values using jq if available, otherwise use grep/sed
        if command -v jq &> /dev/null; then
            m=$(jq -r '.metrics.m // "N/A"' "$report_file" 2>/dev/null || echo "N/A")
            ef_construction=$(jq -r '.metrics.ef_construction // "N/A"' "$report_file" 2>/dev/null || echo "N/A")
            ef_search=$(jq -r '.metrics.ef_search // "N/A"' "$report_file" 2>/dev/null || echo "N/A")
            build_time=$(jq -r '.timing.build_time_seconds // "N/A"' "$report_file")
            index_size=$(jq -r '.index_size_bytes // 0' "$report_file")
            mrr=$(jq -r '.metrics.mrr // "N/A"' "$report_file")
            ndcg10=$(jq -r '.metrics.ndcg_at[] | select(.[0] == 10) | .[1] // "N/A"' "$report_file" 2>/dev/null || echo "N/A")
            recall10=$(jq -r '.metrics.recall_at[] | select(.[0] == 10) | .[1] // "N/A"' "$report_file" 2>/dev/null || echo "N/A")
            
            # Format index size
            if [[ "$index_size" != "0" && "$index_size" != "N/A" ]]; then
                index_size_mb=$((index_size / 1024 / 1024))
                index_size_str="${index_size_mb}MB"
            else
                index_size_str="N/A"
            fi
            
            # Format build time
            if [[ "$build_time" != "N/A" ]]; then
                build_time_str="${build_time}s"
            else
                build_time_str="N/A"
            fi
            
            # Extract m, ef_construction, ef_search from config if not in metrics
            if [[ "$m" == "N/A" ]]; then
                m=$(grep "^m = " "$config" | head -1 | sed 's/m = //' | tr -d ' ')
            fi
            if [[ "$ef_construction" == "N/A" ]]; then
                ef_construction=$(grep "^ef_construction = " "$config" | head -1 | sed 's/ef_construction = //' | tr -d ' ')
            fi
            if [[ "$ef_search" == "N/A" ]]; then
                ef_search=$(grep "^ef_search = " "$config" | head -1 | sed 's/ef_search = //' | tr -d ' ')
            fi
            
            echo "| $profile | $m | $ef_construction | $ef_search | $build_time_str | $index_size_str | $mrr | $ndcg10 | $recall10 |" >> "$SUMMARY_FILE"
        else
            echo "| $profile | - | - | - | - | - | - | - | - |" >> "$SUMMARY_FILE"
        fi
    else
        echo "| $profile | - | - | - | - | - | - | - | - |" >> "$SUMMARY_FILE"
    fi
done

cat >> "$SUMMARY_FILE" << 'EOF'

## Configuration Details

### Profiles Tested

1. **baseline**: Current settings (m=8, ef_construction=80, ef_search=120)
2. **m12_ef120**: Medium quality (m=12, ef_construction=120, ef_search=120)
3. **high_quality**: High build quality (m=16, ef_construction=200, ef_search=120)
4. **max_quality**: Maximum quality (m=16, ef_construction=200, ef_search=200)

## Parameter Explanations

- **m**: Number of connections per node. Higher = more accurate but larger index.
- **ef_construction**: Size of dynamic candidate list during build. Higher = better graph quality.
- **ef_search**: Size of dynamic candidate list during search. Higher = better recall but slower queries.

## Recommendations

See `docs/hnsw-tuning-report.md` for detailed recommendations.

## Raw Data

Individual profile results are in JSON files in this directory.
EOF

echo ""
echo "=========================================="
echo "Sweep Complete!"
echo "=========================================="
echo "Summary: $SUMMARY_FILE"
echo "Individual reports: $REPORTS_DIR/"
echo ""
echo "To view results:"
echo "  cat $SUMMARY_FILE"
echo ""
