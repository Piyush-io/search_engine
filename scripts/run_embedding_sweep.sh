#!/bin/bash
#
# Embedding Sweep Runner
# 
# Runs embedding experiments across different profiles to compare:
# - Throughput (items/sec)
# - Quality (MRR, NDCG from eval)
# - Resource usage
#
# Usage:
#   ./scripts/run_embedding_sweep.sh [profile_name]
#   ./scripts/run_embedding_sweep.sh              # Run all profiles
#   ./scripts/run_embedding_sweep.sh bge_small_fast   # Run single profile
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
REPORTS_DIR="$PROJECT_DIR/reports/embedding_sweep"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Available profiles
PROFILES=(
    "embed_bge_small_fast"
    "embed_bge_small_quality"
    "embed_bge_base_quality"
    "embed_cpu_fallback"
)

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Create reports directory
mkdir -p "$REPORTS_DIR"

# Parse arguments
SINGLE_PROFILE=""
if [ $# -ge 1 ]; then
    SINGLE_PROFILE="$1"
    log_info "Running single profile: $SINGLE_PROFILE"
fi

# Validate profile if specified
if [ -n "$SINGLE_PROFILE" ]; then
    VALID_PROFILE=false
    for p in "${PROFILES[@]}"; do
        if [ "$p" == "$SINGLE_PROFILE" ]; then
            VALID_PROFILE=true
            break
        fi
    done
    if [ "$VALID_PROFILE" = false ]; then
        log_error "Invalid profile: $SINGLE_PROFILE"
        log_info "Valid profiles: ${PROFILES[*]}"
        exit 1
    fi
    PROFILES=("$SINGLE_PROFILE")
fi

# Function to check if crawl data exists
check_crawl_data() {
    local profile=$1
    local config_file="$PROJECT_DIR/config.${profile}.toml"
    
    if [ ! -f "$config_file" ]; then
        log_error "Config file not found: $config_file"
        return 1
    fi
    
    # Extract db_path from config
    local db_path=$(grep "^db_path" "$config_file" | head -1 | sed 's/.*= *"\(.*\)".*/\1/')
    
    if [ -z "$db_path" ]; then
        log_warn "Could not extract db_path from config"
        return 1
    fi
    
    # Check if database exists (look for CURRENT file in RocksDB)
    if [ -f "$db_path/CURRENT" ]; then
        return 0
    fi
    
    return 1
}

# Function to run a single profile experiment
run_profile() {
    local profile=$1
    local config_file="$PROJECT_DIR/config.${profile}.toml"
    local timing_report="$REPORTS_DIR/${profile}_${TIMESTAMP}.json"
    local eval_report="$REPORTS_DIR/${profile}_eval_${TIMESTAMP}.json"
    local summary_report="$REPORTS_DIR/${profile}_summary_${TIMESTAMP}.json"
    
    log_info "=========================================="
    log_info "Running profile: $profile"
    log_info "Config: $config_file"
    log_info "=========================================="
    
    # Check if crawl data exists
    if ! check_crawl_data "$profile"; then
        log_warn "No crawl data found for $profile"
        log_info "You need to crawl data first with this config:"
        log_info "  SEARCH_ENGINE_CONFIG_PATH=$config_file cargo run --release --bin crawl"
        return 1
    fi
    
    # Set config path
    export SEARCH_ENGINE_CONFIG_PATH="$config_file"
    
    log_info "Step 1/3: Running embedding (--full-scan)..."
    local embed_start=$(date +%s)
    
    # Run embedding with timing output
    if ! cargo run --release --bin embed -- --full-scan --timing "$timing_report" 2>&1 | tee "$REPORTS_DIR/${profile}_embed_${TIMESTAMP}.log"; then
        log_error "Embedding failed for $profile"
        return 1
    fi
    
    local embed_end=$(date +%s)
    local embed_duration=$((embed_end - embed_start))
    log_success "Embedding completed in ${embed_duration}s"
    
    # Extract timing stats from the log
    local throughput=$(grep "throughput" "$REPORTS_DIR/${profile}_embed_${TIMESTAMP}.log" | tail -1 | grep -oE '[0-9]+\.?[0-9]*' | tail -1 || echo "0")
    
    log_info "Step 2/3: Running indexing (--full)..."
    local index_start=$(date +%s)
    
    if ! cargo run --release --bin index -- --full 2>&1 | tee "$REPORTS_DIR/${profile}_index_${TIMESTAMP}.log"; then
        log_error "Indexing failed for $profile"
        return 1
    fi
    
    local index_end=$(date +%s)
    local index_duration=$((index_end - index_start))
    log_success "Indexing completed in ${index_duration}s"
    
    log_info "Step 3/3: Running evaluation..."
    local eval_start=$(date +%s)
    
    # Check if benchmark files exist
    local queries_file="$PROJECT_DIR/benchmarks/niche_db/queries_100.tsv"
    local qrels_file="$PROJECT_DIR/benchmarks/niche_db/qrels_100.tsv"
    
    if [ ! -f "$queries_file" ] || [ ! -f "$qrels_file" ]; then
        log_warn "Benchmark files not found:"
        log_warn "  Queries: $queries_file"
        log_warn "  Qrels: $qrels_file"
        log_warn "Skipping evaluation..."
        
        # Create placeholder summary
        cat > "$summary_report" << EOF
{
  "profile": "$profile",
  "timestamp": "$TIMESTAMP",
  "config": "$config_file",
  "embedding": {
    "wall_time_secs": $embed_duration,
    "throughput_items_per_sec": null,
    "report_path": "$timing_report"
  },
  "indexing": {
    "wall_time_secs": $index_duration
  },
  "evaluation": {
    "skipped": true,
    "reason": "benchmark files not found"
  }
}
EOF
        return 0
    fi
    
    # Run evaluation
    if ! cargo run --release --bin eval -- \
        --queries "$queries_file" \
        --qrels "$qrels_file" \
        2>&1 | tee "$REPORTS_DIR/${profile}_eval_${TIMESTAMP}.log"; then
        log_warn "Evaluation failed for $profile (non-fatal)"
    fi
    
    local eval_end=$(date +%s)
    local eval_duration=$((eval_end - eval_start))
    log_success "Evaluation completed in ${eval_duration}s"
    
    # Extract metrics from eval log
    local mrr=$(grep "MRR:" "$REPORTS_DIR/${profile}_eval_${TIMESTAMP}.log" | grep -oE '[0-9]+\.[0-9]+' | head -1 || echo "null")
    local ndcg5=$(grep "NDCG@5:" "$REPORTS_DIR/${profile}_eval_${TIMESTAMP}.log" | grep -oE '[0-9]+\.[0-9]+' | head -1 || echo "null")
    local ndcg10=$(grep "NDCG@10:" "$REPORTS_DIR/${profile}_eval_${TIMESTAMP}.log" | grep -oE '[0-9]+\.[0-9]+' | head -1 || echo "null")
    
    # Extract query latency (rough estimate from eval output)
    local query_latency=$(grep -oE '[0-9]+\.?[0-9]*ms' "$REPORTS_DIR/${profile}_eval_${TIMESTAMP}.log" | tail -1 || echo "null")
    
    # Create summary report
    cat > "$summary_report" << EOF
{
  "profile": "$profile",
  "timestamp": "$TIMESTAMP",
  "config": "$config_file",
  "embedding": {
    "wall_time_secs": $embed_duration,
    "throughput_items_per_sec": ${throughput:-null},
    "report_path": "$timing_report"
  },
  "indexing": {
    "wall_time_secs": $index_duration
  },
  "evaluation": {
    "wall_time_secs": $eval_duration,
    "mrr": ${mrr:-null},
    "ndcg_at_5": ${ndcg5:-null},
    "ndcg_at_10": ${ndcg10:-null},
    "approx_query_latency": "${query_latency:-null}",
    "log_path": "$REPORTS_DIR/${profile}_eval_${TIMESTAMP}.log"
  }
}
EOF
    
    log_success "Summary saved to: $summary_report"
    return 0
}

# Main execution
log_info "Starting embedding sweep..."
log_info "Reports will be saved to: $REPORTS_DIR"
log_info "Timestamp: $TIMESTAMP"
log_info "Profiles to run: ${PROFILES[*]}"

# Track results
SUCCESS_COUNT=0
FAILED_COUNT=0

for profile in "${PROFILES[@]}"; do
    if run_profile "$profile"; then
        ((SUCCESS_COUNT++))
    else
        ((FAILED_COUNT++))
        log_error "Profile $profile failed"
    fi
    echo ""
done

# Create aggregate summary
log_info "=========================================="
log_info "Sweep Summary"
log_info "=========================================="
log_info "Successful: $SUCCESS_COUNT"
log_info "Failed: $FAILED_COUNT"
log_info "Total: ${#PROFILES[@]}"
log_info "Reports directory: $REPORTS_DIR"

# List all summary files
log_info "Summary files:"
for f in "$REPORTS_DIR"/*_summary_*.json; do
    if [ -f "$f" ]; then
        echo "  - $f"
    fi
done

log_success "Sweep complete!"

# Provide next steps
log_info ""
log_info "Next steps:"
log_info "  1. Review individual reports in: $REPORTS_DIR"
log_info "  2. Compare quality metrics across profiles"
log_info "  3. Update docs/embedding-sweep-report.md with findings"
