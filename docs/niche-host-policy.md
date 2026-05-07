# Niche Host Policy

## Overview

The Niche Host Policy system reduces noisy host dominance and improves top-rank precision for exact technical queries in specialized domains (PostgreSQL, SQLite, etc.). This is a profile-scoped feature that activates only when configured explicitly, preserving backward compatibility.

## Rationale

### Problem

- Aggregator sites (Stack Overflow, Medium, tutorials) often dominate results for technical queries
- Official documentation gets buried under SEO-optimized content
- For exact technical queries (e.g., `wal_level postgresql`), users want canonical sources first
- Current system uses dynamic host caps but doesn't distinguish source quality

### Solution

Profile-scoped host policy controls that:
1. Boost canonical/trusted hosts (PostgreSQL.org, sqlite.org)
2. Penalize noisy aggregator/tutorial hosts
3. Enforce hard caps per host in top-k results

## Configuration

### Fields in `RankingConfig`

```toml
[ranking]
# Multiplier for canonical hosts (1.0 = neutral, >1.0 boosts)
host_allowlist_boost = 1.15

# Penalty multiplier for noisy hosts (1.0 = neutral, <1.0 penalizes)
host_soft_penalty = 0.85

# Hard cap on results per host (None = unlimited, recommended: 3-5)
host_hard_cap = 3

# Canonical hosts to boost
host_allowlist = [
    "postgresql.org",
    "www.postgresql.org", 
    "wiki.postgresql.org",
    "sqlite.org",
    "www.sqlite.org",
]

# Noisy hosts to penalize
host_penalty_list = [
    "stackoverflow.com",
    "medium.com",
    "dev.to",
    "tutorialspoint.com",
    "w3schools.com",
    "geeksforgeeks.org",
]
```

### Backward Compatibility

All host policy fields have serde defaults:
- `host_allowlist_boost`: 1.0 (no boost)
- `host_soft_penalty`: 1.0 (no penalty)
- `host_hard_cap`: None (unlimited)
- `host_allowlist`: [] (empty)
- `host_penalty_list`: [] (empty)

Existing configs without these fields continue to work unchanged.

## Implementation Details

### Host Extraction

The system extracts canonical hosts from URLs:
- Normalizes to lowercase
- Strips `www.` prefix
- Handles special cases (e.g., Rust docs aliases)

### Host Classification

During candidate scoring, each result is classified:
- `Canonical`: In allowlist → score × `host_allowlist_boost`
- `Noisy`: In penalty list → score × `host_soft_penalty`
- `Neutral`: Not in any list → score unchanged

### Hard Cap Enforcement

If `host_hard_cap` is set:
- Limits results per canonical host in final selection
- Applied during `fill_results()` alongside existing deduplication
- Falls back to dynamic cap calculation if not set

## Guardrails

1. **No global hardcoded bans**: Hosts are only penalized via config, never removed
2. **Multiplicative only**: Scores are multiplied, never set to zero
3. **Profile-scoped**: Only active in `config.niche.toml`, safe defaults elsewhere
4. **No destructive changes**: Corpus remains intact, only ranking changes

## Recommended Settings

### Conservative (safe default)
```toml
host_allowlist_boost = 1.05  # Slight boost
host_soft_penalty = 0.95     # Slight penalty
host_hard_cap = 5            # Moderate diversity
```

### Aggressive (niche focus)
```toml
host_allowlist_boost = 1.20  # Strong boost
host_soft_penalty = 0.75     # Strong penalty
host_hard_cap = 2            # High diversity
```

### Balanced (current niche profile)
```toml
host_allowlist_boost = 1.15
host_soft_penalty = 0.85
host_hard_cap = 3
```

## Evaluation

To measure impact:

```bash
# Baseline (without host policy)
SEARCH_ENGINE_CONFIG_PATH=config.toml cargo run --release --bin eval -- --queries eval/queries_100.toml

# Niche (with host policy)
SEARCH_ENGINE_CONFIG_PATH=config.niche.toml cargo run --release --bin eval -- --queries eval/queries_100.toml
```

### Expected Improvements

- **ExactIdentifier bucket**: Higher MRR as canonical docs rank higher
- **AcronymDisambiguation bucket**: Reduced noise from aggregators
- **Overall**: Slight recall decrease acceptable for precision gain

## Future Enhancements

1. **Query-aware host selection**: Different allowlists per query type
2. **Dynamic host learning**: Learn from click patterns
3. **Host quality scoring**: Continuous quality assessment
4. **Cross-profile tuning**: Separate configs for different domains

## Files Modified

- `src/config.rs`: Added host policy fields with serde defaults
- `src/search/query.rs`: Host extraction, classification, and policy application
- `config.niche.toml`: Niche-specific host policy settings
- `docs/niche-host-policy.md`: This documentation
