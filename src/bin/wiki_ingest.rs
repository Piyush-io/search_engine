use search_engine::{config, knowledge::wikipedia::WikiRecord, storage};

fn parse_record(line: &str) -> Option<WikiRecord> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;

    let title = v.get("title")?.as_str()?.trim().to_string();
    if title.is_empty() {
        return None;
    }

    let summary = v
        .get("summary")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("extract").and_then(|x| x.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();

    if summary.is_empty() {
        return None;
    }

    let image_url = v
        .get("image_url")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());

    let description = v
        .get("description")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string());

    Some(WikiRecord {
        title,
        summary,
        image_url,
        description,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = storage::open_db(&cfg.paths.db_path)?;
    let wiki_cf = storage::cf(&db, storage::CF_WIKI)?;

    let input_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "training/wiki_summaries.jsonl".to_string());

    let file = std::fs::File::open(&input_path)?;
    let reader = std::io::BufReader::new(file);

    let mut inserted = 0usize;
    for line in std::io::BufRead::lines(reader) {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let Some(rec) = parse_record(&line) else {
            continue;
        };

        let key = rec.title.to_ascii_lowercase();
        db.put_cf(wiki_cf, key.as_bytes(), serde_json::to_vec(&rec)?)?;
        inserted += 1;

        if inserted % 10_000 == 0 {
            println!("[wiki_ingest] inserted={}", inserted);
        }
    }

    println!("[wiki_ingest] done. inserted={} source={}", inserted, input_path);
    Ok(())
}
