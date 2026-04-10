# Neural Search Engine for Computer Science

## Wilson-Aligned Architecture | 8-Week Major Project Plan

**Reference:** [Building a web search engine from scratch in two months with 3 billion neural embeddings](https://blog.wilsonl.in/search-engine/) by Wilson Lin

**Strategy:** Replicate Wilson's full architecture at local scale (50K pages vs 280M). Same pipeline, same embedding model, same HNSW library. All parameters configurable so the architecture scales with hardware.

**Primary demo:** Your own search engine running locally on CS content.
**Report reference:** Wilson's blog (cited) + his live demo shown as "what this architecture achieves at production scale."

---

## Hardware & Development Setup

| Machine | Specs                               | Role                                              |
| ------- | ----------------------------------- | ------------------------------------------------- |
| M2 Mac  | 16GB RAM, Apple Silicon             | Primary development, CoreML inference             |
| PC      | 16GB RAM, GTX 1660 Super (6GB VRAM) | GPU training (DistilBERT), batch embedding (CUDA) |

---

## Configurable Parameters (config.toml)

```
Parameter               | M2 Mac (dev)   | PC (1660S)      | Wilson's Infra
------------------------|----------------|-----------------|----------------
max_pages               | 10,000         | 50,000          | 280,000,000
crawl_concurrency       | 10             | 20              | hundreds of nodes
rate_limit_ms           | 1000           | 1000            | per-origin adaptive
embedding_model         | multi-qa-mpnet-base-dot-v1 (same across all)
embedding_dim           | 768            | 768             | 768
embedding_batch_size    | 8              | 32              | larger
embedding_device        | coreml         | cuda            | 200 GPUs
hnsw_shards             | 1              | 1               | 64
hnsw_m                  | 16             | 16              | tunable
hnsw_ef_construction    | 200            | 200             | tunable
hnsw_ef_search          | 50             | 100             | tunable
hnsw_max_elements       | 100,000        | 500,000         | billions
chunk_context_depth     | 3              | 3               | full tree
wiki_articles_limit     | 100,000        | 500,000         | 6,000,000+
rocksdb_block_cache_mb  | 256            | 512             | multi-TB
```

---

## Memory Budget (16GB PC — worst case)

```
Component                          | Estimate
-----------------------------------|----------
HNSW index (500K x 768-dim f32)   | ~1.55 GB
RocksDB (50K pages + chunks)      | ~2 GB
Embedding model (ONNX, GPU VRAM)  | ~440 MB
DistilBERT classifier (ONNX, GPU) | ~260 MB
Wikipedia HNSW (100K articles)     | ~310 MB
OS + Rust runtime                  | ~2 GB
Headroom                           | ~9.5 GB free
```

---

## Crate Decisions (Research-Backed)

| Component          | Crate                      | Version          | Why                                                                                            |
| ------------------ | -------------------------- | ---------------- | ---------------------------------------------------------------------------------------------- |
| Sentence splitting | `srx`                      | 0.1.4            | Pure Rust, LanguageTool rules, ~91-93 F1 vs spaCy's 92.9. Zero ML overhead                     |
| ONNX inference     | `ort`                      | =2.0.0-rc.11     | ONNX Runtime in Rust. CoreML on M2, CUDA on 1660S. Production-ready RC                         |
| Tokenization       | `tokenizers`               | 0.22             | HuggingFace official. Exact same tokenization as Python. `http` feature for downloads          |
| Embedding model    | multi-qa-mpnet-base-dot-v1 | ONNX export      | 768-dim, CLS pooling. Same model Wilson used. Pre-exported ONNX on HuggingFace                 |
| Statement chaining | distilbert-base-uncased    | Fine-tune + ONNX | Wilson's exact approach. Train on PC, export ONNX, infer in Rust via `ort`                     |
| HNSW               | `hnswlib-rs`               | 0.10.0           | Wilson Lin's own pure-Rust port of the C++ hnswlib. Typed keys, upsert/delete, filtered search |
| HNSW fallback      | `hnsw_rs`                  | 0.3.3            | 277K downloads, 5+ years, parallel insert/search, mmap. Use if hnswlib-rs has issues           |
| Web server         | `axum`                     | 0.8              | SSR, no JS. Wilson's philosophy                                                                |
| Templates          | `askama`                   | 0.13             | Compile-time SSR templates                                                                     |
| Click tracking     | `aes-gcm`                  | 0.10             | AES-256-GCM for encrypted redirect URLs. Wilson's exact approach                               |
| URL validation     | `publicsuffix`             | 2                | eTLD validation for URL canonicalization                                                       |
| Serialization      | `bincode`                  | 1                | Serialize [f32; 768] embeddings efficiently                                                    |

### Full Cargo.toml Dependencies

```toml
[dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }
futures = "0.3"

# HTTP
reqwest = { version = "0.13", features = ["json"] }
axum = "0.8"

# HTML parsing
kuchiki = "0.8"
scraper = "0.22"

# Persistence
rocksdb = "0.24"
bincode = "1"

# URL processing
url = "2.5"
publicsuffix = "2"

# Sentence splitting
srx = { version = "0.1.4", features = ["from_xml"] }

# ML inference (use "coreml" on Mac, "cuda" on PC)
ort = { version = "=2.0.0-rc.11", features = ["coreml"] }
tokenizers = { version = "0.22", features = ["http"] }
ndarray = "0.17"

# Vector search
hnswlib-rs = "0.10"

# Web UI
askama = "0.13"

# Crypto (click tracking)
aes-gcm = "0.10"

# Config
toml = "0.8"
serde = { version = "1", features = ["derive"] }

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"
```

---

## Project Structure

```
search_engine/
├── Cargo.toml
├── config.toml                   # All configurable parameters
├── models/                       # ONNX model files + tokenizer.json
│   ├── multi-qa-mpnet/
│   │   ├── model.onnx
│   │   └── tokenizer.json
│   └── statement-chain/
│       ├── model.onnx
│       └── tokenizer.json
├── src/
│   ├── main.rs                   # Web server entry point (axum)
│   ├── lib.rs                    # Re-exports, shared types
│   ├── config.rs                 # Load config.toml parameters
│   ├── crawler/
│   │   ├── mod.rs
│   │   ├── frontier.rs           # Refactored from current frontier.rs
│   │   ├── canon.rs              # URL canonicalization
│   │   ├── dns.rs                # DNS resolution + private IP check
│   │   └── robots.rs             # Extracted from task8
│   ├── extraction/
│   │   ├── mod.rs
│   │   ├── normalizer.rs         # HTML -> semantic document tree
│   │   ├── density.rs            # Text density heuristic
│   │   └── metadata.rs           # OpenGraph, schema.org extraction
│   ├── chunking/
│   │   ├── mod.rs
│   │   ├── sentencizer.rs        # Sentence splitting via srx
│   │   ├── context.rs            # Heading chain attachment
│   │   └── chaining.rs           # Statement chaining (DistilBERT)
│   ├── embeddings/
│   │   ├── mod.rs
│   │   └── client.rs             # ONNX embedding via ort + tokenizers
│   ├── search/
│   │   ├── mod.rs
│   │   ├── hnsw.rs               # HNSW index wrapper (hnswlib-rs)
│   │   └── query.rs              # Query pipeline: embed -> search -> fetch
│   ├── knowledge/
│   │   ├── mod.rs
│   │   ├── wikipedia.rs          # Wikipedia dump parser
│   │   └── panel.rs              # Knowledge panel builder
│   └── web/
│       ├── mod.rs
│       ├── serp.rs               # Search results page handler
│       ├── tracking.rs           # Click tracking (AES-encrypted /act)
│       └── templates/            # askama HTML templates
│           ├── base.html
│           ├── search.html
│           └── results.html
├── src/bin/
│   ├── crawl.rs                  # Production crawler binary
│   ├── embed.rs                  # Batch embed all chunks
│   ├── index.rs                  # Build HNSW from embeddings
│   ├── label.rs                  # CLI labeling tool for statement chaining
│   ├── wiki_ingest.rs            # Wikipedia dump ingestion
│   ├── task1_sequential.rs       # (learning artifact — keep)
│   ├── task2_3_4_concurrent.rs   # (learning artifact — keep)
│   ├── task5_frontier.rs         # (learning artifact — keep)
│   ├── task6.rs                  # (learning artifact — keep)
│   ├── task7.rs                  # (learning artifact — keep)
│   ├── task8.rs                  # (learning artifact — keep)
│   ├── task9.rs                  # (learning artifact — keep)
│   ├── task10.rs                 # (learning artifact — keep)
│   └── frontier.rs               # (learning artifact — keep)
└── training/                     # Python scripts for model training only
    ├── train_chaining.py         # Fine-tune DistilBERT classifier
    ├── export_onnx.py            # Export trained model to ONNX
    └── requirements.txt          # torch, transformers, optimum
```

---

## CS-Focused Crawl Seeds

Target: 50,000 pages of Computer Science content.

```
# Documentation (high-quality structured text)
doc.rust-lang.org
docs.python.org
developer.mozilla.org/en-US/docs
cppreference.com
go.dev/doc
docs.oracle.com/javase

# Educational (lectures, tutorials, explanations)
cs.stanford.edu
ocw.mit.edu (CS courses)
geeksforgeeks.org
tutorialspoint.com/data_structures_algorithms

# Wikipedia CS (dense, well-structured)
en.wikipedia.org/wiki/Computer_science
en.wikipedia.org/wiki/Algorithm
en.wikipedia.org/wiki/Data_structure
en.wikipedia.org/wiki/Operating_system
en.wikipedia.org/wiki/Computer_network
en.wikipedia.org/wiki/Database
en.wikipedia.org/wiki/Compiler

# Q&A (real questions + expert answers)
stackoverflow.com (tagged: algorithms, data-structures,
  operating-systems, networking, databases, compilers)

# Blogs (deep technical content)
blog.acolyer.org
jvns.ca
martinfowler.com
research.google/blog
blog.rust-lang.org

# Research (abstracts only, not PDFs)
arxiv.org/list/cs
```

A query like "how does garbage collection handle circular references" should return precise statement-level answers from these sources.

---

## Pipeline Overview

```
Crawl (50K CS pages)
  -> Canonicalize URLs (eTLD, normalize, deduplicate)
  -> DNS resolve (reject private IPs)
  -> Respect robots.txt + rate limits
  -> Fetch HTML

Normalize (semantic document tree)
  -> Strip script/style/nav/header/footer
  -> Text density filtering (>50% ratio)
  -> Parse heading hierarchy (h1 -> h2 -> h3)
  -> Extract OpenGraph metadata
  -> Handle tables (header -> cell), definition lists (dt -> dd)

Chunk (sentence-level with context)
  -> Split into sentences (srx crate, LanguageTool rules)
  -> Attach heading chain as context prefix
  -> Statement chaining (trained DistilBERT classifier via ONNX)
  -> Non-leaf sentences flagged as "never match"

Embed (multi-qa-mpnet-base-dot-v1 via ONNX)
  -> Tokenize with HuggingFace tokenizers crate
  -> Batch inference via ort (CoreML on Mac, CUDA on PC)
  -> CLS pooling on transformer output
  -> Store [f32; 768] in RocksDB embeddings CF

Index (HNSW via hnswlib-rs)
  -> Load all embeddings -> insert into HNSW
  -> Configurable M, ef_construction, ef_search
  -> Serialize index to disk

Serve (axum + askama, SSR, JS-free)
  -> Query text -> embed -> HNSW search -> top-K chunks
  -> Retrieve chunk text + source URL from RocksDB
  -> Knowledge panel from Wikipedia HNSW
  -> Click tracking via AES-encrypted /act redirect

```

---

## Completed Work (Phase 0 & 1)

### Phase 0: Async Rust Mental Models (DONE)

- [x] **Task 1:** Sequential web fetcher (`src/bin/task1_sequential.rs`)
  - Fetches 20 URLs one-by-one with ureq, prints response time
- [x] **Task 2:** Concurrent web fetcher (`src/bin/task2_3_4_concurrent.rs`)
  - tokio::spawn + reqwest, all URLs fetched concurrently
- [x] **Task 3:** Backpressure control (same file)
  - Semaphore(50) limits concurrent requests
- [x] **Task 4:** Graceful failure handling (same file)
  - Timeout(5s), error_for_status(), match on JoinHandle result

### Phase 1: Persistent State & URL Frontier (DONE)

- [x] **Task 5:** In-memory frontier (`src/bin/task5_frontier.rs`)
  - HashSet<String> for seen, VecDeque<String> for queue, BFS web crawl
- [x] **Task 6:** Persistent frontier with RocksDB (`src/bin/task6.rs`)
  - Column families: "seen", "to_crawl". Snapshot-based iteration
- [x] **Task 7:** Per-domain rate limiting (`src/bin/task7.rs`)
  - "domains" CF stores last-visit timestamps, 1s delay enforced
- [x] **Task 8:** robots.txt checking (`src/bin/task8.rs`)
  - Fetches and parses robots.txt, respects Disallow rules
  - Frontier struct extracted to `src/bin/frontier.rs` (4 CFs: seen, to_crawl, domains, robots)
- [x] **Task 9:** Parse raw HTML (`src/bin/task9.rs`)
  - Fetches 50 diverse pages, parses with scraper crate
- [x] **Task 10:** Remove script/style/nav tags (`src/bin/task10.rs`)
  - kuchiki DOM manipulation, strips boilerplate, serializes clean HTML

---

## Week 1: Refactor + URL Canonicalization + Content Extraction

_Aligns with Wilson's: Crawler, URL processing, Normalization_

### 1a. Refactor project structure

- Reorganize from scattered `task*.rs` bins into the module structure above
- Create `config.rs` + `config.toml` with all configurable parameters
- Create `src/lib.rs` with shared types (ChunkId, PageRecord, EmbeddingVec, etc.)
- Keep all `src/bin/task*.rs` files as learning artifacts
- Verify `cargo build` still compiles everything

### 1b. URL canonicalization (`src/crawler/canon.rs`)

Wilson explicitly calls this critical. All URLs must be strictly processed before entering the system:

- Enforce `https:` scheme only (reject ftp:, data:, javascript:, etc.)
- Validate eTLD using `publicsuffix` crate (reject invalid hostnames)
- Percent-decode all components, then re-encode with minimal consistent charset
- Sort query parameters alphabetically (or drop them entirely, configurable)
- Lowercase the origin (scheme + host)
- Strip ports, usernames, passwords
- Reject URLs containing C0/C1 control characters
- Enforce max URL length (2048 bytes)
- Deduplicate trailing slashes

_Wilson's lesson: "URLs seem straightforward, but can actually be subtle to deal with."_

### 1c. DNS security (`src/crawler/dns.rs`)

Wilson's "surprising failure point":

- Resolve DNS manually before making HTTP requests
- Check resolved IP against private ranges:
  - 10.0.0.0/8
  - 172.16.0.0/12
  - 192.168.0.0/16
  - 127.0.0.0/8
  - ::1, fe80::/10
- Reject requests to private IPs (prevents SSRF / internal data leaks)
- Handle EAI_AGAIN and SERVFAIL DNS failures gracefully (retry with backoff)

### 1d. Content extraction — semantic document tree (`src/extraction/`)

Wilson's normalization pipeline preserves document structure, not just text:

**normalizer.rs:**

- Parse HTML into DOM (kuchiki)
- Build a semantic tree: heading hierarchy (h1 -> h2 -> h3 nesting)
- Each content block knows its parent heading chain
- Handle tables: associate column headers with cell values
- Handle definition lists: associate `<dt>` terms with `<dd>` definitions
- "Leading" sentences before `<ul>/<ol>` lists associate with list items

**density.rs:**

- Calculate text-to-HTML ratio for each DOM subtree
- Keep nodes with >50% text density
- Discard nodes where >20% of text content is inside `<a>` tags (link-heavy = navigation)

**metadata.rs:**

- Extract `<title>` tag
- Extract OpenGraph meta tags (og:title, og:description, og:image)
- Extract `<meta name="description">`
- Extract schema.org structured data if present (JSON-LD)

**Store in RocksDB:**

- New column family: `content`
- Key: canonicalized URL
- Value: serialized struct containing title, heading hierarchy, clean text blocks, metadata

**Checkpoint:** Crawl 100 CS pages. Verify clean text extraction with heading context preserved.

---

## Week 2: Chunking Pipeline + Statement Chaining

_Aligns with Wilson's: Chunking, Semantic context, Statement chaining_

### 2a. Sentence-level chunking (`src/chunking/sentencizer.rs`)

Wilson splits into sentences, not fixed-size chunks — sentences are the natural atomic unit:

- Use `srx` crate with bundled LanguageTool rules
- Split extracted text blocks into individual sentences
- Each sentence gets a unique `chunk_id` (hash of URL + position)
- Tag each sentence with: source URL, position index, parent heading chain

_Wilson's reasoning: "Breaking into sentences would be a good atomic unit of detail: enough to pinpoint the exact relevant part or answer to a query."_

### 2b. Semantic context attachment (`src/chunking/context.rs`)

Wilson's key innovation — each chunk carries its structural context:

- Prepend the heading chain to each sentence before embedding
- Example output for a sentence deep in a doc:
  ```
  ["PostgreSQL Performance Tuning Guide",
   "Connection Settings",
   "Maximum connections",
   "Each connection uses a new process."]
  .join("\n")
  ```
- Table cells carry their column header text
- `<dd>` definitions carry their `<dt>` term
- List items carry their preceding "leading" sentence (e.g., "Here are the suggested values:")
- Configurable `chunk_context_depth` — how many heading levels to include

### 2c. Statement chaining — labeling tool (`src/bin/label.rs`)

Wilson trained a DistilBERT classifier to detect when a sentence depends on a prior sentence for meaning. This requires labeled training data:

- Build a CLI tool that:
  1. Loads sentences from RocksDB chunks CF
  2. Shows each sentence + the 3 preceding sentences
  3. Asks: "Which preceding sentence (if any) is required for this one to make sense? (0=none, 1/2/3=that sentence)"
  4. Saves label to a JSONL file
- Target: 500-1000 labeled examples
- Focus on CS content where pronouns and references are common:
  - "It uses a mark-and-sweep algorithm" (depends on prior sentence naming the GC)
  - "This is different to most other systems" (depends on context)
  - "Therefore, the setting may have surprising impact" (multi-hop chain)
- Estimated time: 2-3 days of labeling sessions

### 2d. Statement chaining — model training (`training/`)

Train on PC (1660 Super, 6GB VRAM):

**train_chaining.py:**

- Load labeled JSONL data
- Fine-tune `distilbert-base-uncased` as binary classifier
- Input format: `[CLS] current_sentence [SEP] candidate_antecedent [SEP]`
- Output: binary (depends / doesn't depend)
- Training: ~few hours on 1660 Super
- Save PyTorch model

**export_onnx.py:**

- Export fine-tuned model to ONNX via `optimum-cli`:
  ```
  optimum-cli export onnx \
    --model ./trained_model \
    --task text-classification \
    statement_chain_onnx/
  ```
- Copy `model.onnx` + `tokenizer.json` to `models/statement-chain/`

### 2e. Statement chaining — Rust inference (`src/chunking/chaining.rs`)

- Load DistilBERT ONNX via `ort` (CoreML on Mac, CUDA on PC)
- Load tokenizer via `tokenizers` crate
- At chunking time: for each sentence, check dependency on preceding sentences
- Follow the chain backwards to build full context string
- Non-leaf sentences (those that are only dependencies, never standalone matches) get flagged — they won't be returned as search results

_Wilson: "This also had the benefit of labelling sentences that should never be matched, because they were not 'leaf' sentences by themselves."_

**Checkpoint:** Chunk 1000 pages into context-rich sentences. Spot-check 20 sentences: does the heading chain + statement chain produce meaningful, self-contained text?

---

## Week 3: Embedding Pipeline

_Aligns with Wilson's: SBERT embeddings, GPU buildout_

### 3a. ONNX model setup

- Download pre-exported `multi-qa-mpnet-base-dot-v1` ONNX from HuggingFace
  - The model page already lists ONNX as a supported format
  - Alternatively: `optimum-cli export onnx --model sentence-transformers/multi-qa-mpnet-base-dot-v1 --task feature-extraction multi_qa_mpnet_onnx/`
- Download `tokenizer.json` from the same model repo
- Place in `models/multi-qa-mpnet/`
- Verify: load in `ort`, run a test embedding, check output shape is [1, 768]

**Important ONNX caveat:** The ONNX export only contains the transformer backbone. Pooling (CLS for this model) must be implemented in Rust manually — take the first token's output vector. This is a few lines of ndarray code.

### 3b. Rust embedding client (`src/embeddings/client.rs`)

- Load tokenizer: `Tokenizer::from_file("models/multi-qa-mpnet/tokenizer.json")`
- Load ONNX session with appropriate EP:
  ```
  Mac:  ep::CoreML with ComputeUnits::CPUAndNeuralEngine
  PC:   ep::CUDA with device_id(0)
  ```
- Encode text: `tokenizer.encode(text)` -> `input_ids`, `attention_mask`
- Run session: feed input_ids + attention_mask as ort tensors
- CLS pooling: extract first token output -> [f32; 768]
- Batch mode: pad multiple inputs to max sequence length, run as batch

Expected throughput:

- M2 Mac (CoreML): ~30-50 chunks/sec
- PC (1660S CUDA): ~50-100 chunks/sec

### 3c. Batch embedding binary (`src/bin/embed.rs`)

- Read all chunks from RocksDB `chunks` CF
- Skip chunks already in `embeddings` CF (resumable)
- Batch into groups of `embedding_batch_size`
- Embed each batch via the client
- Store in RocksDB `embeddings` CF:
  - Key: chunk_id (same as in chunks CF)
  - Value: `[f32; 768]` serialized via `bincode` (3072 bytes per embedding)
- Print progress: "Embedded 1000/500000 chunks (2 chunks/sec, ETA: 4h 10m)"
- Total storage: 500K chunks x 3072 bytes = ~1.5 GB

### 3d. Query embedding function

- Same model, single-input mode
- Target: <50ms per query embedding
- Used at query time in the web server

**Checkpoint:** Embed 10,000 chunks. Verify round-trip: load embedding from RocksDB, compute cosine similarity between two related chunks — should be >0.7.

---

## Week 4: HNSW Vector Search

_Aligns with Wilson's: Sharded HNSW (single shard at your scale)_

### 4a. HNSW index construction (`src/bin/index.rs`)

Using Wilson's own `hnswlib-rs` crate:

- Load all embeddings from RocksDB `embeddings` CF
- Create HNSW index:
  ```
  HnswConfig::new(768, max_elements)
    .m(hnsw_m)                     // from config.toml
    .ef_construction(hnsw_ef_c)    // from config.toml
    .ef_search(hnsw_ef_search)     // from config.toml
  ```
- Insert all vectors with chunk_id as key
- Serialize to disk: `hnsw.save_to(&mut file)`
- Print: build time, memory usage, index file size

`hnswlib-rs` advantages over alternatives:

- Typed keys (String chunk_ids, not just integers)
- Upsert/delete support via `set()` / `delete()`
- Filtering during search: `search(&vectors, &query, k, Some(&filter_fn))`
- Can read legacy C++ hnswlib format indices
- Supports Qi8 quantized vectors for memory savings

**Fallback plan:** If `hnswlib-rs` (0.10.0, 3K downloads) has stability issues, switch to `hnsw_rs` (0.3.3, 277K downloads) — same concept, more mature, slightly different API.

### 4b. Search query pipeline (`src/search/query.rs`)

- On server startup: load serialized HNSW index from disk
- Query flow:
  1. User enters query text
  2. Embed query with same model -> [f32; 768]
  3. HNSW search -> top-K chunk_ids + distances
  4. For each chunk_id: fetch chunk text + source URL + heading context from RocksDB
  5. Return ranked results
- `ef_search` is runtime-adjustable: `hnsw.set_ef_search(n)`

### 4c. Benchmarks (for report)

Run these and save results for the report:

1. **Brute-force vs HNSW latency** — same 500K vectors, same queries
2. **Recall@10 at varying ef_search** — 10, 50, 100, 200 (use brute-force as ground truth)
3. **Insert throughput** — vectors/sec during index build
4. **Memory usage** — measure RSS during search
5. **Index file size** — serialized HNSW on disk

Target: <20ms search latency, >90% recall@10 with ef_search=100

### 4d. Sharding architecture (design only, document in report)

Even with 1 shard, structure the code so the query aggregator pattern is visible:

- Query hits aggregator -> fans out to shard(s) -> merges top-K from each
- In report: explain Wilson's 64-shard approach, how hash(chunk_id) % N distributes data
- Explain what would change at scale: more shards, parallel queries, partial failure handling

**Checkpoint:** Run 100 semantic queries against 500K vectors. Do results make sense? Is latency <20ms?

---

## Week 5: SERP + Web UI

_Aligns with Wilson's: SERP design, State tracking, JS-free SSR_

### 5a. Web server (`src/main.rs` + `src/web/`)

Using `axum`:

- `GET /` — search home page (query box)
- `GET /search?q=...` — results page (SERP)
- `GET /act?d=...` — click tracking redirect
- Bind to `0.0.0.0:3000` (configurable)
- On startup: load HNSW index + embedding model + RocksDB handles

### 5b. SERP design (`src/web/serp.rs` + `templates/`)

Wilson's design philosophy: "signal over noise" — minimal, instant, no loading indicators.

- Clean, minimal layout (no flashy UI)
- Query box persists at top of results page
- Each result card:
  - Page title (from metadata)
  - URL (displayed, not linked directly — goes through /act)
  - Matched statement snippet with heading context shown
  - "Fact" pages (docs, wikis): show specific statement with full context chain
- Knowledge panel on the right sidebar (Week 6)
- No JavaScript whatsoever — pure SSR via askama templates
- SSR HTML templates with minimal CSS

### 5c. Click tracking (`src/web/tracking.rs`)

Wilson's approach — AES-encrypted redirect URLs:

- When rendering results, each URL goes through `/act?d=<encrypted_data>`
- Encrypted payload contains: query text, result position, target URL, timestamp
- `aes-gcm` crate for AES-256-GCM encryption
- On click: decrypt, log (query, position, url, timestamp) to RocksDB `clicks` CF, redirect to target URL
- PRG (Post/Redirect/Get) pattern with one-off cookies for UX state
- This data is useful for search quality analysis in the report

**Checkpoint:** Open `localhost:3000`, type a CS query, see ranked results with statement snippets, click through — all working end-to-end.

---

## Week 6: Knowledge Graph

_Aligns with Wilson's: Wikipedia + Wikidata knowledge panels_

### 6a. Wikipedia data ingestion (`src/bin/wiki_ingest.rs`)

Wilson used Wikipedia + Wikidata full dumps to build knowledge panels:

- Download Wikipedia article summaries dump from dumps.wikimedia.org
  - REST API format: title, summary, image URL, Wikidata QID
  - ~2GB compressed for English Wikipedia
- Parse and store in RocksDB `wiki` column family:
  - Key: article title (normalized)
  - Value: serialized struct (title, summary, image_url, wikidata_qid, description)
- Limit to `wiki_articles_limit` from config (100K-500K)

### 6b. Knowledge panel retrieval (`src/knowledge/`)

- Combine title + summary text for each Wikipedia article
- Embed using the same SBERT model
- Build a separate HNSW index (stored as `wiki_hnsw.bin`)
- At query time: also search Wikipedia HNSW -> top-1 match
- If similarity score exceeds threshold (configurable), show as knowledge panel

### 6c. Knowledge panel UI

- Right sidebar on SERP (below results on narrow screens)
- Display:
  - Article image (if available)
  - Title
  - Short description
  - Summary extract (first 2-3 sentences)
  - "Read more on Wikipedia" link
- Wikidata properties (stretch goal):
  - Download subset of Wikidata JSON dump
  - For entities with QID: look up key properties (birth date, country, etc.)
  - Show as "Quick Facts" table

**Checkpoint:** Query "quicksort" — does a knowledge panel appear with the Wikipedia article?

---

## Week 7: Search Quality + Benchmarks

_Aligns with Wilson's: Search quality analysis_

### 7a. Search quality testing

Create 50 test queries across categories:

**Factual lookups:**

- "what is a B-tree"
- "time complexity of merge sort"
- "what does TCP three-way handshake do"

**Natural language questions:**

- "why is Rust memory safe without a garbage collector"
- "how does garbage collection handle circular references"
- "when should I use a hash table vs a binary search tree"

**Complex multi-sentence queries** (Wilson's strength):

- "I want to store graph data where nodes have properties and edges have weights, and I need to find shortest paths frequently but also do pattern matching on subgraphs"
- "my database queries are slow because of joins across 5 tables with millions of rows each, and adding indexes didn't help"

For each query:

1. Run on your engine, record top-5 results
2. Run same query on Wilson's live demo, record top-5
3. Subjective quality score (1-5) for each
4. Document in report

### 7b. Comprehensive benchmarks

Collect and format for report:

| Metric                   | Value         | Method                        |
| ------------------------ | ------------- | ----------------------------- |
| Crawl throughput         | pages/sec     | Time 1000 page crawl          |
| Content extraction       | pages/sec     | Time normalizer on 1000 pages |
| Chunking throughput      | sentences/sec | Time sentencizer on all pages |
| Embedding throughput     | chunks/sec    | From embed.rs progress output |
| HNSW build time          | seconds       | From index.rs                 |
| HNSW index size          | MB on disk    | File size of serialized index |
| HNSW memory usage        | MB RSS        | Measure during search         |
| Query embedding latency  | ms            | Average over 100 queries      |
| HNSW search latency      | ms            | Average over 100 queries      |
| End-to-end query latency | ms            | From HTTP request to response |
| Recall@10                | %             | At ef_search = 50, 100, 200   |
| Total pages crawled      | count         |                               |
| Total chunks             | count         |                               |
| Total embeddings         | count         |                               |
| Storage breakdown        | MB per CF     | RocksDB stats                 |

### 7c. Recall curve

Generate recall@10 vs ef_search plot:

- ef_search values: 10, 20, 50, 100, 200, 500
- Ground truth: brute-force top-10 on 1000 random queries
- Plot: X = ef_search, Y = recall@10
- This is the most important graph for the report

---

## Week 8: Report + Demo Preparation

### 8a. Report structure

1. **Abstract** — Neural search engine for CS content, 50K pages, statement-level semantic search
2. **Introduction** — Problem: keyword search fails for complex queries. Solution: neural embeddings
3. **Related Work** — Cite Wilson Lin's blog. Describe his architecture and scale (280M pages, 3B embeddings, 200 GPUs). This is the production reference
4. **System Architecture** — Your diagram mirroring Wilson's pipeline. Component descriptions
5. **Implementation Details** — Each module: design decisions, crate choices, challenges
   - URL canonicalization (Wilson's lessons)
   - Semantic document tree (heading hierarchy)
   - Sentence chunking + context attachment
   - Statement chaining (trained classifier)
   - ONNX embeddings in Rust
   - HNSW indexing (Wilson's own library)
   - SERP design (JS-free SSR)
   - Knowledge panels (Wikipedia)
6. **Configurable Parameters** — The full table showing your config vs Wilson's config, explaining what changes at scale and why
7. **Results & Evaluation** — Benchmarks, recall curves, sample queries, quality comparison
8. **Comparison to Production Scale** — Reference Wilson's live demo. What your architecture would need to scale: more shards, more GPUs, distributed crawling, CoreNN vector DB
9. **Limitations & Future Work** — What you'd improve, what Wilson has that you simplified (multi-level stochastic queues, full Wikidata, latency optimization)
10. **Conclusion**

### 8b. Demo preparation

- Your local demo: live search on `localhost:3000`
  - Type CS queries, show results with statement snippets
  - Show knowledge panels
  - Show click tracking data
- Side-by-side with Wilson's live demo:
  - Run same queries on both
  - Frame: "Same architecture, same embedding model, same HNSW library — different scale"
  - This is cited and credited, not presented as your work
- Prepare for Q&A:
  - "Why HNSW over brute-force?" — show recall curve + latency comparison
  - "What happens at scale?" — explain sharding, point to Wilson's 64-node setup
  - "Why not just use Elasticsearch?" — explain neural vs keyword, show the complex query examples
  - "How does statement chaining work?" — show labeled examples, explain the classifier

---

## Cut Order (If Behind Schedule)

If you're running behind, drop items in this order:

1. **Wikidata properties** (Week 6c stretch goal) — skip, keep Wikipedia summaries only
2. **Knowledge graph entirely** (Week 6) — cut the whole Wikipedia integration
3. **Click tracking encryption** (Week 5c) — simplify to plain URL params, no AES
4. **Statement chaining model** (Week 2c-e) — fall back to heuristic: if sentence starts with pronoun/reference word ("it", "this", "therefore", "however"), prepend previous sentence
5. **HNSW recall benchmarks** (Week 4c) — skip the brute-force comparison, just report latency

Core that cannot be cut: crawl -> normalize -> chunk -> embed -> HNSW search -> web UI

---

## Key References

- Wilson Lin, "Building a web search engine from scratch in two months with 3 billion neural embeddings" — https://blog.wilsonl.in/search-engine/
- Wilson Lin, CoreNN vector database — https://github.com/wilsonzlin/CoreNN
- Wilson Lin, `hnswlib-rs` (Rust HNSW port) — https://crates.io/crates/hnswlib-rs
- HuggingFace `multi-qa-mpnet-base-dot-v1` — https://huggingface.co/sentence-transformers/multi-qa-mpnet-base-dot-v1
- HuggingFace `distilbert-base-uncased` — https://huggingface.co/distilbert-base-uncased
- `ort` (ONNX Runtime for Rust) — https://crates.io/crates/ort
- `srx` (sentence segmentation) — https://crates.io/crates/srx
- Anthropic, "Contextual Retrieval" — https://www.anthropic.com/news/contextual-retrieval
- Jina AI, "Late Chunking" — https://jina.ai/news/late-chunking-in-long-context-embedding-models/
