# Search Engine Plan

## Goal

Turn the search engine into a quality-first, incrementally updatable CS search system that can grow the corpus over time without requiring a full rebuild for every new page.

## Current Status

- The original corpus and serving path are operational, including incremental queues, lexical deletes, vector tombstones, and full compaction/rebuild support.
- A full remote Modal rebuild path now exists for heavy embedding and index rebuild work.
- Post-prune compaction on the existing corpus succeeded, but retrieval quality still appears to be limited more by corpus composition and ranking than by rebuild mechanics.
- Work has shifted to an isolated fresh corpus experiment using `config.high_quality.toml` and `seeds.high_quality.md` so new corpus-building work does not disturb the current main corpus.
- The fresh corpus crawl is already underway in `crawl_data.high_quality`.
- Phase 2 and Phase 3 for the isolated corpus have now been completed on Modal against the mounted local workspace snapshot.
- The current isolated high-quality snapshot is queryable with `stored_pages=80801`, `stored_chunks=4507321`, `stored_embeddings=4243078`, zero work queues, and zero vector delta/tombstones.
- Verification on the fresh corpus shows strong Rust-doc results, decent but still noisy lower ranks for TCP handshake queries, improved B-tree handling, and remaining duplicate-result/canonicalization work for `sqlite.org` vs `www.sqlite.org`.
- Public corpus import is now a first-class path via `import_pages_jsonl`, so curated external datasets can be routed through the same normalize/embed/index pipeline as crawled pages.

## What Is Already Done

- Seeds are now loaded from `seeds.md` through config instead of being hardcoded in `src/bin/crawl.rs`.
- Config loading now supports `SEARCH_ENGINE_CONFIG_PATH`, so alternate corpus profiles can be run without swapping files.
- Crawl host policy is stricter, so noisy subdomains do not leak into the frontier as easily.
- Normalization now uses page manifests and content-hash-based chunk IDs.
- Stale chunks, stale embeddings, lexical deletes, and vector tombstones are now tracked.
- Embedding now has two modes:
  - incremental queue-driven mode
  - full-scan rebuild mode with `--full-scan`
- Vector indexing now has two modes:
  - incremental delta-index updates
  - full rebuild with `--full`
- Lexical indexing now has two modes:
  - incremental upserts/deletes
  - full rebuild with `--full`
- The server now loads a base vector index plus a delta overlay.
- A dedicated high-quality corpus profile now exists:
  - `config.high_quality.toml`
  - `seeds.high_quality.md`
  - `scripts/fresh_high_quality_pipeline.sh`
- A dedicated Modal path now exists for the isolated corpus, including `sync_high_quality_db` and `phase23_high_quality` in `modal_app.py`.
- Public JSONL page import now exists via `import_pages_jsonl`, which writes into `content` plus `normalize_queue` and reuses the existing downstream pipeline.

## Fresh Corpus Status And Next Steps

### Phase 1: Finish The Isolated Fresh Crawl

Continue the crawl that targets `crawl_data.high_quality` until the frontier meaningfully stabilizes or the configured page target is reached.

Run:

```bash
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/crawl
```

Expected result:

- the fresh corpus grows in `crawl_data.high_quality`
- the main corpus in `crawl_data` remains untouched
- host mix can be reviewed independently with `domain_stats`

### Phase 2: Run The Sequential Clean Build

Status:

- completed on Modal for the current isolated snapshot
- remote artifacts were rebuilt at `/data/hnsw_index.high_quality.bin` and `/data/lexical_index.high_quality`

Re-run when the isolated crawl advances materially or after importing new public corpora.

Once the fresh crawl is paused at a reasonable checkpoint, run the normal sequential build path against the isolated corpus.

Run:

```bash
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/normalize_pages
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/embed --full-scan
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/index --full
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/lexical_index --full
```

Expected result:

- all crawled pages are normalized into chunk manifests
- a fresh base vector index is built with no delta overlay dependence
- a fresh lexical index is built from the same chunk set
- the isolated corpus becomes queryable end-to-end

### Phase 3: Verify The Fresh Corpus

Status:

- completed on Modal for the current isolated snapshot
- verification report was written to `/results/reports/phase23_high_quality_remote.txt`
- last verified snapshot had `stored_pages=80801`, `stored_chunks=4507321`, `stored_embeddings=4243078`
- queues were `normalize_queue=0`, `embed_queue=0`, `vector_queue=0`, `lexical_queue=0`
- `base_index_exists=true`, `vector_delta_entries=0`, and `vector_tombstones=0`

Re-run after new crawl progress or after any public-corpus import.

Run:

```bash
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/stats
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/queue_stats
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/index_stats
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/domain_stats --limit 50
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/sample_query "what is a B-tree" 5
```

Check:

- chunk count and embedding count are in the expected range
- queue sizes are near zero after rebuild
- the base index exists and delta/tombstones remain at zero or near zero
- representative queries return plausible sources for real CS questions

Observed query-quality notes from the last verification run:

- `rust lifetime elision rules` was strong, with Rust Reference and Rust Book at the top
- `tcp three-way handshake` had the correct MDN result at rank 1 but still showed lower-rank noise
- `what is a B-tree` improved materially versus the older corpus but still does not consistently lead with the cleanest definition-first result
- `sqlite wal checkpoint` returned correct top results but still showed `sqlite.org` and `www.sqlite.org` duplicates

### Phase 4: Import Curated Public Corpora

Augment the isolated fresh corpus with curated public sources that already fit the page-based pipeline well.

Recommended order:

1. selected Stack Exchange dumps
2. cleaned Wikipedia slice
3. optionally other curated technical datasets

