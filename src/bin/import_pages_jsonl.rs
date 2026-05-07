use std::io::{BufRead, BufReader};

use rocksdb::WriteBatch;
use search_engine::{PageRecord, TextBlock, config, storage};
use serde_json::Value;
use url::Url;

const FLUSH_EVERY: usize = 1_000;

fn text_to_blocks(text: &str) -> Vec<TextBlock> {
    text.split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .map(|paragraph| TextBlock {
            heading_chain: Vec::new(),
            text: paragraph.to_string(),
        })
        .collect()
}

fn optional_string(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_record(line: &str) -> Option<PageRecord> {
    let value: Value = serde_json::from_str(line).ok()?;
    let url = optional_string(&value, "url")?;

    let title = optional_string(&value, "title").unwrap_or_else(|| url.clone());
    let description = optional_string(&value, "description");

    let blocks = if let Some(blocks_value) = value.get("blocks") {
        serde_json::from_value::<Vec<TextBlock>>(blocks_value.clone()).ok()?
    } else {
        let text = optional_string(&value, "text")
            .or_else(|| optional_string(&value, "body"))
            .or_else(|| optional_string(&value, "content"))
            .or_else(|| optional_string(&value, "summary"))
            .or_else(|| optional_string(&value, "extract"))?;
        text_to_blocks(&text)
    };

    if blocks.is_empty() {
        return None;
    }

    Some(PageRecord {
        url,
        title,
        description,
        blocks,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input_path = std::env::args()
        .nth(1)
        .ok_or("usage: import_pages_jsonl <input.jsonl>")?;

    let cfg = config::load()?;
    let db = storage::open_db_with_cache(&cfg.paths.db_path, cfg.rocksdb.block_cache_mb)?;
    let content_cf = storage::cf(&db, storage::CF_CONTENT)?;
    let normalize_queue_cf = storage::cf(&db, storage::CF_NORMALIZE_QUEUE)?;
    let seen_cf = storage::cf(&db, storage::CF_SEEN)?;
    let domains_cf = storage::cf(&db, storage::CF_DOMAINS)?;

    let file = std::fs::File::open(&input_path)?;
    let reader = BufReader::new(file);

    let mut batch = WriteBatch::default();
    let mut inserted = 0usize;
    let mut skipped = 0usize;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let Some(page) = parse_record(&line) else {
            skipped += 1;
            continue;
        };

        let encoded = serde_json::to_vec(&page)?;
        batch.put_cf(content_cf, page.url.as_bytes(), encoded);
        batch.put_cf(normalize_queue_cf, page.url.as_bytes(), []);
        batch.put_cf(seen_cf, page.url.as_bytes(), []);

        if let Ok(parsed) = Url::parse(&page.url) {
            if let Some(host) = parsed.host_str() {
                batch.put_cf(domains_cf, host.as_bytes(), b"imported");
            }
        }

        inserted += 1;

        if inserted % FLUSH_EVERY == 0 {
            db.write(batch)?;
            batch = WriteBatch::default();
            println!(
                "[import_pages_jsonl] inserted={} skipped={}",
                inserted, skipped
            );
        }
    }

    if inserted % FLUSH_EVERY != 0 {
        db.write(batch)?;
    }

    println!(
        "[import_pages_jsonl] done. inserted={} skipped={} source={}",
        inserted, skipped, input_path
    );
    Ok(())
}
