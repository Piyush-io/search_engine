#!/bin/bash
#
# Niche Tuning Loop - Diagnostics-Driven Auto-Tuning
#
# Runs multiple config variants for the niche profile, evaluates each,
# calculates weighted scores, and generates ranked recommendations.
#
# Usage:
#   ./scripts/run_niche_tuning_loop.sh [options]
#
# Options:
#   --quick               Run with 10-query set for fast iteration
#   --full                Run with 100-query set for comprehensive evaluation (default)
#   --variants <list>     Comma-separated list of variant names to run
#   --skip-eval           Skip evaluation, only generate reports from existing results
#   --baseline-only       Only run baseline for comparison
#
# Examples:
#   ./scripts/run_niche_tuning_loop.sh                    # Run all variants with 100 queries
#   ./scripts/run_niche_tuning_loop.sh --quick          # Quick validation with 10 queries
#   ./scripts/run_niche_tuning_loop.sh --variants baseline,high_identifier_boost
#

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
CONFIGS_DIR="$PROJECT_DIR/configs/tuning"
REPORTS_DIR="$PROJECT_DIR/reports/niche_tuning"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)

# Default settings
QUERY_SET="full"
VARIANTS_TO_RUN=""
SKIP_EVAL=false
BASELINE_ONLY=false

# Default benchmark paths
QUERIES_FULL="$PROJECT_DIR/benchmarks/niche_db/queries_100.tsv"
QRELS_FULL="$PROJECT_DIR/benchmarks/niche_db/qrels_100.tsv"
QUERIES_QUICK="$PROJECT_DIR/benchmarks/niche_db/queries_10.tsv"
QRELS_QUICK="$PROJECT_DIR/benchmarks/niche_db/qrels_10.tsv"

# All available variants
ALL_VARIANTS=(
    "baseline"
    "high_identifier_boost"
    "high_acronym_boost"
    "aggressive_host_policy"
    "balanced_combo"
    "max_precision"
    "conservative_safe"
    "vector_focused"
    "lexical_heavy"
)

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN[SUCCESS]}${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_section() {
    echo -e "${CYAN}\n========================================${NC}"
    echo -e "${CYAN}$1${NC}"
    echo -e "${CYAN}========================================${NC}\n"
}

# Parse arguments
parse_args() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            --quick)
                QUERY_SET="quick"
                shift
                ;;
            --full)
                QUERY_SET="full"
                shift
                ;;
            --variants)
                VARIANTS_TO_RUN="$2"
                shift 2
                ;;
                --skip-eval)
                SKIP_EVAL=true
                shift
                ;;
            --baseline-only)
                BASELINE_ONLY=true
                VARIANTS_TO_RUN="baseline"
                shift
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                show_help
                exit 1
                ;;
        esac
    done
}

show_help() {
    cat << 'EOF'
Niche Tuning Loop - Diagnostics-Driven Auto-Tuning

Usage: ./scripts/run_niche_tuning_loop.sh [options]

Options:
  --quick               Run with 10-query set for fast iteration
  --full                Run with 100-query set for comprehensive evaluation (default)
  --variants <list>     Comma-separated list of variant names to run
  --skip-eval           Skip evaluation, only generate reports from existing results
  --baseline-only       Only run baseline for comparison
  -h, --help            Show this help message

Examples:
  ./scripts/run_niche_tuning_loop.sh                    # Run all variants with 100 queries
  ./scripts/run_niche_tuning_loop.sh --quick          # Quick validation with 10 queries
  ./scripts/run_niche_tuning_loop.sh --variants baseline,high_identifier_boost
EOF
}

