# Niche Corpus Playbook

This playbook defines a focused evaluation track to demonstrate where lexical search is strong and to avoid misleading quality claims from generic web-backed assistants.

## Why A Niche Corpus

A broad corpus rewards generic recall. It is weaker for proving precision on technical terms where exact token match matters.

Use a niche corpus to measure:
- exact identifier retrieval (`wal_level`, `synchronous_commit`, `SERIALIZABLE`)
- acronym disambiguation (`WAL` in databases vs unrelated expansions)
- versioned and standards-heavy terminology

## Topic Scope

Use one scope at a time. Start with:
- database internals and transactions

Recommended seed file:
- `seeds.niche.md`

## Build The Niche Corpus

Use a dedicated config path to keep artifacts isolated from the main corpus.

1) Copy `config.high_quality.toml` to a niche profile and change only paths/seeds:

- `paths.db_path = "./crawl_data.niche"`
- `paths.index_path = "./hnsw_index.niche.bin"`
- `paths.lexical_index_path = "./lexical_index.niche"`
- `paths.vector_delta_path = "./hnsw_delta.niche.bin"`
- `paths.seeds_path = "./seeds.niche.md"`

2) Run the normal pipeline with that config:

```bash
SEARCH_ENGINE_CONFIG_PATH=./config.niche.toml ./scripts/fresh_high_quality_pipeline.sh ./config.niche.toml
```

## Build A Ground-Truth Eval Set

Create two files for this niche:
- `benchmarks/niche_db/queries.tsv`
- `benchmarks/niche_db/qrels.tsv`

Format:
- queries: `query_id<TAB>query_text`
- qrels: `query_id<TAB>0<TAB>doc_id<TAB>relevance`

Tip:
- use canonical URLs as `doc_id`
- include both exact-match and paraphrase queries

## Compare Retrieval Modes Fairly

Run at least these conditions on the same query set:
- lexical enabled (normal hybrid)
- lexical disabled (dense only)

How to disable lexical quickly:
- temporarily set `paths.lexical_index_path` to a non-existent path in a copied config profile

Then run:

```bash
SEARCH_ENGINE_CONFIG_PATH=./config.niche.hybrid.toml cargo run --release --bin eval -- --qrels benchmarks/niche_db/qrels.tsv --queries benchmarks/niche_db/queries.tsv --k 1,3,5,10
SEARCH_ENGINE_CONFIG_PATH=./config.niche.dense.toml cargo run --release --bin eval -- --qrels benchmarks/niche_db/qrels.tsv --queries benchmarks/niche_db/queries.tsv --k 1,3,5,10
```

Keep both JSON reports and diff MRR/NDCG/Recall.

## What To Claim (And What Not To)

Safe claim:
- "On a curated niche corpus with explicit qrels, lexical-heavy retrieval improves exact-term precision."

Avoid this claim:
- "Web-backed chat quality is low."

Reason:
- chat systems optimize end-task helpfulness, not pure retrieval metrics
- they are not constrained to your corpus, ranking policy, or doc IDs
- cross-system comparisons are invalid without shared corpus and labels

## Better Comparative Framing

Use this framing in demos:
- "This benchmark isolates retrieval quality on exact technical terminology."
- "We compare retrieval modes under identical corpus and qrels."
- "This is a retrieval evaluation, not a general assistant evaluation."

## Minimal Query Starter Pack (Database Internals)

- `what does wal_level logical enable`
- `sqlite wal checkpoint truncate behavior`
- `postgres serializable isolation anomaly prevention`
- `difference between repeatable read and serializable postgres`
- `what is full_page_writes in postgres wal`
- `sqlite locking states shared reserved pending exclusive`
- `mvcc snapshot visibility postgres`
- `vacuum freeze xid wraparound`

## Next Iteration

After baseline results are stable, add one more niche only:
- compiler internals, or
- kernel scheduling

Do not mix many niches in one experiment if the goal is clear causal conclusions.
