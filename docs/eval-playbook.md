# Evaluation Playbook

This document describes how to run offline evaluation and interpret quality + latency metrics for the hybrid search engine.

## Table of Contents

- [Benchmark Format](#benchmark-format)
- [Running Evaluation](#running-evaluation)
- [Interpreting Metrics](#interpreting-metrics)
- [Regression Criteria](#regression-criteria)
- [Query Buckets](#query-buckets)
- [Profiling Data](#profiling-data)

---

## Benchmark Format

### Query File Format (TSV)

Queries are stored in tab-separated value (TSV) format with two columns:

```
query_id<TAB>query_text
```

Example (`queries_10.tsv`):
```
q1	what does wal_level logical enable
q2	sqlite wal checkpoint truncate behavior
```

**Requirements:**
- Query IDs must be unique within a file
- Query text should not contain tabs
- Lines starting with `#` are treated as comments (if supported)
- Empty lines are skipped

### Qrels File Format (TSV)

Query relevance judgments (qrels) use the standard TREC format with 4 columns:

```
query_id<TAB>iteration<TAB>doc_id<TAB>relevance
```

Example (`qrels_10.tsv`):
```
q1	0	https://www.postgresql.org/docs/current/runtime-config-wal.html	3
q1	0	https://www.postgresql.org/docs/current/wal-intro.html	2
```

**Relevance Grades:**

| Grade | Meaning | Description |
|-------|---------|-------------|
| 0 | Irrelevant | Document has no relation to the query |
| 1 | Related | Document touches on the topic but doesn't answer the query |
| 2 | Relevant | Document directly answers or explains the query topic |
| 3 | Highly Relevant | Document is the authoritative/best source for this query |

**Doc ID Format:**
Doc IDs are canonicalized URLs. The system uses `canonical_doc_key()` from `src/eval/url_match.rs` to normalize URLs, which:
- Strips query parameters and fragments
- Normalizes hostnames (e.g., `www.postgresql.org` → `postgresql.org`)
- Handles special cases like Rust documentation channels

---

## Running Evaluation

### Basic Usage

Run the evaluation binary with required arguments:

```bash
cargo run --release --bin eval -- \
  --queries benchmarks/niche_db/queries_10.tsv \
  --qrels benchmarks/niche_db/qrels_10.tsv \
  --k 1,3,5,10
```

### Command Line Arguments

| Argument | Required | Default | Description |
|----------|----------|---------|-------------|
| `--queries` | Yes | - | Path to TSV file containing queries |
| `--qrels` | Yes | - | Path to TSV file with relevance judgments |
| `--k` | No | `1,3,5,10` | Comma-separated list of k values for metrics |

### Output Files

The evaluation produces two report files:

1. **`reports/eval_results.json`** - Aggregated metrics across all queries
2. **`reports/eval_query_diagnostics.json`** - Per-query profiling data (latency, candidate counts)

### Example Output

```
[eval] <embedding backend info>
=== Evaluation Results ===
Queries evaluated: 10
Zero-result queries: 0 (0.0%)
MRR: 0.8500
NDCG@1: 0.7500
NDCG@3: 0.8200
NDCG@5: 0.8400
Recall@1: 0.6500
Recall@3: 0.8000
Recall@5: 0.8500

=== Per-Bucket Metrics ===
Exact Identifier:
  Queries: 3 | MRR: 0.9444 | NDCG@3: 0.9444
Acronym/Disambiguation:
  Queries: 2 | MRR: 0.7500 | NDCG@3: 0.8333
Conceptual Paraphrase:
  Queries: 3 | MRR: 0.8056 | NDCG@3: 0.8333
Long-form Mixed:
  Queries: 2 | MRR: 0.8333 | NDCG@3: 0.7500

[eval] report written to reports/eval_results.json
[eval] diagnostics written to reports/eval_query_diagnostics.json
```

### Running with Different Query Sets

**Small set (10 queries - quick validation):**
```bash
cargo run --release --bin eval -- \
  --queries benchmarks/niche_db/queries_10.tsv \
  --qrels benchmarks/niche_db/qrels_10.tsv
```

**Large set (100 queries - comprehensive evaluation):**
```bash
cargo run --release --bin eval -- \
  --queries benchmarks/niche_db/queries_100.tsv \
  --qrels benchmarks/niche_db/qrels_100.tsv \
  --k 1,3,5,10,20
```

---

## Interpreting Metrics

### Core Quality Metrics

#### Mean Reciprocal Rank (MRR)

**Formula:** Average of `1/rank` where rank is the position of the first relevant result.

**Interpretation:**
- **MRR >= 0.9**: Excellent - users typically find what they want at rank 1
- **MRR 0.7-0.9**: Good - relevant results usually in top 2-3
- **MRR 0.5-0.7**: Fair - room for improvement
- **MRR < 0.5**: Poor - significant issues with ranking

**Use case:** Best for evaluating "find the one right answer" scenarios.

#### NDCG@k (Normalized Discounted Cumulative Gain)

**Formula:** DCG@k / IDCG@k, where:
- DCG = SUM (2^rel - 1) / log2(i + 2)
- IDCG = ideal DCG (perfect ranking)

**Interpretation:**
- **NDCG >= 0.9**: Results are nearly perfectly ordered by relevance
- **NDCG 0.7-0.9**: Good ranking quality
- **NDCG 0.5-0.7**: Moderate quality, needs improvement
- **NDCG < 0.5**: Poor ranking

**Use case:** Best when there are multiple relevant results with varying relevance grades.

#### Recall@k

**Formula:** Number of relevant docs in top-k / Total relevant docs

**Interpretation:**
- **Recall >= 0.8**: Most relevant documents are retrieved
- **Recall 0.6-0.8**: Decent coverage
- **Recall < 0.6**: Significant relevant content is missing

**Use case:** Important when users want comprehensive results, not just the top hit.

### Per-Bucket Metrics

Metrics are computed separately for different query types:

| Bucket | Description | Expected Behavior |
|--------|-------------|-------------------|
| `exact_identifier` | Exact config/parameter names | Should have very high MRR (>=0.9) |
| `acronym_disambiguation` | Acronyms like "xid", "wal" | Tests context understanding |
| `conceptual_paraphrase` | Paraphrased concepts | Tests semantic matching |
| `long_form_mixed` | Complex multi-part queries | Tests comprehensive retrieval |

### Zero-Result Rate

**Formula:** Percentage of queries returning no results.

**Interpretation:**
- **0%**: Ideal - all queries return something
- **< 5%**: Acceptable for broad queries
- **> 10%**: Concerning - indicates gaps in index coverage

---

## Regression Criteria

When making changes to the search system, use these thresholds to detect regressions:

### Critical Regressions (Block Release)

| Metric | Threshold | Action Required |
|--------|-----------|-----------------|
| MRR | Drop > 0.05 | Investigate immediately |
| NDCG@5 | Drop > 0.05 | Investigate immediately |
| Zero-result rate | Increase > 5% | Block release |

### Warning Signs (Investigate)

| Metric | Threshold | Action |
|--------|-----------|--------|
| MRR | Drop 0.03-0.05 | Review changes |
| NDCG@5 | Drop 0.03-0.05 | Review changes |
| Exact identifier MRR | Drop > 0.05 | Check query handling |
| Per-query latency | Increase > 50% | Profile and optimize |

### Per-Bucket Regression Thresholds

| Bucket | MRR Drop Concern | Notes |
|--------|------------------|-------|
| Exact Identifier | > 0.03 | Should be most stable |
| Acronym Disambiguation | > 0.05 | May fluctuate with synonym changes |
| Conceptual Paraphrase | > 0.05 | Sensitive to embedding changes |
| Long-form Mixed | > 0.05 | Complex, more variance expected |

### What to Do When Regressions Occur

1. **Check the diagnostics file** (`reports/eval_query_diagnostics.json`) for:
   - Queries with high latency
   - Queries with zero candidates
   - Candidate count changes

2. **Compare per-query results** between baseline and new version

3. **Identify pattern** in failing queries:
   - Specific bucket affected?
   - Specific query length?
   - Specific topic area?

4. **Isolate the change** that caused the regression

---

## Query Buckets

Queries are classified into buckets based on their characteristics:

### 1. Exact Identifier (`exact_identifier`)

**Characteristics:**
- Exact parameter/variable names
- Configuration settings
- Function names

**Examples:**
- "wal_level postgresql"
- "full_page_writes"
- "sqlite busy_timeout"

**Expected Performance:** Highest MRR due to exact matching

### 2. Acronym/Disambiguation (`acronym_disambiguation`)

**Characteristics:**
- Short acronyms
- Terms needing context
- Ambiguous abbreviations

**Examples:**
- "xid postgres" (transaction ID)
- "wal vs checkpoint"
- "lsn write ahead log"

**Expected Performance:** Moderate MRR, tests synonym expansion

### 3. Conceptual Paraphrase (`conceptual_paraphrase`)

**Characteristics:**
- Natural language descriptions
- "How does..." questions
- Paraphrased concepts

**Examples:**
- "how does postgres handle concurrent writes"
- "what prevents dirty reads in sqlite"
- "why does vacuum freeze tuples"

**Expected Performance:** Tests semantic understanding via embeddings

### 4. Long-form Mixed (`long_form_mixed`)

**Characteristics:**
- Multiple concepts
- Complex questions
- Requires synthesis

**Examples:**
- "explain postgres mvcc transaction isolation levels with examples"
- "sqlite wal mode advantages disadvantages when to use"

**Expected Performance:** Most challenging, lower but stable metrics

---

## Profiling Data

The diagnostics file contains per-query performance metrics:

### Structure

```json
{
  "queries": [
    {
      "query_id": "q1",
      "query_text": "what does wal_level logical enable",
      "bucket": "exact_identifier",
      "elapsed_ms": 45.2,
      "num_results": 5,
      "candidates": {
        "vector_hits": 500,
        "lexical_hits": 400,
        "fused_candidates": 600,
        "final_selected": 5
      }
    }
  ],
  "summary": {
    "total_queries": 100,
    "zero_result_queries": 2,
    "avg_elapsed_ms": 52.3,
    "p50_elapsed_ms": 48.0,
    "p95_elapsed_ms": 89.5,
    "p99_elapsed_ms": 120.3
  }
}
```

### Interpreting Candidate Counts

| Metric | Healthy Range | Interpretation |
|--------|---------------|----------------|
| `vector_hits` | 100-2000 | Vector search pool size |
| `lexical_hits` | 50-1000 | Lexical search pool size |
| `fused_candidates` | 200-2000 | After RRF fusion |
| `final_selected` | k to 2*k | After deduplication/filtering |

**Warning signs:**
- `vector_hits` = 0: Embedding service issue
- `lexical_hits` = 0: Lexical index issue
- `fused_candidates` << `k`: Pool too small
- `final_selected` << `k`: Aggressive filtering

### Latency Analysis

**Typical latency breakdown:**
- Embedding: ~20-50ms
- Vector search: ~10-30ms
- Lexical search: ~5-20ms
- Fusion + reranking: ~5-15ms

**Total expected:** 40-115ms per query

---

## Best Practices

### Before Making Changes

1. Run evaluation on current code to establish baseline
2. Note MRR, NDCG@5, and zero-result rate
3. Check per-bucket metrics for any pre-existing issues

### After Making Changes

1. Run evaluation with same query/qrel files
2. Compare all metrics against baseline
3. Check for regressions per criteria above
4. Review diagnostics for latency changes
5. Document any intentional metric trade-offs

### Adding New Queries

1. Follow TSV format exactly
2. Assign unique query IDs
3. Create qrels with at least 2 relevant docs per query
4. Classify query into appropriate bucket
5. Test query manually before adding to benchmark

### Maintaining Qrels

1. Use graded relevance (0-3 scale)
2. Include authoritative sources at grade 3
3. Include related concepts at grade 1-2
4. Review qrels periodically as index content changes

---

## Troubleshooting

### Common Issues

**"No queries evaluated"**
- Check query IDs match between queries and qrels files
- Verify TSV format is correct

**"All queries return zero results"**
- Check search stack loaded correctly
- Verify index is populated
- Check embedding service is available

**"MRR suddenly dropped"**
- Check for code changes in ranking weights
- Verify qrels file hasn't changed
- Compare diagnostics for candidate count changes

**"High zero-result rate"**
- Check if index contains relevant documents
- Verify query tokenization
- Check for overly strict filtering

---

## Related Files

- `src/bin/eval.rs` - Evaluation binary
- `src/eval/metrics.rs` - Metric computation
- `src/eval/queries.rs` - Query loading and classification
- `src/eval/qrels.rs` - Qrels loading
- `src/eval/profiling.rs` - Query profiling
- `benchmarks/niche_db/` - Benchmark data
