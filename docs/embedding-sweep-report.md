# Embedding Sweep Report

This document reports on the embedding quality/performance sweep conducted to find optimal embedding profiles for niche DB retrieval.

## Overview

The sweep compares different embedding configurations across:
- **Throughput**: Items embedded per second
- **Quality**: MRR and NDCG metrics from retrieval evaluation
- **Resource usage**: Memory, CPU/GPU utilization
- **Index characteristics**: Build time, search latency

## Tested Configurations

### 1. `bge-small-fast` (Recommended for Quick Iteration)
```toml
model = "bge-small-en-v1.5"
dim = 384
max_length = 128
batch_size = 64
backend = "cuda"
bulk_workers = 1
bulk_intra_threads = 2
```

**Characteristics:**
- Fastest embedding throughput
- Lower memory footprint (384-dim vectors)
- Shorter context window (128 tokens)
- Good for rapid experimentation and development

### 2. `bge-small-quality` (Balanced Option)
```toml
model = "bge-small-en-v1.5"
dim = 384
max_length = 256
batch_size = 32
backend = "cuda"
bulk_workers = 1
bulk_intra_threads = 2
```

**Characteristics:**
- Same model as fast profile but with longer context
- Better handling of longer documents
- Slightly reduced throughput due to longer sequences
- Good balance of speed and quality

### 3. `bge-base-quality` (Recommended for Production)
```toml
model = "bge-base-en-v1.5"
dim = 768
max_length = 256
batch_size = 16
backend = "cuda"
bulk_workers = 1
bulk_intra_threads = 2
```

**Characteristics:**
- Higher quality embeddings (768-dim)
- Better semantic understanding
- Lower throughput (larger model)
- Higher memory usage
- Best retrieval quality for production

### 4. `cpu-fallback` (No GPU Option)
```toml
model = "bge-small-en-v1.5"
dim = 384
max_length = 128
batch_size = 32
backend = "cpu"
bulk_workers = 2
bulk_intra_threads = 4
```

**Characteristics:**
- CPU-only execution (no GPU required)
- Uses more workers for parallelism
- Significantly slower than GPU
- Useful for CI/testing environments

## Expected Results

### Throughput Estimates (Approximate)

| Profile | Expected Throughput | Relative Speed |
|---------|--------------------|----------------|
| bge-small-fast | ~200-400 items/sec | 1.0x (baseline) |
| bge-small-quality | ~150-300 items/sec | ~0.75x |
| bge-base-quality | ~50-100 items/sec | ~0.25x |
| cpu-fallback | ~10-30 items/sec | ~0.1x |

*Note: Actual throughput depends on hardware (GPU model), document length distribution, and concurrent system load.*

### Quality Metrics (Expected Range)

| Profile | Expected MRR | Expected NDCG@5 | Expected NDCG@10 |
|---------|-------------|----------------|------------------|
| bge-small-fast | 0.35-0.45 | 0.30-0.40 | 0.35-0.45 |
| bge-small-quality | 0.38-0.48 | 0.33-0.43 | 0.38-0.48 |
| bge-base-quality | 0.45-0.55 | 0.40-0.50 | 0.45-0.55 |
| cpu-fallback | 0.35-0.45 | 0.30-0.40 | 0.35-0.45 |

*Note: Quality metrics depend on the specific benchmark dataset and domain.*

## Running the Sweep

### Prerequisites

1. Ensure all embedding models are cached:
```bash
# The models will be downloaded automatically on first use
# Check .fastembed_cache/ directory
```

2. Have crawl data ready for each profile:
```bash
# For each profile, run crawl with its config
SEARCH_ENGINE_CONFIG_PATH=config.embed_bge_small_fast.toml cargo run --release --bin crawl
```

### Running All Profiles

```bash
./scripts/run_embedding_sweep.sh
```

### Running Single Profile

```bash
./scripts/run_embedding_sweep.sh bge_small_fast
```

### Output Structure

