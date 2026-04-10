# search_engine

Neural / hybrid search engine for computer science content.

## Main entrypoints

- Web server: `cargo run --release --bin search_engine`
- Full rebuild pipeline: `./start_pipeline.sh`
- Incremental update pipeline: `./update_pipeline.sh`

## Repository layout

- `src/` — library code and production binaries
- `examples/milestones/` — older task/milestone binaries kept for reference
- `scripts/` — operational shell scripts
- `docs/` — architecture notes, plans, seeds, updates, reports, assets
- `training/` — Python training/export helpers

## Production binaries

The actively used binaries live in `src/bin/`, including:

- `crawl`
- `normalize_pages`
- `embed`
- `index`
- `lexical_index`
- `wiki_ingest`
- `wiki_embed`
- `wiki_index`
- `stats`
- `domain_stats`
- `queue_stats`
- `index_stats`
- `merge_db`
- `requeue_all_pages`
- `clear_embeddings`
- `label`
- `link_harvest`
- benchmarks: `bench`, `bench_embed`, `bench_ann`

## Notes

- Older `task*.rs` learning artifacts were moved out of `src/bin/` to reduce clutter.
- Root-level `start_pipeline.sh` and `update_pipeline.sh` are thin wrappers around `scripts/` for backwards compatibility.
- Large runtime data (`crawl_data`, `lexical_index`, HNSW artifacts, backups/restores) should stay untracked.