Run:

```bash
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml \
cargo run --release --bin import_pages_jsonl -- path/to/corpus.jsonl
```

Then re-run:

```bash
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/normalize_pages
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/embed --full-scan
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/index --full
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/lexical_index --full
```

Expected result:

- external corpora flow through the same page/chunk/index pipeline as crawled data
- quality can be improved with curated imports without changing storage or serving architecture

## Immediate Decision Point

The plan is now at a fork:

1. continue Phase 1 until the isolated crawl stabilizes further, then re-run Phase 2 and Phase 3
2. move to Phase 4 and start importing curated public corpora into the isolated corpus
3. pause corpus growth work and do a ranking-quality pass for the duplicate-result and canonicalization issues found during Phase 3 verification

## Ongoing Operating Plan

### Incremental Update Path

Use:

```bash
./update_pipeline.sh
```

This does:

1. crawl new pages
2. normalize changed pages
3. embed queued chunks
4. update the vector delta index
5. update the lexical index

Use this as the normal daily or periodic workflow for the main corpus.

### Full Rebuild / Compaction Path

Use:

```bash
./start_pipeline.sh
```

Use a full rebuild when:

- the vector delta file grows large
- tombstones accumulate
- retrieval quality looks stale or inconsistent
- you change chunking, embedding, or index structure
- you want to compact accumulated incremental state into a fresh base snapshot

### Corpus Cleanup Path

Use this when you want to remove already-stored low-value pages from the corpus before a rebuild.

Start with a dry run:

```bash
cargo run --release --bin prune_low_quality_hosts -- --dry-run
```

Apply the safe cleanup path for pages that no longer pass the current crawl policy:

```bash
cargo run --release --bin prune_low_quality_hosts
cargo run --release --bin lexical_index
cargo run --release --bin index
```

If you want a stricter quality-first cleanup, also prune stored pages on hosts outside the current high-quality set:

```bash
cargo run --release --bin prune_low_quality_hosts -- --prune-non-high-quality
./start_pipeline.sh
```

Review the dry run carefully before using `--prune-non-high-quality`: it follows the stricter `host_is_high_quality` set used for recrawl policy, which is narrower than the broader crawl allowlist.

Expected result:

- disallowed or low-quality stored pages are removed from `crawl_data`
- associated chunks, embeddings, lexical deletes, and vector tombstones are queued consistently
- incremental index updates can apply the delete set
- a full rebuild can compact the cleaned corpus into a fresh snapshot when the delete set is large

## Delta Index Policy

The current design uses:

- base HNSW snapshot for the stable corpus
- small brute-force delta index for incremental updates
- tombstones for deleted or replaced chunk IDs

Operational policy:

- keep using incremental updates while the delta stays moderate
- trigger a full rebuild when delta size becomes operationally large
- the current code prints a rebuild hint once the delta reaches about 100k entries

This keeps the system incremental now while leaving room to scale corpus size upward later.

## Corpus Growth Strategy

Do not treat growth as "more pages at any cost." Grow by preserving corpus quality first.

### Priority Sources

- official docs
- free textbooks
- course notes
- canonical references
- expert technical blogs
- narrowly-scoped high-signal Q&A
- selected Stack Exchange dumps
- Wikipedia or other strong reference corpora
- curated research sources where they improve real query quality

### Lower Priority Sources

- unfiltered broad UGC
- news aggregators
- vendor community forums
- operational subdomains
- high-noise blog platforms

### Growth Rules

- expand seeds gradually
- import curated public corpora before loosening crawl policy broadly
- keep per-host caps explicit
- review `domain_stats` output regularly
- add new hosts only when they improve retrieval quality for actual CS queries
- prefer adding new high-quality domains over loosening host matching rules

## Recommended Capacity Strategy

The code now leaves room for a larger corpus, but growth should still be staged.

Suggested operating stages:

- Stage 1: keep the corpus around the current scale until incremental updates are stable
- Stage 2: grow the isolated high-signal corpus while monitoring RAM, queue sizes, and index sizes
- Stage 3: merge the best fresh-corpus approach back into the main corpus only after quality checks justify it
- Stage 4: if the base index becomes too heavy for local serving, move full rebuilds and heavy compactions off-machine while keeping local incremental serving

## Follow-Up Engineering Tasks

### High Priority

- add a recrawl policy with freshness windows and conditional HTTP requests
- store `etag` and `last-modified` per page in page state
- build a converter for selected Stack Exchange dumps into `import_pages_jsonl` format
- build a converter for a cleaned Wikipedia slice into `import_pages_jsonl` format
- compare fresh-crawl-only quality versus crawl-plus-import quality on representative CS queries
- harden corruption reporting and recovery paths further

### Medium Priority

- split seeds into structured config with metadata like tier, depth, and recrawl cadence
- add richer per-domain scoring rules
- add periodic compaction automation based on delta size or tombstone count
- improve operational dashboards for queue and corpus health
- add lightweight quality benchmarking for a fixed query set

### Future Scaling Options

- shard the base index by topic or host class
- use a small HNSW delta instead of brute-force if needed
- move full embedding/index compactions to Modal or another remote machine
- add a scheduler for background recrawl windows

## Success Criteria

This work is successful when:

- new pages can be added without a full rebuild
- changed pages replace stale chunks cleanly
- deleted/replaced chunks stop appearing in results
- lexical and vector indexes stay aligned with chunk state
- the corpus can grow gradually without architecture changes
- curated public corpora can be imported through the same page pipeline as crawled pages
- full rebuilds become a compaction tool, not the default way to operate
