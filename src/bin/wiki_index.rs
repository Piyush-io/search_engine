use rocksdb::IteratorMode;
use search_engine::{config, search::hnsw::HnswIndex, storage};

fn decode_vector(value: &[u8], dim: usize) -> Option<Vec<f32>> {
    if value.len() == dim * std::mem::size_of::<f32>() {
        let mut out = vec![0.0_f32; dim];
        for (i, slot) in out.iter_mut().enumerate() {
            let start = i * 4;
            let bytes = [
                value[start],
                value[start + 1],
                value[start + 2],
                value[start + 3],
            ];
            *slot = f32::from_le_bytes(bytes);
        }
        return Some(out);
    }

    if let Ok(vector_vec) = bincode::deserialize::<Vec<f32>>(value) {
        if vector_vec.len() == dim {
            return Some(vector_vec);
        }
    }

    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = storage::open_db(&cfg.paths.db_path)?;

    let wiki_embeddings_cf = storage::cf(&db, storage::CF_WIKI_EMBEDDINGS)?;

    let mut index = HnswIndex::with_params(
        cfg.embedding.dim,
        cfg.hnsw.m,
        cfg.hnsw.ef_construction,
        cfg.hnsw.ef_search,
        cfg.hnsw.max_elements,
    );
    let mut inserted = 0usize;

    for item in db.iterator_cf(wiki_embeddings_cf, IteratorMode::Start) {
        let (key, value) = item?;
        let wiki_key = String::from_utf8(key.to_vec())?;

        let Some(vector) = decode_vector(value.as_ref(), cfg.embedding.dim) else {
            continue;
        };

        index.insert(wiki_key, vector);
        inserted += 1;

        if inserted % 5_000 == 0 {
            println!("[wiki_index] inserted={}", inserted);
        }
    }

    index.save_to_path(&cfg.paths.wiki_index_path)?;
    println!(
        "[wiki_index] done. entries={} saved_to={}",
        index.len(),
        cfg.paths.wiki_index_path
    );

    Ok(())
}
