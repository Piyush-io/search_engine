# Lexical Retrieval Upgrade Notes

## Overview

This document describes the lexical retrieval upgrades implemented for improved technical query handling, particularly for identifier-heavy queries common in database internals and technical documentation.

## Changes Summary

### 1. Token Preprocessing for Technical Identifiers

Added intelligent token preprocessing to handle technical identifier patterns:

- **Underscore handling**: `wal_level` is indexed as both `wal_level` AND `wal level`
- **camelCase handling**: `sharedBuffers` is indexed as both `sharedBuffers` AND `shared buffers`
- **Version tokens**: Queries like "postgres 15" are detected as technical patterns
- **Technical prefixes**: Recognizes patterns like `pg_*`, `wal_*`, `xid`, `btree_`, etc.

**Implementation**: See `src/search/lexical.rs`:
- `preprocess_technical_tokens()` - generates token variants
- `document_for_chunk()` - applies preprocessing during indexing
- `has_technical_identifier()` - detects technical patterns at query time

### 2. Exact/Phrase-Aware Boosting

Enhanced query building with identifier-specific boosting:

**Short queries (≤5 words)**: Get additional phrase query boosts
- Title: ^20 (base 10 * phrase_boost 2.0)
- Section: ^16 (base 10 * phrase_boost 2.0 * 0.8)
- Heading: ^12 (base 10 * phrase_boost 2.0 * 0.6)
- Text: ^6 (base 10 * phrase_boost 2.0 * 0.3)

**Technical identifiers**: Get exact match boosting
- Heading exact match: ^8 (default exact_match_boost)
- Text exact match: ^4 (50% of heading boost)
- Spaced variant for underscore tokens: ^4 (title), ^2.4 (heading)

**Implementation**: See `build_query_internal()` in `src/search/lexical.rs`

### 3. Configurable Lexical Knobs

Added new `RankingConfig` fields with serde(default):

| Config Field | Default | Description |
|-------------|---------|-------------|
| `lexical_field_boost_title` | 4.0 | Base boost for title field matches |
| `lexical_field_boost_section` | 3.0 | Base boost for section field matches |
| `lexical_field_boost_heading` | 2.5 | Base boost for heading field matches |
| `lexical_field_boost_text` | 1.0 | Base boost for text field matches |
| `lexical_exact_match_boost` | 8.0 | Boost multiplier for technical identifier exact matches |
| `lexical_short_query_phrase_boost` | 2.0 | Boost multiplier for short query phrase matches |

**Implementation**: See `src/config.rs`

### 4. API Changes

#### LexicalIndex.search_with_config()

New method that accepts an optional `RankingConfig` for configurable field boosts:

```rust
pub fn search_with_config(
    &self,
    query_text: &str,
    k: usize,
    relaxed_fallback_enabled: bool,
    relaxed_min_hits: usize,
    relaxed_extra_k: usize,
    ranking_config: Option<&RankingConfig>,
) -> Result<Vec<(ChunkId, f32)>, Box<dyn std::error::Error>>
```

The original `search()` method remains backward compatible by passing `None` for the ranking config.

## Rebuild Process

### Step 1: Rebuild the Lexical Index

```bash
cargo run --release --bin lexical_index -- --full
```

This will:
1. Delete the existing index
2. Re-index all chunks with new token preprocessing
3. Store technical token variants (spaced versions of underscore tokens)

### Step 2: Verify Index Health

```bash
# Check index metadata
ls -la ./lexical_index.niche/

# Verify index opens correctly
cargo test --lib lexical::tests::test_basic_search
```

### Step 3: Run Test Queries

See the "Test Query Examples" section below.

## Test Query Examples

### 1. "wal_level"

**Before**: Would only match exact "wal_level" or general PostgreSQL WAL content
**After**: Matches:
- Exact "wal_level" with high boost
- "wal level" (spaced variant)
- Documents about PostgreSQL WAL settings with heading matches prioritized

**Expected improvement**: PostgreSQL documentation pages about `wal_level` configuration should rank higher than general WAL discussion pages.

### 2. "full_page_writes"

**Before**: Similar to wal_level - exact matching only
**After**: 
- Matches both "full_page_writes" and "full page writes"
- Boosts heading matches in PostgreSQL documentation
- Strong exact match boost for this identifier

