use std::{collections::HashSet, path::Path, sync::Arc};

use crate::{
    config,
    search::{
        bruteforce::BruteForceIndex, composite::CompositeVectorIndex, hnsw::HnswIndex,
        lexical::LexicalIndex, vector_index::VectorIndex,
    },
    storage,
};

pub struct SearchStack {
    pub db: Arc<rocksdb::DB>,
    pub index: Arc<dyn VectorIndex>,
    pub lexical: Option<Arc<LexicalIndex>>,
}

pub fn load_search_stack() -> Result<SearchStack, Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = Arc::new(storage::open_db(&cfg.paths.db_path)?);

    let index_backend = cfg.hnsw.backend.to_ascii_lowercase();
    println!(
        "[server] loading vector index from {} (this may take a minute)...",
        cfg.paths.index_path
    );
    let t0 = std::time::Instant::now();

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

    println!(
        "[server] vector backend={} entries={} loaded in {:.1}s",
        index_backend,
        index.len(),
        t0.elapsed().as_secs_f64(),
    );

    let lexical_meta = Path::new(&cfg.paths.lexical_index_path).join("meta.json");
    let lexical = if lexical_meta.exists() {
        Some(Arc::new(LexicalIndex::open(&cfg.paths.lexical_index_path)?))
    } else {
        None
    };

    Ok(SearchStack { db, index, lexical })
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
