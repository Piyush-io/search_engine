use rocksdb::IteratorMode;
use search_engine::{
    config,
    search::{bruteforce::BruteForceIndex, hnsw::HnswIndex},
    storage,
};

fn decode_vector(value: &[u8], dim: usize) -> Option<Vec<f32>> {
    // Fast path: raw little-endian f32 bytes written by embed.rs
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

    // Backward compatibility: old bincode<Vec<f32>> payloads
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

    let embeddings_cf = storage::cf(&db, storage::CF_EMBEDDINGS)?;

    let backend = cfg.hnsw.backend.to_ascii_lowercase();
    if backend == "bruteforce" {
        let mut index = BruteForceIndex::new(cfg.embedding.dim);
        let mut inserted = 0usize;
        let mut skipped = 0usize;

        for item in db.iterator_cf(embeddings_cf, IteratorMode::Start) {
            let (key, value) = item?;
            let chunk_id = String::from_utf8(key.to_vec())?;

            let Some(vector) = decode_vector(value.as_ref(), cfg.embedding.dim) else {
                skipped += 1;
                continue;
            };

            index.insert(chunk_id, vector);
            inserted += 1;

            if inserted % 5_000 == 0 {
                println!("[index] inserted={} entries (bruteforce)", inserted);
            }
        }

        if inserted == 0 && skipped > 0 {
            return Err(format!(
                "all embeddings were skipped due to dim mismatch (expected dim={}). Re-run embed after clearing old vectors.",
                cfg.embedding.dim
            )
            .into());
        }

        index.save_to_path(&cfg.paths.index_path)?;
        println!(
            "[index] done. backend=bruteforce entries={} skipped={} saved_to={}",
            index.len(),
            skipped,
            cfg.paths.index_path
        );

        return Ok(());
    }

    let mut index = HnswIndex::with_params(
        cfg.embedding.dim,
        cfg.hnsw.m,
        cfg.hnsw.ef_construction,
        cfg.hnsw.ef_search,
        cfg.hnsw.max_elements,
    );
    let mut inserted = 0usize;
    let mut skipped = 0usize;

    for item in db.iterator_cf(embeddings_cf, IteratorMode::Start) {
        let (key, value) = item?;
        let chunk_id = String::from_utf8(key.to_vec())?;

        let Some(vector) = decode_vector(value.as_ref(), cfg.embedding.dim) else {
            skipped += 1;
            continue;
        };

        index.insert(chunk_id, vector);
        inserted += 1;

        if inserted % 5_000 == 0 {
            println!("[index] inserted={} entries (hnsw)", inserted);
        }
    }

    if inserted == 0 && skipped > 0 {
        return Err(format!(
            "all embeddings were skipped due to dim mismatch (expected dim={}). Re-run embed after clearing old vectors.",
            cfg.embedding.dim
        )
        .into());
    }

    index.save_to_path(&cfg.paths.index_path)?;
    println!(
        "[index] done. backend=hnsw entries={} skipped={} saved_to={}",
        index.len(),
        skipped,
        cfg.paths.index_path
    );

    Ok(())
}