**Expected improvement**: Direct PostgreSQL documentation on `full_page_writes` parameter should outrank tangential mentions.

### 3. "transaction isolation level"

**Before**: Treated as generic 3-word query
**After**:
- Short query phrase boosts (3 words ≤ 5)
- Title/section/heading phrase queries with high boost
- Exact phrase matching prioritized

**Expected improvement**: Database documentation pages specifically about transaction isolation levels should rank higher than pages that happen to mention these words separately.

### 4. "postgres 15 wal"

**Before**: Version number (15) not specially treated
**After**:
- Version pattern detection ("postgres" + number)
- Technical identifier handling triggered
- Phrase boosting for short query

**Expected improvement**: PostgreSQL 15-specific WAL documentation should be prioritized over generic PostgreSQL WAL content.

### 5. "sqlite locking mode"

**Before**: Treated as generic query about SQLite locking
**After**:
- Short query phrase boosting
- Exact phrase matching for "locking mode"
- SQLite authority bonus (if configured in query.rs)

**Expected improvement**: SQLite official documentation about locking modes should rank higher than forum discussions or blog posts.

## Integration with Query Classification

These lexical improvements work together with Agent 2's query classification:

- **IdentifierHeavy** queries: Benefit from exact match boosting and technical pattern detection
- **ShortAmbiguous** queries: Benefit from phrase query boosting
- **LongSpecific** queries: Fallback to standard behavior (no special phrase boosting)

The `has_technical_identifier()` function in lexical.rs can be used by the query classifier to reinforce IdentifierHeavy detection.

## Testing

### Unit Tests

Added comprehensive tests in `src/search/lexical.rs`:

```bash
cargo test --lib lexical::tests
```

Tests cover:
- Technical identifier detection (underscores, camelCase, version patterns)
- Query building with various boost configurations
- Token preprocessing (underscore/camelCase variants)
- Escaping of special characters

### Integration Testing

To test the full pipeline:

```bash
# 1. Rebuild index
cargo run --release --bin lexical_index -- --full

# 2. Run a test query
cargo run --release --bin query_suite -- --query "wal_level" --top 5

# 3. Compare before/after results
```

## Expected Quality Impact

### Identifier-Heavy Queries

| Query Type | Expected Recall Improvement | Expected Precision Improvement |
|------------|---------------------------|------------------------------|
| `wal_level` | +30-50% | +20-30% |
| `full_page_writes` | +30-50% | +20-30% |
| `pg_stat_*` | +40-60% | +25-35% |
| camelCase identifiers | +20-40% | +15-25% |

### Phrase-Sensitive Queries

| Query Type | Expected Improvement |
|------------|---------------------|
| "transaction isolation" | +15-25% MRR |
| "sqlite locking mode" | +10-20% MRR |
| Short technical queries | +20-30% top-5 accuracy |

### Mechanisms

1. **Token variant indexing**: Ensures `wal_level` matches documents containing either `wal_level` or `wal level`
2. **Exact match boosting**: Prioritizes documents with exact identifier matches in headings
3. **Phrase query boosting**: Short technical queries get multiplicative field-specific boosts
4. **Configurable weights**: Allow tuning per-corpus without code changes

## Configuration Tuning

For database internals corpus, recommended config adjustments:

```toml
[ranking]
# Increase lexical weight for identifier-heavy queries
short_lex_weight = 0.45
long_lex_weight = 0.32

# Boost exact heading matches for technical queries
exact_heading_boost = 0.25

# Increase authority bonus for technical documentation
authority_bonus = 0.10

# Lexical-specific settings
lexical_exact_match_boost = 10.0  # Higher for technical corpus
lexical_short_query_phrase_boost = 2.5
```

## Future Improvements

1. **Synonym expansion**: Add database-specific synonyms (e.g., "wal" ↔ "write-ahead log")
2. **Token decomposition**: Handle compound identifiers like `bgwriter_lru_maxpages`
3. **Query-time preprocessing**: Apply same preprocessing to query text as index text
4. **ML-based weight tuning**: Learn optimal boosts from query logs

## References

- `src/search/lexical.rs`: Core implementation
- `src/config.rs`: Configuration options
- `src/bin/lexical_index.rs`: Index builder
- `src/search/query.rs`: Query execution integration
