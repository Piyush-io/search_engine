# Search Engine Plan

## Goal

Turn the search engine into a quality-first, incrementally updatable CS search system that can grow the corpus over time without requiring a full rebuild for every new page.

## What Is Already Done

- Seeds are now loaded from `seeds.md` through config instead of being hardcoded in `src/bin/crawl.rs`.
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

## Immediate Migration Plan

### Phase 1: Migrate Existing Corpus

The existing corpus was created before page manifests existed, so old pages do not yet have tracked chunk lineage.

Run:

```bash
cargo run --release --bin requeue_all_pages -- --reset-derived
./start_pipeline.sh
```

Expected result:

- all stored pages are requeued for normalization
- old chunks and embeddings are cleared
- lexical index is rebuilt from scratch
- base vector index is rebuilt from scratch
- new page manifests are created for all current pages

### Phase 2: Verify Migration

Run:

```bash
cargo run --release --bin stats
cargo run --release --bin domain_stats
```

Check:

- chunk count and embedding count are in the expected range
- queue sizes are near zero after rebuild
- noisy hosts are no longer dominating the frontier
- the server can load the rebuilt index set successfully

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

Use this as the normal daily or periodic workflow.

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

### Lower Priority Sources

- broad UGC
- news aggregators
- vendor community forums
- operational subdomains
- high-noise blog platforms

### Growth Rules

- expand seeds gradually
- keep per-host caps explicit
- review `domain_stats` output regularly
- add new hosts only when they improve retrieval quality for actual CS queries
- prefer adding new high-quality domains over loosening host matching rules

## Recommended Capacity Strategy

The code now leaves room for a larger corpus, but growth should still be staged.

Suggested operating stages:

- Stage 1: keep the corpus around the current scale until incremental updates are stable
- Stage 2: grow to a larger high-signal corpus while monitoring RAM, queue sizes, and delta growth
- Stage 3: if the base index becomes too heavy for local serving, move full rebuilds and heavy compactions off-machine while keeping local incremental serving

## Follow-Up Engineering Tasks

### High Priority

- add a recrawl policy with freshness windows and conditional HTTP requests
- store `etag` and `last-modified` per page in page state
- add a command to inspect queue sizes directly
- add a command to inspect delta index size and tombstone count
- harden corruption reporting and recovery paths further

### Medium Priority

- split seeds into structured config with metadata like tier, depth, and recrawl cadence
- add richer per-domain scoring rules
- add periodic compaction automation based on delta size or tombstone count
- improve operational dashboards for queue and corpus health

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
- full rebuilds become a compaction tool, not the default way to operate
