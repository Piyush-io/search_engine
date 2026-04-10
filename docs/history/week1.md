# Week 1 Update (Implemented)

## Scope
- Refactor + URL canonicalization + extraction foundations + crawler wiring.

## Implemented
- `src/crawler/canon.rs`
  - HTTPS-only guard
  - host normalization + credential/port stripping
  - query sorting
  - trailing slash dedupe
  - control-char + length validation
- `src/crawler/dns.rs`
  - private/loopback/link-local filtering
  - async DNS resolution + retry/backoff
- `src/crawler/robots.rs`
  - robots.txt wildcard `Disallow` parsing + RocksDB caching
- `src/extraction/*`
  - metadata extraction (`title`, `description`, OpenGraph)
  - density filtering
  - semantic normalization with heading-aware text blocks
- `src/storage.rs`
  - centralized RocksDB column-family setup
- `src/bin/crawl.rs`
  - end-to-end crawl pipeline implemented:
    - frontier queue in RocksDB
    - rate limiting
    - DNS + robots filtering
    - HTML fetch + normalization
    - chunk generation + storage
    - link discovery and enqueue

## Remaining Follow-ups
- strict eTLD validation with `publicsuffix`
- richer table/list semantic context extraction
