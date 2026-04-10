use std::{collections::HashSet, path::Path, sync::Arc};

use search_engine::{
    config,
    embeddings::client,
    search::{
        bruteforce::BruteForceIndex, composite::CompositeVectorIndex, hnsw::HnswIndex,
        lexical::LexicalIndex, query, vector_index::VectorIndex,
    },
    storage,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let query_text = args
        .next()
        .unwrap_or_else(|| "what is a B-tree".to_string());
    let top_k = args
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5);

    let cfg = config::load()?;
    println!("[sample_query] {}", client::backend_info()?);

    let db = Arc::new(storage::open_db(&cfg.paths.db_path)?);
    let index_backend = cfg.hnsw.backend.to_ascii_lowercase();
    let index: Arc<dyn VectorIndex> = if index_backend == "bruteforce" {
        let idx = if Path::new(&cfg.paths.index_path).exists() {
            BruteForceIndex::load_from_path(&cfg.paths.index_path)?
        } else {
            BruteForceIndex::new(cfg.embedding.dim)
        };
        Arc::new(idx)
    } else {
        let base_index: Arc<dyn VectorIndex> = if Path::new(&cfg.paths.index_path).exists() {
            Arc::new(HnswIndex::load_from_path(&cfg.paths.index_path)?)
        } else {
            Arc::new(HnswIndex::with_params(
                cfg.embedding.dim,
                cfg.hnsw.m,
                cfg.hnsw.ef_construction,
                cfg.hnsw.ef_search,
                cfg.hnsw.max_elements,
            ))
        };

        let delta = if Path::new(&cfg.paths.vector_delta_path).exists() {
            Some(BruteForceIndex::load_from_path(
                &cfg.paths.vector_delta_path,
            )?)
        } else {
            None
        };
        let tombstones = load_chunk_ids_from_cf(&db, storage::CF_VECTOR_TOMBSTONES)?;
        Arc::new(CompositeVectorIndex::new(base_index, delta, tombstones))
    };

    let lexical_meta = Path::new(&cfg.paths.lexical_index_path).join("meta.json");
    let lexical = if lexical_meta.exists() {
        Some(LexicalIndex::open(&cfg.paths.lexical_index_path)?)
    } else {
        None
    };

    let started = std::time::Instant::now();
    let results = query::run_query(&db, index.as_ref(), lexical.as_ref(), &query_text, top_k);
    println!(
        "[sample_query] query={query_text:?} hits={} elapsed_ms={}",
        results.len(),
        started.elapsed().as_millis()
    );

    for (idx, result) in results.iter().enumerate() {
        let heading = if result.heading_chain.is_empty() {
            "-".to_string()
        } else {
            result.heading_chain.join(" > ")
        };
        let preview = result
            .text
            .split_whitespace()
            .take(32)
            .collect::<Vec<_>>()
            .join(" ");

        println!(
            "{rank}. score={score:.3} url={url}\n   heading={heading}\n   text={preview}",
            rank = idx + 1,
            score = result.score,
            url = result.source_url,
        );
    }

    if results.is_empty() {
        return Err("sample query returned no results".into());
    }

    Ok(())
}

fn load_chunk_ids_from_cf(
    db: &rocksdb::DB,
    name: &str,
) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let cf = storage::cf(db, name)?;
    let mut out = HashSet::new();
    for item in db.iterator_cf(cf, rocksdb::IteratorMode::Start) {
        let (key, _) = item?;
        out.insert(String::from_utf8(key.to_vec())?);
    }
    Ok(out)
}