# Setup directories and validate environment
setup() {
    log_section "Niche Tuning Loop Setup"
    
    # Create reports directory
    mkdir -p "$REPORTS_DIR"
    
    # Check Python scoring script
    if [[ ! -f "$SCRIPT_DIR/score_variant.py" ]]; then
        log_error "Scoring script not found: $SCRIPT_DIR/score_variant.py"
        exit 1
    fi
    
    # Set benchmark paths based on query set
    if [[ "$QUERY_SET" == "quick" ]]; then
        QUERIES_PATH="$QUERIES_QUICK"
        QRELS_PATH="$QRELS_QUICK"
        log_info "Using quick query set (10 queries)"
    else
        QUERIES_PATH="$QUERIES_FULL"
        QRELS_PATH="$QRELS_FULL"
        log_info "Using full query set (100 queries)"
    fi
    
    # Validate benchmark files exist
    if [[ ! -f "$QUERIES_PATH" ]]; then
        log_warn "Queries file not found: $QUERIES_PATH"
        log_warn "Evaluation will be skipped for each variant"
    fi
    
    if [[ ! -f "$QRELS_PATH" ]]; then
        log_warn "Qrels file not found: $QRELS_PATH"
        log_warn "Evaluation will be skipped for each variant"
    fi
    
    # Determine which variants to run
    if [[ -n "$VARIANTS_TO_RUN" ]]; then
        IFS=',' read -ra VARIANTS <<< "$VARIANTS_TO_RUN"
        log_info "Running specific variants: ${VARIANTS[*]}"
    elif [[ "$BASELINE_ONLY" == true ]]; then
        VARIANTS=("baseline")
        log_info "Running baseline only"
    else
        VARIANTS=("${ALL_VARIANTS[@]}")
        log_info "Running all ${#VARIANTS[@]} variants"
    fi
    
    # Validate config files exist
    local missing_configs=()
    for variant in "${VARIANTS[@]}"; do
        local config_file="$CONFIGS_DIR/${variant}.toml"
        if [[ ! -f "$config_file" ]]; then
            missing_configs+=("$variant")
        fi
    done
    
    if [[ ${#missing_configs[@]} -gt 0 ]]; then
        log_error "Missing config files for: ${missing_configs[*]}"
        exit 1
    fi
    
    log_success "Setup complete. Reports will be saved to: $REPORTS_DIR"
}

# Check if index exists for a given config
check_index_exists() {
    local config_file="$1"
    
    # Extract index path from config
    local index_path=$(grep "^index_path" "$config_file" | head -1 | sed 's/.*= *"\(.*\)".*/\1/')
    index_path="${index_path#./}"  # Remove leading ./
    index_path="$PROJECT_DIR/$index_path"
    
    if [[ -f "$index_path" ]]; then
        return 0
    else
        return 1
    fi
}

# Run evaluation for a single variant
run_variant() {
    local variant="$1"
    local config_file="$CONFIGS_DIR/${variant}.toml"
    local report_file="$REPORTS_DIR/${variant}_${TIMESTAMP}.json"
    local log_file="$REPORTS_DIR/${variant}_${TIMESTAMP}.log"
    
    log_section "Running Variant: $variant"
    log_info "Config: $config_file"
    log_info "Report: $report_file"
    
    # Record timing
    local start_time=$(date +%s)
    local start_datetime=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    
    # Check if we should skip this variant
    if [[ "$SKIP_EVAL" == true ]]; then
        log_info "Skipping evaluation (--skip-eval specified)"
        
        # Check for existing report
        local existing_report=$(ls -t "$REPORTS_DIR/${variant}"_*.json 2>/dev/null | head -1)
        if [[ -n "$existing_report" ]]; then
            log_info "Using existing report: $existing_report"
            report_file="$existing_report"
        else
            log_warn "No existing report found for $variant"
            return 1
        fi
        
        local end_time=$(date +%s)
        local elapsed=$((end_time - start_time))
        
        # Create summary entry
        echo "${variant}|${report_file}|${elapsed}|skipped"
        return 0
    fi
    
    # Set config path
    export SEARCH_ENGINE_CONFIG_PATH="$config_file"
    
    # Check index exists (but don't rebuild - tuning loop assumes index is built)
    if ! check_index_exists "$config_file"; then
        log_warn "Index not found for $variant"
        log_warn "Run indexing first: SEARCH_ENGINE_CONFIG_PATH=$config_file cargo run --release --bin index -- --full"
        
        # Create error report
        cat > "$report_file" << EOF
{
  "variant": "$variant",
  "config": "$config_file",
  "timestamp": "$start_datetime",
  "status": "error",
  "error": "Index not found. Please build index first.",
  "timing": {
    "elapsed_seconds": 0
  }
}
EOF
        return 1
    fi
    
    log_info "Index found, proceeding with evaluation..."
    
    # Run evaluation
    local eval_status=0
    local metrics_json="null"
    
    if [[ -f "$QUERIES_PATH" && -f "$QRELS_PATH" ]]; then
        log_info "Running evaluation with $QUERY_SET query set..."
        
        if cargo run --release --bin eval -- \
            --queries "$QUERIES_PATH" \
            --qrels "$QRELS_PATH" \
            --k 1,3,5,10,20 2>&1 | tee "$log_file"; then
            
            # Extract metrics from eval_results.json
            local eval_results="$PROJECT_DIR/reports/eval_results.json"
            if [[ -f "$eval_results" ]]; then
                metrics_json=$(cat "$eval_results")
                log_success "Evaluation complete"
            else
                log_warn "eval_results.json not found"
                eval_status=1
            fi
        else
            log_error "Evaluation failed"
            eval_status=1
        fi
    else
        log_warn "Benchmark files not available, skipping evaluation"
        eval_status=1
    fi
    
    local end_time=$(date +%s)
    local elapsed=$((end_time - start_time))
    
    # Create comprehensive report
    cat > "$report_file" << EOF
{
  "variant": "$variant",
  "config": "$config_file",
  "timestamp": "$start_datetime",
  "status": "$(if [[ $eval_status -eq 0 ]]; then echo "success"; else echo "eval_failed"; fi)",
  "query_set": "$QUERY_SET",
  "queries_file": "$QUERIES_PATH",
  "qrels_file": "$QRELS_PATH",
  "timing": {
    "elapsed_seconds": $elapsed,
    "start_time": "$start_datetime"
  },
  "metrics": $metrics_json,
  "log_file": "$log_file"
}
EOF
    
    log_success "Report saved: $report_file"
    echo "${variant}|${report_file}|${elapsed}|$(if [[ $eval_status -eq 0 ]]; then echo "success"; else echo "failed"; fi)"
    
    return $eval_status
}

# Score all variants and generate ranked list
score_variants() {
    local results=($1)
    
    log_section "Scoring Variants"
    
    # Find baseline report
    local baseline_report=""
    for result in "${results[@]}"; do
        IFS='|' read -r variant report_file elapsed status <<< "$result"
        if [[ "$variant" == "baseline" && "$status" == "success" ]]; then
            baseline_report="$report_file"
            break
        fi
    done
    
    if [[ -z "$baseline_report" ]]; then
        log_warn "Baseline report not found, scoring without baseline comparison"
    else
        log_info "Using baseline for comparison: $baseline_report"
    fi
    
    # Score each variant
    local scored_variants=()
    
    for result in "${results[@]}"; do
        IFS='|' read -r variant report_file elapsed status <<< "$result"
        
        if [[ "$status" != "success" && "$status" != "skipped" ]]; then
            log_warn "Skipping scoring for $variant (status: $status)"
            continue
        fi
        
        local score_file="$REPORTS_DIR/${variant}_${TIMESTAMP}_scored.json"
        
        # Run scoring
        if [[ -n "$baseline_report" && "$variant" != "baseline" ]]; then
            python3 "$SCRIPT_DIR/score_variant.py" "$report_file" "$baseline_report" --output "$score_file" 2>/dev/null || true
        else
            python3 "$SCRIPT_DIR/score_variant.py" "$report_file" --output "$score_file" 2>/dev/null || true
        fi
        
        # Extract final score
        if [[ -f "$score_file" ]]; then
            local final_score=$(python3 -c "import json,sys; d=json.load(open('$score_file')); print(d.get('scoring',{}).get('final_score',0))" 2>/dev/null || echo "0")
            scored_variants+=("${final_score}|${variant}|${report_file}|${score_file}")
        fi
    done
    
    # Sort by score (descending)
    IFS=$'\n' scored_variants=($(sort -t'|' -k1 -nr <<< "${scored_variants[*]}"))
    unset IFS
    
    echo "${scored_variants[*]}"
}

# Generate summary report
generate_summary() {
    local scored_variants=($1)
    
    log_section "Generating Summary Report"
    
    local summary_file="$REPORTS_DIR/summary.md"
    
    # Get top variant as recommendation
    local recommended_variant=""
    local recommended_score=""
    if [[ ${#scored_variants[@]} -gt 0 ]]; then
        IFS='|' read -r recommended_score recommended_variant _ _ <<< "${scored_variants[0]}"
    fi
    
    cat > "$summary_file" << EOF
# Niche Tuning Loop Results

**Generated:** $(date -u +"%Y-%m-%d %H:%M:%S UTC")  
**Query Set:** $QUERY_SET  
**Timestamp:** $TIMESTAMP

## Recommended Configuration

$(if [[ -n "$recommended_variant" ]]; then echo "**${recommended_variant}** (Score: ${recommended_score})"; else echo "No recommendation available"; fi)

$(if [[ -n "$recommended_variant" ]]; then echo "This configuration achieved the best balance of exact/acronym performance while maintaining conceptual query quality."; fi)

## Ranked Results

| Rank | Variant | Score | MRR | Recall@10 | Exact MRR | Acronym MRR | Conceptual MRR | Status |
|------|---------|-------|-----|-----------|-----------|-------------|----------------|--------|
EOF
    
    local rank=1
    for entry in "${scored_variants[@]}"; do
        IFS='|' read -r score variant report_file score_file <<< "$entry"
        
        # Extract metrics from report
        local mrr=$(python3 -c "import json,sys; d=json.load(open('$report_file')); print(d.get('metrics',{}).get('mrr',0))" 2>/dev/null || echo "N/A")
        local recall_10="N/A"
        local exact_mrr="N/A"
        local acronym_mrr="N/A"
        local conceptual_mrr="N/A"
        
        if [[ -f "$score_file" ]]; then
            recall_10=$(python3 -c "import json,sys; d=json.load(open('$score_file')); print(d.get('scoring',{}).get('raw_metrics',{}).get('recall_at_10','N/A'))" 2>/dev/null || echo "N/A")
            exact_mrr=$(python3 -c "import json,sys; d=json.load(open('$score_file')); print(d.get('scoring',{}).get('raw_metrics',{}).get('exact_mrr','N/A'))" 2>/dev/null || echo "N/A")
            acronym_mrr=$(python3 -c "import json,sys; d=json.load(open('$score_file')); print(d.get('scoring',{}).get('raw_metrics',{}).get('acronym_mrr','N/A'))" 2>/dev/null || echo "N/A")
            conceptual_mrr=$(python3 -c "import json,sys; d=json.load(open('$score_file')); print(d.get('scoring',{}).get('raw_metrics',{}).get('conceptual_mrr','N/A'))" 2>/dev/null || echo "N/A")
        fi
        
        # Get classification
        local classification=$(python3 -c "import json,sys; d=json.load(open('$score_file')); print(d.get('classification','unknown'))" 2>/dev/null || echo "unknown")
        
        # Format numbers
        [[ "$mrr" != "N/A" ]] && mrr=$(printf "%.4f" "$mrr")
        [[ "$score" != "N/A" ]] && score=$(printf "%.4f" "$score")
        
        local status_icon="✓"
        if [[ "$classification" == "regression" ]]; then
            status_icon="✗"
        elif [[ "$classification" == "mixed" ]]; then
            status_icon="~"
        fi
        
        echo "| $rank | $variant | $score | $mrr | $recall_10 | $exact_mrr | $acronym_mrr | $conceptual_mrr | $status_icon |" >> "$summary_file"
        
        ((rank++))
    done
    
    cat >> "$summary_file" << EOF

## Scoring Methodology

**Weighted Score Formula:**

```
score = 0.4*MRR + 0.2*Recall@10 + 0.2*ExactMRR + 0.2*AcronymMRR - regressions*0.5 + bonus
```

**Priorities:**
- Exact Identifier MRR (weight: 0.2) - Critical for technical parameter queries
- Acronym MRR (weight: 0.2) - Important for disambiguation
- Overall MRR (weight: 0.4) - General ranking quality
- Recall@10 (weight: 0.2) - Coverage of relevant results

**Penalties:**
- Conceptual Paraphrase MRR below 0.6 incurs penalty
- Prevents gains in exact/acronym at expense of semantic understanding

**Bonuses:**
- +0.05 for Exact MRR > 0.9
- +0.03 for Acronym MRR > 0.8

## Configuration Details

### Variant Descriptions

| Variant | Focus | Key Changes |
|---------|-------|-------------|
| baseline | Reference | Original config.niche.toml settings |
| high_identifier_boost | Exact matches | Increased lexical weights, higher exact_heading_boost (0.35), larger identifier pools |
| high_acronym_boost | Short queries | Reduced rrf_k (50), higher lex weights for short queries, larger short query pools |
| aggressive_host_policy | Authority | Higher authority_bonus (0.15), strict scoring floor, disabled relaxed fallback |
| balanced_combo | Hybrid | Moderate increases across all lexical knobs, balanced pool sizing |
| max_precision | Precision | Extreme lexical weighting (0.58 short), high exact boosts (0.45), large pools |
| conservative_safe | Safety | Lower weights, conservative penalties, minimal regression risk |
| vector_focused | Semantic | Higher vector weights (0.30/0.40), lower lexical emphasis |
| lexical_heavy | Term matching | Maximum lexical weights (0.58 short), aggressive host policy |

## Bucket MRR Interpretation

**Target Thresholds:**

| Bucket | Target MRR | Notes |
|--------|------------|-------|
| Exact Identifier | ≥ 0.90 | Should be highest - exact matches expected |
| Acronym Disambiguation | ≥ 0.80 | Good disambiguation via context |
| Conceptual Paraphrase | ≥ 0.70 | Semantic understanding via embeddings |

**Classification Legend:**
- ✓ (strong_improvement/moderate_improvement) - Safe to deploy
- ~ (neutral/mixed) - Review recommended
- ✗ (regression) - Do not deploy

## Files Generated

Individual reports per variant:
EOF
    
    # List all generated files
    for entry in "${scored_variants[@]}"; do
        IFS='|' read -r _ variant _ _ <<< "$entry"
        echo "- \`${variant}_${TIMESTAMP}.json\` - Full evaluation results" >> "$summary_file"
        echo "- \`${variant}_${TIMESTAMP}_scored.json\` - Scoring breakdown" >> "$summary_file"
        echo "- \`${variant}_${TIMESTAMP}.log\` - Evaluation log" >> "$summary_file"
        echo "" >> "$summary_file"
    done
    
    cat >> "$summary_file" << EOF

## Next Steps

1. **Review the top-ranked variant** - Check logs for any anomalies
2. **Validate on production-like data** - Run A/B test if possible
3. **Monitor bucket metrics** - Ensure all query types perform well
4. **Iterate on promising variants** - Create new variants near top performers

## Adding New Variants

To add a new variant:

1. Create \`configs/tuning/your_variant.toml\` based on a top performer
2. Modify 2-3 ranking parameters at a time
3. Document the hypothesis in comments
4. Re-run the tuning loop
5. Compare results using this summary

---

*Generated by Niche Tuning Loop v1.0*
EOF
    
    log_success "Summary report generated: $summary_file"
}

# Main execution
main() {
    parse_args "$@"
    setup
    
    log_section "Starting Niche Tuning Loop"
    log_info "Query set: $QUERY_SET"
    log_info "Variants: ${#VARIANTS[@]}"
    log_info "Reports dir: $REPORTS_DIR"
    
    # Run all variants
    local results=()
    local success_count=0
    local fail_count=0
    
    for variant in "${VARIANTS[@]}"; do
        if result=$(run_variant "$variant"); then
            results+=("$result")
            ((success_count++))
        else
            results+=("$result")
            ((fail_count++))
            log_error "Variant $variant failed"
        fi
        echo ""
    done
    
    log_section "Variant Execution Summary"
    log_info "Successful: $success_count"
    log_info "Failed: $fail_count"
    log_info "Total: ${#VARIANTS[@]}"
    
    # Score and rank variants (only if we have successful results)
    if [[ $success_count -gt 0 ]]; then
        local scored_list=$(score_variants "${results[*]}")
        generate_summary "$scored_list"
    else
        log_warn "No successful results to score"
    fi
    
    log_section "Tuning Loop Complete"
    log_info "Results saved to: $REPORTS_DIR"
    log_info "Summary report: $REPORTS_DIR/summary.md"
    
    echo ""
    echo "To view results:"
    echo "  cat $REPORTS_DIR/summary.md"
    echo ""
    
    # Return success if at least half the variants ran successfully
    if [[ $success_count -ge $((${#VARIANTS[@]} / 2)) ]]; then
        exit 0
    else
        log_error "Too many failures - review logs in $REPORTS_DIR"
        exit 1
    fi
}

# Run main
main "$@"
