# Crawler V2 Architecture

## Goal

Build a crawler that is optimized for sustained throughput, strict host politeness, and high-signal page discovery for retrieval.

The core change from the current crawler is architectural:

- crawling becomes `discover + fetch + persist`
- normalization/chunking stop living in the fetch hot path
- scheduling moves from global RocksDB scans to per-host in-memory queues
- RocksDB becomes durable backing storage, not the live scheduler

## Design Principles

1. Keep the hot path short.
2. Never do global frontier scans during steady-state crawling.
3. Enforce politeness at the scheduler, not with ad hoc sleeps inside workers.
4. Do cheap reject decisions before network I/O.
5. Do heavy HTML normalization and chunk production after fetch, in a separate stage.
6. Batch all writes.
7. Treat host state as first-class data.

## High-Level Pipeline

```text
seed loader / link extractor
  -> frontier scheduler
  -> fetch workers
  -> parse workers
  -> persistence writer
  -> downstream normalization + chunking job
```

## Runtime Topology

### 1. Scheduler

Owns the live crawl state.

Responsibilities:

- maintain per-host queues
- enforce per-host next-eligible-at timestamps
- cap host parallelism at 1 active request by default
- dedupe URLs before they enter ready queues
- hand ready URLs to fetch workers

Core structures:

```rust
struct Scheduler {
    hosts: DashMap<HostKey, HostState>,
    ready_hosts: SegQueue<HostKey>,
    inflight_hosts: DashSet<HostKey>,
    seen_recent: moka::sync::Cache<Fingerprint, ()>,
}

struct HostState {
    queue: VecDeque<UrlTask>,
    next_allowed_at: Instant,
    robots: RobotsState,
    active_fetches: usize,
    persisted_count: usize,
    soft_cap: usize,
}

struct UrlTask {
    url: String,
    depth: u16,
    discovered_from: Option<String>,
    priority: i32,
}
```

Notes:

- `ready_hosts` contains host keys, not URLs.
- scheduler pops one URL from one host, then requeues the host if more work remains.
- this keeps host diversity without random frontier scans.

### 2. Fetch Workers

Pure network stage.

Responsibilities:

- GET page
- follow redirects
- validate final URL
- collect response metadata
- return raw HTML bytes/string

Must not do:

- heavy DOM extraction
- chunking
- RocksDB point writes for every micro-decision

Output:

```rust
struct FetchResult {
    requested_url: String,
    final_url: String,
    status: u16,
    content_type: Option<String>,
    fetched_at_ms: i64,
    html: Option<String>,
    x_robots_noindex: bool,
    reject_reason: Option<RejectReason>,
}
```

### 3. Parse Workers

CPU-bound stage using `spawn_blocking` or a rayon pool.

Responsibilities:

- parse HTML once
- extract canonical, title, description, noindex, outlinks
- compute a cheap text-quality score
- decide whether the page is worth persisting
- emit a lightweight parsed page artifact

Output:

```rust
struct ParsedPage {
    final_url: String,
    canonical_url: String,
    title: Option<String>,
    description: Option<String>,
    raw_text: String,
    cleaned_html: Option<String>,
    outlinks: Vec<String>,
    noindex: bool,
    quality: PageQuality,
}

struct PageQuality {
    text_bytes: usize,
    block_count: usize,
    link_density: f32,
    should_store: bool,
    reject_reason: Option<RejectReason>,
}
```

Important:

- V2 crawl stores page-level artifacts first.
- chunking becomes an asynchronous downstream pipeline over accepted pages.

### 4. Persistence Writer

Single-purpose batched write stage.

Responsibilities:

- batch RocksDB writes every N items or every T milliseconds
- atomically persist page record, outlink discoveries, host metadata, seen markers
- keep write amplification low

Write policy:

- use `WriteBatch`
- avoid one-write-per-URL patterns
- coalesce `seen`, `content`, `frontier`, and `host_meta` updates together

