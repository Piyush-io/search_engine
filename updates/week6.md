# Week 6 Update (Implemented)

## Scope
- Wikipedia ingestion + knowledge panel.

## Implemented
- `src/knowledge/wikipedia.rs`
  - `WikiRecord` schema added
- `src/bin/wiki_ingest.rs`
  - ingests JSONL dump into RocksDB `wiki` CF
  - flexible field parsing (`summary` / `extract`)
- `src/knowledge/panel.rs`
  - lightweight lexical matcher for panel candidate retrieval
- SERP integration
  - knowledge panel rendered in `/search` when match exists

## Notes
- Dedicated wiki embedding index is pending.
- Current lexical matching keeps panel feature operational now.
