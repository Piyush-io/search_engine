use std::time::Instant;

use search_engine::{
    config,
    search::{hnsw::HnswIndex, lexical::LexicalIndex, query},
    storage,
};

const SAMPLE_QUERIES: &[&str] = &[
    "what is a B-tree",
    "time complexity of merge sort",
    "how does tcp three-way handshake work",
    "why is rust memory safe without gc",
    "hash table vs binary search tree",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = storage::open_db(&cfg.paths.db_path)?;
    let index = HnswIndex::load_from_path(&cfg.paths.index_path)?;
    let lexical = LexicalIndex::open(&cfg.paths.lexical_index_path).ok();

    let mut records = Vec::new();

    for q in SAMPLE_QUERIES {
        let t0 = Instant::now();
        let hits = query::run_query(&db, &index, lexical.as_ref(), q, 10);
        let ms = t0.elapsed().as_millis();
        records.push(serde_json::json!({
            "query": q,
            "latency_ms": ms,
            "hits": hits.len(),
            "top_hit": hits.first().map(|h| &h.source_url),
        }));
        println!("[bench] q='{}' latency={}ms hits={}", q, ms, hits.len());
    }

    std::fs::create_dir_all("reports")?;
    std::fs::write("reports/benchmark_results.json", serde_json::to_string_pretty(&records)?)?;
    println!("[bench] wrote reports/benchmark_results.json");

    Ok(())
}