## New Crawl States

Use explicit state transitions:

```text
discovered
  -> scheduled
  -> fetching
  -> fetched
  -> parsed
  -> accepted | rejected
  -> normalized
  -> chunked
```

This is better than mixing "seen" and "stored" semantics.

## RocksDB Schema

Keep RocksDB, but restructure its responsibilities.

### Column Families

```text
host_meta            # host-level scheduler state and robots cache
frontier_log         # append-only discovered URL records
seen                 # durable URL fingerprints / canonical URLs
fetch_cache          # raw fetch outcomes and response metadata
pages                # accepted page-level parsed artifacts
outlinks             # optional edge store for debugging / recrawl
rejected             # rejected URL + reason + timestamp
normalize_queue      # accepted pages waiting for normalization/chunking
chunks               # existing chunk store
robots               # existing cached robots rules
metrics              # rolling counters/checkpoints
```

### Suggested Records

```rust
struct HostMetaRecord {
    host: String,
    next_allowed_epoch_ms: i64,
    active_fetches: u8,
    persisted_count: u32,
    discovered_count: u32,
    soft_cap: u32,
    robots_fetched_epoch_ms: Option<i64>,
}

struct FrontierRecord {
    url: String,
    host: String,
    depth: u16,
    priority: i32,
    discovered_from: Option<String>,
    discovered_epoch_ms: i64,
}

struct FetchCacheRecord {
    requested_url: String,
    final_url: Option<String>,
    status: Option<u16>,
    content_type: Option<String>,
    fetched_epoch_ms: i64,
    x_robots_noindex: bool,
    reject_reason: Option<String>,
}

struct PageRecordV2 {
    url: String,
    requested_url: String,
    title: Option<String>,
    description: Option<String>,
    text: String,
    canonical_url: Option<String>,
    fetched_epoch_ms: i64,
}
```

## Scheduler Algorithm

### Per-host fairness

Instead of scanning a global frontier and filtering it, do this:

1. enqueue discovered URL into the host queue
2. if host becomes ready and is not inflight, push host key to `ready_hosts`
3. worker pops a host key
4. scheduler checks:
   - host queue non-empty
   - `Instant::now() >= next_allowed_at`
   - robots already cached or lazily fetched once
5. scheduler pops one URL from that host and marks host inflight
6. on completion, scheduler advances `next_allowed_at` and requeues host if work remains

This avoids:

- full key-order RocksDB iteration
- batch-time random shuffling
- host rebucketing per batch
- repeated rediscovery of stale frontier entries

## Concurrency Model

### Recommended workers

```text
scheduler task:           1
fetch workers:            64-256 (network-bound)
parse workers:            num_cpus::get() or num_cpus::get_physical()
writer task:              1-2
normalization workers:    CPU pool, separate queue
```

### Channel layout

```text
scheduler -> fetch_tx -> fetch workers
fetch workers -> parse_tx -> parse workers
parse workers -> persist_tx -> writer
writer -> scheduler feedback channel
writer -> normalize_queue CF
```

Use bounded channels so one stage cannot blow up memory.

## URL Admission Rules

Admission should happen before the URL enters the ready scheduler.

Order:

1. canonicalize
2. host allowlist / scope check
3. extension and path policy
4. query-param policy
5. in-memory recent-seen check
6. durable seen check
7. host-cap check
8. enqueue

This means robots is checked in two places:

- lazily at scheduling time if rules are cached or fetched
- again after redirects on final URL

## Redirect Policy

Follow redirects, but validate the final URL as if it were a newly discovered URL.

Rules:

- final host must be allowed
- final URL must pass path/query filters
- final URL must pass robots for the final host
- canonical URL may rewrite storage key, but only after validation

## Robots Strategy

Robots should be host metadata, not a fetch-time surprise.

Policy:

- fetch once per host on first scheduling need
- cache in memory
- persist in RocksDB
- refresh rarely, e.g. 24h+
- do not run global frontier-wide robots purge jobs