Reports are saved to `reports/embedding_sweep/`:
```
reports/embedding_sweep/
├── {profile}_embed_{timestamp}.log       # Embedding output
├── {profile}_index_{timestamp}.log       # Indexing output
├── {profile}_eval_{timestamp}.log        # Evaluation output
├── {profile}_{timestamp}.json            # Timing report (from --timing)
└── {profile}_summary_{timestamp}.json    # Aggregated summary
```

## Recommendations

### Fast Profile: `bge-small-fast`

**Use when:**
- Rapid iteration during development
- CI/CD pipelines
- Limited GPU memory
- Quick proof-of-concepts
- Documents are mostly short (<128 tokens)

**Tradeoffs:**
- May miss semantic nuances in longer documents
- Lower absolute retrieval quality

### Quality Profile: `bge-base-quality`

**Use when:**
- Production deployment
- Maximum retrieval quality is critical
- Documents contain complex semantic content
- Sufficient GPU memory available (768-dim vectors)
- Embedding time is not a bottleneck

**Tradeoffs:**
- 4x slower embedding
- 2x memory usage for vectors
- Larger index size

### Balanced Profile: `bge-small-quality`

**Use when:**
- Need better quality than fast profile
- Documents frequently exceed 128 tokens
- GPU memory is constrained
- Good middle ground for most use cases

### CPU Fallback: `cpu-fallback`

**Use when:**
- No GPU available
- Testing environments
- Small datasets
- Background/batch processing where speed is not critical

## Implementation Notes

### Timing Measurement

The sweep captures:
1. **Wall time**: Total elapsed time from start to finish
2. **Embed time**: Time spent in actual model inference
3. **Write time**: Time spent writing to RocksDB
4. **Throughput**: Items processed / wall time

### Quality Evaluation

Evaluation uses:
- 100 queries from `benchmarks/niche_db/queries_100.tsv`
- Relevance judgments from `benchmarks/niche_db/qrels_100.tsv`
- MRR (Mean Reciprocal Rank) at k=10
- NDCG (Normalized Discounted Cumulative Gain) at k=5 and k=10

### Query Latency

Approximate per-query latency is captured during evaluation. This includes:
- Query embedding time
- Vector search time (HNSW)
- Lexical search time (if enabled)
- Reranking time

## Configuration Reference

All profiles share the same base configuration except for embedding parameters:

| Parameter | Fast | Small-Quality | Base-Quality | CPU |
|-----------|------|---------------|--------------|-----|
| Model | bge-small | bge-small | bge-base | bge-small |
| Dimensions | 384 | 384 | 768 | 384 |
| Max Length | 128 | 256 | 256 | 128 |
| Batch Size | 64 | 32 | 16 | 32 |
| Backend | cuda | cuda | cuda | cpu |
| Workers | 1 | 1 | 1 | 2 |
| Intra Threads | 2 | 2 | 2 | 4 |

## Future Improvements

Potential enhancements to the sweep framework:

1. **Dynamic batch size tuning**: Automatically find optimal batch size per hardware
2. **Memory profiling**: Track peak memory usage per profile
3. **Concurrent query load test**: Test search performance under load
4. **Multi-GPU support**: Distribute embedding across multiple GPUs
5. **Mixed precision**: Test FP16 vs FP32 embedding quality/performance
6. **Quantized models**: Evaluate `bge-*-q` quantized variants

## Troubleshooting

### Common Issues

**CUDA out of memory:**
- Reduce batch_size
- Use bge-small instead of bge-base
- Reduce max_length
- Close other GPU applications

**Slow CPU embedding:**
- Expected - CPU is significantly slower than GPU
- Consider using fewer bulk_workers (reduce overhead)
- Increase bulk_intra_threads for better CPU utilization

**Model not found:**
```bash
# First run will download models to .fastembed_cache/
# Ensure you have internet connectivity for first run
```

**Dimension mismatch errors:**
- Ensure config.embedding.dim matches the model's actual output dimension
- 384 for bge-small, 768 for bge-base
