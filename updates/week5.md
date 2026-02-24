# Week 5 Update (Implemented)

## Scope
- Web server + SERP + click tracking.

## Implemented
- `src/main.rs`
  - axum server with routes:
    - `GET /` home page
    - `GET /search?q=...` result page
    - `GET /act?d=...` tracked redirect
  - loads RocksDB + search index at startup
- `src/web/serp.rs`
  - SSR HTML rendering for home and results pages
  - result cards with tracked redirect links
  - knowledge panel sidebar rendering hook
- `src/web/tracking.rs`
  - signed + base64url click payload encode/decode
- Click logging
  - writes click events into RocksDB `clicks` CF

## Notes
- UI is functional and minimal SSR-first.
- Askama templates are still scaffolded; current rendering is string-based SSR.