The scheduler should simply refuse to schedule blocked URLs for hosts with cached rules.

## Page Acceptance Policy

Do not store every successful HTML page.

Before writing `pages`, require something like:

- minimum text bytes, e.g. `>= 400`
- minimum paragraph/block count, e.g. `>= 2`
- low enough link density
- not `noindex`
- not known low-signal path classes

This is the cleanest way to kill `chunks=0` style waste before chunking exists.

## Move Chunking Out of Crawl

This is the largest architectural win.

Current hot-path cost includes:

- normalize HTML
- build block tree
- sentence split
- sliding windows
- context augmentation
- statement chaining
- chunk writes

V2 should instead do:

- crawl stores accepted page artifact
- writer appends page URL to `normalize_queue`
- separate `normalize_pages` binary reads that queue and produces chunks

This gives:

- higher crawl throughput
- cleaner failure isolation
- easier re-chunking without recrawl
- better observability of acceptance vs chunk production

## Failure Handling

Use typed reject reasons.

```rust
enum RejectReason {
    BadUrl,
    ScopeFiltered,
    RobotsBlocked,
    RedirectFiltered,
    RedirectRobotsBlocked,
    DnsFailed,
    Timeout,
    Http4xx,
    Http5xx,
    NotHtml,
    NoIndex,
    LowText,
    Duplicate,
}
```

Persist them in `rejected` with counts so you can tune crawl policy without replaying logs.

## Recommended Module Layout

```text
src/crawler/
  mod.rs
  types.rs          # UrlTask, FetchResult, ParsedPage, RejectReason
  scheduler.rs      # per-host in-memory scheduler
  frontier_log.rs   # durable append/recovery
  fetch.rs          # reqwest fetch workers
  parse.rs          # HTML parse + link extraction + quality gates
  persist.rs        # batched RocksDB writer
  host_state.rs     # robots + rate limit + caps
  policy.rs         # host/path/query admission rules
  recover.rs        # restart from RocksDB state
```

And binaries:

```text
src/bin/crawl_v2.rs         # runtime orchestrator
src/bin/normalize_pages.rs  # page -> chunks
```

## Restart / Recovery Model

On startup:

1. load host metadata into memory
2. rebuild per-host queues from recent `frontier_log` entries not marked seen
3. reload robots cache lazily from `robots` CF
4. continue scheduling

Do not rebuild the live frontier by scanning every page/chunk key.

## Migration Plan

### Phase 1

- introduce `PageRecordV2` and `normalize_queue`
- stop chunking in crawl hot path
- keep current URL policies

### Phase 2

- add in-memory per-host scheduler
- stop using global RocksDB frontier scans as the primary scheduler

### Phase 3

- move to batched persistence writer
- add typed reject records and host metadata

### Phase 4

- add recrawl strategy: freshness windows, host budgets, revisit priority

## Performance Expectations

If implemented cleanly, V2 should outperform the current crawler because it removes the most expensive anti-patterns:

- no steady-state global frontier scans
- no background frontier purges
- no fetch-path chunking
- fewer RocksDB point writes per URL
- less scheduler churn on blocked/stale URLs

Expected gains should come from:

- much higher effective fetch utilization
- lower CPU per accepted page in crawl
- lower write amplification
- better host diversity without randomization work

## What Not To Rewrite

Keep these pieces unless benchmarks prove otherwise:

- `canon.rs`
- `dns.rs`
- much of `robots.rs`
- existing extraction logic, but move it to parse/normalize stages
- RocksDB as durable storage

The main rewrite target is the orchestration model, not every helper.

## Recommended First Implementation Slice

If only one slice gets built first, do this:

1. add `crawl_v2.rs`
2. fetch and persist page-level records only
3. move chunking to `normalize_pages.rs`
4. keep current allowlist and policies
5. use per-host in-memory queues

That single step should deliver most of the architectural speedup.
