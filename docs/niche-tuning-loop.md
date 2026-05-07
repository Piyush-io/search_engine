# Niche Tuning Loop

A diagnostics-driven auto-tuning system for optimizing ranking knobs on the niche profile.

## Overview

The Niche Tuning Loop systematically tests multiple ranking configurations, evaluates them against labeled benchmarks, and produces ranked recommendations based on a weighted scoring formula that prioritizes exact identifier and acronym disambiguation performance while guarding against conceptual paraphrase regressions.

## Quick Start

```bash
# Run all variants with full evaluation (100 queries)
./scripts/run_niche_tuning_loop.sh

# Quick validation with 10 queries
./scripts/run_niche_tuning_loop.sh --quick

# Run specific variants only
./scripts/run_niche_tuning_loop.sh --variants baseline,high_identifier_boost

# Generate reports from existing results (skip evaluation)
./scripts/run_niche_tuning_loop.sh --skip-eval
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Niche Tuning Loop                         │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────┐     │
│  │   Configs    │ → │  Evaluation  │ → │   Scoring    │     │
│  │  (variants)  │   │   (eval bin) │   │   (Python)   │     │
│  └──────────────┘   └──────────────┘   └──────────────┘     │
│        ↑                                    ↓                 │
│   configs/tuning/                    reports/niche_tuning/   │
│   ├── baseline.toml                ├── *.json (metrics)     │
│   ├── high_identifier_boost.toml   ├── *_scored.json        │
│   ├── high_acronym_boost.toml      ├── *.log               │
│   ├── aggressive_host_policy.toml  └── summary.md          │
│   ├── balanced_combo.toml                                   │
│   ├── max_precision.toml                                    │
│   ├── conservative_safe.toml                                │
│   ├── vector_focused.toml                                   │
│   └── lexical_heavy.toml                                    │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Usage Instructions

### Prerequisites

1. **Built index**: The tuning loop assumes indexes are already built. Build them first:
   ```bash
   for config in configs/tuning/*.toml; do
       SEARCH_ENGINE_CONFIG_PATH="$config" cargo run --release --bin index -- --full
   done
   ```

2. **Benchmark data**: Ensure benchmark files exist:
   - `benchmarks/niche_db/queries_100.tsv` (or queries_10.tsv)
   - `benchmarks/niche_db/qrels_100.tsv` (or qrels_10.tsv)

3. **Python 3**: Required for scoring script (usually pre-installed on macOS/Linux)

### Running the Full Loop

```bash
# Full evaluation with all 8 variants
./scripts/run_niche_tuning_loop.sh

# Expected output:
# [INFO] Starting Niche Tuning Loop
# [INFO] Query set: full
# [INFO] Variants: 8
# ...
# [SUCCESS] Summary report: reports/niche_tuning/summary.md
```

### Quick Validation

Use the quick mode for rapid iteration during development:

```bash
./scripts/run_niche_tuning_loop.sh --quick
```

This uses the 10-query subset for faster feedback (~10x faster than full evaluation).

### Selective Variant Testing

Test specific configurations:

```bash
# Single variant
./scripts/run_niche_tuning_loop.sh --variants baseline

# Multiple variants
./scripts/run_niche_tuning_loop.sh --variants high_identifier_boost,high_acronym_boost

# Comma-separated list, no spaces
```

### Re-Generating Reports

If you need to re-score existing results with different weights:

```bash
./scripts/run_niche_tuning_loop.sh --skip-eval
```

This reads existing JSON reports and regenerates the summary.

## Interpretation Guide

### Reading the Summary Report

The summary (`reports/niche_tuning/summary.md`) contains:

1. **Recommended Configuration**: The top-ranked variant based on weighted score
2. **Ranked Results Table**: All variants sorted by final score with key metrics
3. **Scoring Methodology**: Explanation of the formula and weights
4. **Configuration Details**: What each variant tests

### Understanding the Score

**Formula:**
```
score = 0.4*MRR + 0.2*Recall@10 + 0.2*ExactMRR + 0.2*AcronymMRR - regressions*0.5 + bonus
```

**Components:**

| Component | Weight | Target | Why It Matters |
|-----------|--------|--------|----------------|
| Overall MRR | 0.4 | ≥ 0.80 | General ranking quality |
| Recall@10 | 0.2 | ≥ 0.75 | Coverage of relevant docs |
| Exact Identifier MRR | 0.2 | ≥ 0.90 | Critical for technical terms |
| Acronym MRR | 0.2 | ≥ 0.80 | Disambiguation quality |
| Conceptual Penalty | -0.5 | < 0.6 triggers | Guards against semantic regression |
| Bonus | +0.05/+0.03 | Exact>0.9, Acronym>0.8 | Rewards excellence |

### Bucket MRR Thresholds

| Bucket | Target | Acceptable | Warning | Critical |
|--------|--------|------------|---------|----------|
| Exact Identifier | ≥ 0.90 | 0.85-0.90 | 0.80-0.85 | < 0.80 |
| Acronym Disambiguation | ≥ 0.80 | 0.75-0.80 | 0.70-0.75 | < 0.70 |
| Conceptual Paraphrase | ≥ 0.70 | 0.65-0.70 | 0.60-0.65 | < 0.60 |

### Classification Legend

The summary table includes a status column:

- **✓ strong_improvement**: Score improved >5%, no conceptual regression
- **✓ moderate_improvement**: Score improved >2%, acceptable trade-offs
- **~ neutral**: Changes within ±2%, safe but not impactful
- **~ mixed**: Some gains, some losses - review carefully
- **✗ regression**: Score dropped >3% or conceptual MRR dropped >8%

### Decision Matrix

| Scenario | Action |
|----------|--------|
| Top variant is "strong_improvement" | Deploy after spot-checking logs |
| Top variant is "moderate_improvement" | Validate on production-like data |
| Multiple "neutral" variants | Iterate with more aggressive changes |
| Any "regression" in top 3 | Add stronger conceptual penalty |
| Baseline remains best | Current config is already optimal |

## How to Add New Variants

### 1. Choose a Base

Start with the top-performing variant from the latest run:

```bash
cp configs/tuning/balanced_combo.toml configs/tuning/my_experiment.toml
```

### 2. Modify 2-3 Parameters

Focus on specific hypotheses:

```toml
# Hypothesis: Increasing exact_heading_boost further improves identifiers
# but we need to watch for conceptual regression
exact_heading_boost = 0.40  # was 0.28
exact_body_boost = 0.20     # was 0.14
```

### 3. Document Your Hypothesis

Add comments explaining the expected behavior:

```toml
[ranking]
# EXPERIMENT: Aggressive exact matching
# Expected: +5% Exact MRR, -2% Conceptual MRR (acceptable trade-off)
# Risk: May over-match on partial terms
exact_heading_boost = 0.40
exact_body_boost = 0.20
```

### 4. Run the Loop

```bash
./scripts/run_niche_tuning_loop.sh --variants my_experiment,balanced_combo
```

### 5. Compare Results

Check the summary to see if your hypothesis was correct.

### Parameter Guidelines

**Lexical Weights (short_lex_weight, long_lex_weight):**
- Range: 0.20 - 0.60
- Higher = more lexical matching, better for exact terms
- Lower = more vector matching, better for concepts

**Exact Boosts (exact_heading_boost, exact_body_boost):**
- Heading: 0.15 - 0.45
- Body: 0.05 - 0.30
- Higher = stronger exact match preference
- Diminishing returns above 0.40

**RRF Parameters (rrf_k, *_rrf_vec_weight, *_rrf_lex_weight):**
- Lower rrf_k = more aggressive fusion
- Higher lex_weight = lexical results rank higher
- Balance based on query length

**Pool Multipliers (*_pool_mult_*):**
- Higher = more candidates considered, slower but more thorough
- Lower = faster but may miss relevant results
- Identifier queries benefit from 150-250 range

### Testing Strategies

**Strategy 1: Grid Search**
```bash
# Create variants with systematic parameter sweeps
for boost in 0.25 0.30 0.35 0.40; do
    sed "s/exact_heading_boost = .*/exact_heading_boost = $boost/" \
        configs/tuning/baseline.toml > configs/tuning/exact_${boost}.toml
