# Week 7 Update (Planned)

## Scope
- Search quality evaluation + benchmark collection.

## Planned Execution
- Prepare 50-query benchmark set across factual + natural language + complex cases.
- Add benchmark scripts for:
  - crawl throughput
  - chunking throughput
  - embedding throughput
  - query latency
  - end-to-end latency
- Add recall evaluation:
  - brute-force baseline vs indexed retrieval
  - recall@10 across ef/search-equivalent parameter sweep

## Current Status
- Added `src/bin/bench.rs` for baseline query latency collection.
- Benchmark output target: `reports/benchmark_results.json`.
- Full recall + large query-set evaluation still pending.
