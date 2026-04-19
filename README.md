# search_engine

Neural / hybrid search engine for computer science content.

## Main entrypoints

- Web server: `cargo run --release --bin search_engine`
- Full rebuild pipeline: `./start_pipeline.sh`
- Incremental update pipeline: `./update_pipeline.sh`
- Retrieval debugger TUI: `npm --prefix tui run tui`

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
- `prune_low_quality_hosts`
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

## Retrieval X-Ray TUI

The terminal debugger is built with `@opentui/core` and reads the live Rust debug API exposed by the web server.

1. Start the server with `cargo run --bin search_engine`.
2. Install TUI dependencies with `npm --prefix tui install`.
3. Launch the TUI with `npm --prefix tui run tui`.

If you are using the Modal web deployment instead of a local server, point the TUI at that base URL:

- `RETRIEVAL_XRAY_API_URL=https://<your-modal-search-endpoint>`

Optional environment variables:

- `RETRIEVAL_XRAY_API_URL` — debug API base URL. Default: `http://127.0.0.1:3000`
- `RETRIEVAL_XRAY_QRELS` — local qrels TSV path for aggregate evaluation
- `RETRIEVAL_XRAY_QUERIES` — local queries TSV path for aggregate evaluation
- `RETRIEVAL_XRAY_K` — comma-separated metric cutoffs. Default: `1,3,5,10`
- `RETRIEVAL_XRAY_INITIAL_QUERY` — query shown on launch

The TUI consumes:

- `/debug/api/search` for chunk-level score decomposition and failure-mode heuristics
- `/debug/api/eval` for MRR, NDCG@k, Recall@k, and worst-query inspection