done
```

**Strategy 2: Focused Iteration**
```bash
# Pick top performer, create neighbors
# If balanced_combo wins, test nearby configurations
./scripts/run_niche_tuning_loop.sh --variants \
    balanced_combo,balanced_combo_plus,balanced_combo_minus
```

**Strategy 3: A/B Component Testing**
```bash
# Test one component at a time vs baseline
./scripts/run_niche_tuning_loop.sh --variants \
    baseline,high_identifier_only,high_acronym_only
```

## Troubleshooting

### "Index not found" Errors

Build indexes before running the loop:
```bash
for config in configs/tuning/*.toml; do
    echo "Building index for: $config"
    SEARCH_ENGINE_CONFIG_PATH="$config" cargo run --release --bin index -- --full
done
```

### Low Scores Across All Variants

Check:
1. Are benchmarks appropriate for your data?
2. Is the index populated with relevant documents?
3. Are embedding models loading correctly?

### Unexpected Baseline Performance

If baseline performs poorly:
1. Check `reports/eval_query_diagnostics.json` for zero-result queries
2. Verify `config.niche.toml` is correctly configured
3. Ensure crawled data exists at expected paths

### Scoring Script Errors

Ensure Python 3 is available:
```bash
python3 --version
# If not available, install Python 3
```

## Best Practices

### Before Running the Loop

1. **Document current performance**: Note baseline MRR, recall, and bucket metrics
2. **Check index freshness**: Stale indexes produce misleading results
3. **Validate benchmarks**: Ensure qrels are accurate for your corpus
4. **Free up resources**: Close unnecessary applications for consistent timing

### During Iteration

1. **Make small changes**: Modify 2-3 parameters per variant
2. **Test hypotheses**: Each variant should test a specific theory
3. **Keep baseline**: Always include baseline for comparison
4. **Name descriptively**: `high_identifier_boost` not `variant_7`

### After Running

1. **Review logs**: Check for errors or anomalies in `*.log` files
2. **Validate top performers**: Manually test recommended queries
3. **Check for overfitting**: Ensure improvements generalize
4. **Document learnings**: Update this doc with insights

## Advanced Usage

### Custom Scoring Weights

Modify the scoring formula in `scripts/score_variant.py`:

```python
# In compute_weighted_score():
base_score = (
    0.5 * mrr +              # Increase MRR weight
    0.1 * recall_10 +
    0.3 * exact_mrr +        # Prioritize exact matches
    0.1 * acronym_mrr
)
```

### Custom Evaluation Sets

Specify custom benchmark paths via environment variables:

```bash
export QUERIES_PATH="my_queries.tsv"
export QRELS_PATH="my_qrels.tsv"
./scripts/run_niche_tuning_loop.sh
```

### Parallel Execution

For faster iteration, run variants in parallel (manual coordination):

```bash
# Terminal 1
./scripts/run_niche_tuning_loop.sh --variants baseline &

# Terminal 2
./scripts/run_niche_tuning_loop.sh --variants high_identifier_boost &

# Then manually merge results for scoring
```

## Files and Directories

```
configs/tuning/
├── baseline.toml                 # Reference configuration
├── high_identifier_boost.toml    # Prioritizes exact matches
├── high_acronym_boost.toml       # Optimizes short/acronym queries
├── aggressive_host_policy.toml   # Strong authority weighting
├── balanced_combo.toml           # Moderate improvements across knobs
├── max_precision.toml            # Extreme exact matching
├── conservative_safe.toml        # Minimal regression risk
├── vector_focused.toml           # Semantic matching emphasis
└── lexical_heavy.toml            # Maximum term matching

scripts/
├── run_niche_tuning_loop.sh      # Main orchestrator script
└── score_variant.py              # Weighted scoring logic

reports/niche_tuning/
├── summary.md                    # Human-readable ranked results
├── *_YYYYMMDD_HHMMSS.json        # Per-variant full metrics
├── *_YYYYMMDD_HHMMSS_scored.json # Per-variant scoring breakdown
└── *_YYYYMMDD_HHMMSS.log         # Evaluation logs
```

## Related Documentation

- `docs/eval-playbook.md` - Understanding evaluation metrics
- `docs/hnsw-tuning-report.md` - Index parameter tuning
- `config.niche.toml` - Base configuration reference
- `AGENT1_CHANGELOG.md` through `AGENT5_CHANGELOG.md` - Project history

## Version History

- **v1.0** (Current): Initial implementation with 8 variants, weighted scoring, and comprehensive reporting

---

*For questions or issues, check the logs in `reports/niche_tuning/` and refer to the troubleshooting section.*
