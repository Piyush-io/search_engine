# HNSW Tuning Report

## Overview

This report documents the HNSW (Hierarchical Navigable Small World) parameter tuning framework implemented for the search engine. The goal is to optimize the trade-off between Approximate Nearest Neighbor (ANN) search quality and query latency through systematic parameter sweeps.

## HNSW Parameters

### Key Parameters

| Parameter | Description | Impact |
|-----------|-------------|----------|
| **m** | Number of bi-directional links for each node | Higher = better accuracy, larger index |
| **ef_construction** | Size of dynamic candidate list during index build | Higher = better graph quality, slower build |
| **ef_search** | Size of dynamic candidate list during search | Higher = better recall, slower queries |

### Parameter Sweeps

We tested four configurations representing different quality/latency trade-offs:

| Profile | m | ef_construction | ef_search | Use Case |
|---------|---|-----------------|-----------|----------|
| **baseline** | 8 | 80 | 120 | Current niche profile; fast build, moderate quality |
| **m12_ef120** | 12 | 120 | 120 | Medium quality; general production use |
| **high_quality** | 16 | 200 | 120 | High build quality; offline/cold storage |
| **max_quality** | 16 | 200 | 200 | Maximum quality; best recall, slowest queries |

## Experiment Framework

### Configuration Files

The sweep uses four config files in the project root:

1. `config.hnsw_baseline.toml` - Current niche settings
2. `config.hnsw_m12_ef120.toml` - Medium quality
3. `config.hnsw_high_quality.toml` - High build quality
4. `config.hnsw_max_quality.toml` - Maximum quality

Each config uses distinct index paths to avoid conflicts:
- `index_path = "./hnsw_index.niche.{profile}.bin"`

### Running the Sweep

```bash
# Set paths to evaluation data
export QRELS_PATH="eval/qrels.niche.txt"
export QUERIES_PATH="eval/queries.niche.txt"

# Run the sweep
./scripts/run_hnsw_sweep.sh
```

The script will:
1. Build the index for each profile (timing the build)
2. Run evaluation with timing
3. Collect metrics from `reports/eval_results.json`
4. Generate individual profile reports in `reports/hnsw_sweep/{profile}.json`
5. Create a summary table in `reports/hnsw_sweep/summary.md`

### Expected Results Format

Each profile generates a JSON report:

```json
{
  "profile": "baseline",
  "config": "config.hnsw_baseline.toml",
  "timestamp": "2024-01-15T10:30:00Z",
  "timing": {
    "build_time_seconds": 120,
    "eval_time_seconds": 45,
    "total_time_seconds": 165
  },
  "metrics": {
    "mrr": 0.6234,
    "ndcg_at": [[1, 0.5432], [3, 0.6123], [5, 0.6456], [10, 0.6789]],
    "recall_at": [[1, 0.1234], [3, 0.3456], [5, 0.4567], [10, 0.5678]],
    "num_queries": 150
  },
  "index_size_bytes": 524288000
}
```

## Dynamic ef_search by Query Class

### Implementation

The system now supports query-time ef_search override based on query classification:

```rust
// In RankingConfig:
ef_search_short: usize       // default: 80 (for short ambiguous queries)
ef_search_long: usize       // default: 120 (for long specific queries)
ef_search_identifier: usize // default: 150 (for identifier-heavy queries)
```

### Query Classification

Queries are classified into four categories:

1. **ShortAmbiguous** (1-3 tokens): High ef_search needed due to ambiguity
2. **IdentifierHeavy**: Contains underscores, camelCase, technical patterns
3. **LongSpecific** (6+ tokens): Lower ef_search acceptable due to specificity
4. **Medium** (4-5 tokens): Default behavior

### Usage

The classification happens automatically during query execution:

```rust
let query_class = classify_query_for_retrieval(query_text, &query_tokens);
let ef_search = match query_class {
    RetrievalQueryClass::ShortAmbiguous => ranking.ef_search_short,
    RetrievalQueryClass::LongSpecific => ranking.ef_search_long,
    RetrievalQueryClass::IdentifierHeavy => ranking.ef_search_identifier,
    RetrievalQueryClass::Medium => ranking.ef_search_short,
};
index.set_ef_search(ef_search);
```

## Methodology

### Build Process

For each profile:
1. Clean previous index artifacts
2. Rebuild HNSW index from embeddings: `cargo run --release --bin index -- --full`
3. Time the build process
4. Record index file size

### Evaluation

Using standard TREC-style qrels:
1. Load queries and relevance judgments
2. Run each query through the search pipeline
3. Compute metrics:
   - **MRR** (Mean Reciprocal Rank)
   - **NDCG@k** (Normalized Discounted Cumulative Gain)
   - **Recall@k** (Proportion of relevant docs retrieved)
4. Record query latency (if timing enabled)

### Quality vs Latency Trade-off

Expected trends:

| Metric | baseline → max_quality |
|--------|------------------------|
| Build Time | 1x → 2-3x |
| Index Size | 1x → 1.5-2x |
| MRR | Baseline → +10-20% |
| NDCG@10 | Baseline → +10-15% |
| Query Latency | 1x → 1.5-2x |

## Recommendations

### For Niche Profile (Default)

**Current: baseline (m=8, ef_construction=80, ef_search=120)**

The baseline provides a good balance for the 60k-page niche corpus:
- Fast rebuilds during development
- Moderate quality for technical queries
- 120 ef_search gives reasonable recall

**Suggested improvement:** Use m12_ef120 for production
- 50% more connections per node
- Better recall for ambiguous queries
- Acceptable build time increase (~30-50%)

### Profile Selection Guide

| Scenario | Recommended Profile | Rationale |
|----------|---------------------|-----------|
| Development/CI | baseline | Fast iteration |
| Production (latency-sensitive) | m12_ef120 | Good quality, acceptable latency |
| Production (quality-focused) | high_quality | Best quality with standard search |
| Offline analysis | max_quality | Maximum recall, latency not critical |
| Large corpus (500k+ pages) | high_quality | Need more connections for density |

### Future Work

1. **Adaptive ef_search**: Dynamically adjust based on result confidence
2. **Multi-level index**: Separate indexes for different content tiers
3. **Hybrid search**: Combine multiple HNSW graphs with different parameters
4. **Online learning**: Tune parameters based on user click-through rates

## Appendix: Code Changes

### Files Modified

1. **src/config.rs**: Added ef_search_short, ef_search_long, ef_search_identifier to RankingConfig
2. **src/search/hnsw.rs**: Changed ef_search to AtomicUsize for thread-safe dynamic updates
3. **src/search/vector_index.rs**: Added set_ef_search method to VectorIndex trait
4. **src/search/composite.rs**: Propagated set_ef_search to base HNSW index
5. **src/search/query.rs**: Integrated query classification with ef_search selection

### Files Created

1. **config.hnsw_baseline.toml**: Baseline configuration
2. **config.hnsw_m12_ef120.toml**: Medium quality configuration
3. **config.hnsw_high_quality.toml**: High quality configuration
4. **config.hnsw_max_quality.toml**: Maximum quality configuration
5. **scripts/run_hnsw_sweep.sh**: Automation script for parameter sweeps
6. **docs/hnsw-tuning-report.md**: This report
7. **AGENT3_CHANGELOG.md**: Summary of changes

## References

- Malkov, Y.A., Yashunin, D.A. (2020). Efficient and robust approximate nearest neighbor search using Hierarchical Navigable Small World graphs.
- https://github.com/rust-cv/hnsw
