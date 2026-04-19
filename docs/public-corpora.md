# Public Corpora Fit

This codebase works best when imported data can be mapped into the existing `PageRecord` shape:

- `url`
- `title`
- optional `description`
- `blocks: Vec<TextBlock>` or plain text that can be split into blocks

Once records are in that shape, the normal pipeline already works:

1. import into `content`
2. queue pages in `normalize_queue`
3. run `normalize_pages`
4. run `embed`
5. run `index --full` or incremental `index`
6. run `lexical_index --full` or incremental `lexical_index`

## Best Fits

### 1. Stack Exchange Dumps

Best immediate fit for this architecture.

Why:

- stable post URLs
- explicit titles
- body text is already document-shaped
- naturally useful for technical search
- can be imported one post at a time as `PageRecord`

Recommended use:

- start with Stack Overflow, Unix & Linux, Server Fault, Database Administrators, Information Security, and selected CS-adjacent sites
- convert dump data into JSONL records and import with `import_pages_jsonl`

## 2. Wikipedia Dumps

Very strong quality component, especially for definitions and background topics.

Why:

- clean article structure
- strong coverage for fundamentals
- pairs well with technical docs and Q&A

Tradeoff:

- raw dumps need preprocessing into article text or block structure before import

## 3. Common Crawl

Best raw web source, but not the best first import target for this codebase.

Why:

- huge scale
- real web URLs and crawl timestamps
- closest thing to a general pre-crawled internet snapshot

Tradeoff:

- still needs heavy filtering, extraction, deduplication, and source selection
- easiest to misuse if you want quality-first search rather than broad web recall

## 4. FineWeb / Dolma-Style Derivatives

Good for fast text bootstrapping, less ideal for this search engine than for LLM training.

Why:

- already cleaned and deduplicated
- easier than raw Common Crawl

Tradeoff:

- text-first, not search-engine-first
- metadata and structure are usually thinner than you want for a URL-grounded search corpus

## Recommended Order

If the goal is a useful technical search engine quickly:

1. import selected Stack Exchange dumps
2. import a cleaned Wikipedia slice
3. keep your focused crawl for canonical docs and blogs
4. only then consider Common Crawl or FineWeb slices

## Import Shape

`import_pages_jsonl` accepts one JSON object per line.

Minimum shape:

```json
{"url":"https://example.com/post/1","title":"Example","text":"Paragraph one.\n\nParagraph two."}
```

Structured shape:

```json
{
  "url": "https://example.com/post/1",
  "title": "Example",
  "description": "Optional summary",
  "blocks": [
    {"heading_chain": ["Section"], "text": "Paragraph one."},
    {"heading_chain": ["Section", "Subsection"], "text": "Paragraph two."}
  ]
}
```

## Commands

Import records into the currently selected DB:

```bash
cargo run --release --bin import_pages_jsonl -- path/to/corpus.jsonl
```

Import into the isolated high-quality corpus:

```bash
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml \
cargo run --release --bin import_pages_jsonl -- path/to/corpus.jsonl
```

Then run the existing sequential pipeline:

```bash
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/normalize_pages
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/embed --full-scan
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/index --full
SEARCH_ENGINE_CONFIG_PATH=./config.high_quality.toml ./target/release/lexical_index --full
```
