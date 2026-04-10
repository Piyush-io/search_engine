# Week 4 Update (Implemented)

## Scope
- Vector index build + query pipeline.

## Implemented
- `src/search/hnsw.rs`
  - `HnswIndex` wrapper implemented as brute-force serialized index (temporary backend)
  - supports insert/search/save/load
- `src/bin/index.rs`
  - loads all embeddings from RocksDB
  - builds index and saves to `config.paths.index_path`
- `src/search/query.rs`
  - query embedding + top-k retrieval from index
  - chunk hydration from RocksDB `chunks` CF

## Notes
- Module boundary stays HNSW-compatible.
- Swap to real `hnswlib-rs` backend in-place later without changing higher layers.
