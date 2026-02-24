use rocksdb::IteratorMode;
use search_engine::{config, search::hnsw::HnswIndex, storage};

fn decode_vector(value: &[u8], dim: usize) -> Option<[f32; 768]> {
    // Fast path: raw little-endian f32 bytes written by embed.rs
    if value.len() == dim * std::mem::size_of::<f32>() {
        let mut out = [0.0_f32; 768];
        for (i, slot) in out.iter_mut().take(dim).enumerate() {
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
            let mut out = [0.0_f32; 768];
            out[..dim].copy_from_slice(&vector_vec[..dim]);
            return Some(out);
        }
    }

    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = storage::open_db(&cfg.paths.db_path)?;

    let embeddings_cf = storage::cf(&db, storage::CF_EMBEDDINGS)?;

    let mut index = HnswIndex::new(cfg.embedding.dim);
    let mut inserted = 0usize;

    for item in db.iterator_cf(embeddings_cf, IteratorMode::Start) {
        let (key, value) = item?;
        let chunk_id = String::from_utf8(key.to_vec())?;

        let Some(vector) = decode_vector(value.as_ref(), cfg.embedding.dim) else {
            continue;
        };

        index.insert(chunk_id, vector);
        inserted += 1;

        if inserted % 5_000 == 0 {
            println!("[index] inserted={} entries", inserted);
        }
    }

    index.save_to_path(&cfg.paths.index_path)?;
    println!(
        "[index] done. entries={} saved_to={}",
        index.len(),
        cfg.paths.index_path
    );

    Ok(())
}
