use rocksdb::ReadOptions;
use search_engine::{
    config,
    search::{bruteforce::BruteForceIndex, hnsw::HnswIndex},
    storage,
};

const READAHEAD_BYTES: usize = 8 * 1024 * 1024;
const PARALLEL_CHUNK: usize = 50_000;

fn decode_vector(value: &[u8], dim: usize) -> Option<Vec<f32>> {
    // Fast path: raw little-endian f32 bytes written by embed.rs.
    // RocksDB does NOT guarantee pointer alignment, so we copy into an aligned
    // Vec<f32> instead of casting the slice pointer directly (which bytemuck
    // rejects with TargetAlignmentGreaterAndInputNotAligned when unaligned).
    if value.len() == dim * std::mem::size_of::<f32>() {
        let mut out = vec![0f32; dim];
        // SAFETY: out is a &mut [f32] which has the required alignment; we copy
        // raw bytes from value into it using the safe bytemuck cast on the dst.
        let dst: &mut [u8] = bytemuck::cast_slice_mut(&mut out);
        dst.copy_from_slice(value);
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

fn remove_stale_hnsw_artifacts(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let graph_path = format!("{path}.hnsw.graph");
    let data_path = format!("{path}.hnsw.data");

    match std::fs::remove_file(path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    match std::fs::remove_file(&graph_path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    match std::fs::remove_file(&data_path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    Ok(())
}

fn load_hnsw_batch(
    iter: &mut rocksdb::DBRawIteratorWithThreadMode<'_, rocksdb::DB>,
    dim: usize,
    inserted: usize,
    skipped: &mut usize,
) -> Result<Vec<(String, Vec<f32>)>, Box<dyn std::error::Error>> {
    let mut entries = Vec::with_capacity(PARALLEL_CHUNK);

    while iter.valid() && entries.len() < PARALLEL_CHUNK {
        let (key, value) = match (iter.key(), iter.value()) {
            (Some(k), Some(v)) => (k, v),
            _ => break,
        };

        let chunk_id = String::from_utf8(key.to_vec())?;
        match decode_vector(value, dim) {
            Some(vector) => entries.push((chunk_id, vector)),
            None => *skipped += 1,
        }

        let scanned = inserted + entries.len() + *skipped;
        if scanned % 250_000 == 0 {
            println!(
                "[index] scanned={} inserted_so_far={} skipped={}",
                scanned, inserted, *skipped
            );
        }

        iter.next();
    }

    Ok(entries)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = storage::open_db(&cfg.paths.db_path)?;

    let embeddings_cf = storage::cf(&db, storage::CF_EMBEDDINGS)?;

    // Use readahead for sequential bulk scan — avoids block cache pollution
    let mut read_opts = ReadOptions::default();
    read_opts.fill_cache(false);
    read_opts.set_readahead_size(READAHEAD_BYTES);
    read_opts.set_auto_readahead_size(true);

    let backend = cfg.hnsw.backend.to_ascii_lowercase();
    if backend == "bruteforce" {
        let mut index = BruteForceIndex::new(cfg.embedding.dim);
        let mut inserted = 0usize;
        let mut skipped = 0usize;

        let mut iter = db.raw_iterator_cf_opt(&embeddings_cf, read_opts);
        iter.seek_to_first();
        while iter.valid() {
            let (key, value) = match (iter.key(), iter.value()) {
                (Some(k), Some(v)) => (k, v),
                _ => break,
            };
            let chunk_id = String::from_utf8(key.to_vec())?;

            match decode_vector(value, cfg.embedding.dim) {
                Some(vector) => {
                    index.insert(chunk_id, vector);
                    inserted += 1;
                    if inserted % 5_000 == 0 {
                        println!("[index] inserted={} entries (bruteforce)", inserted);
                    }
                }
                None => skipped += 1,
            }
            iter.next();
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

    println!("[index] streaming embeddings into bounded HNSW build batches…");

    let mut iter = db.raw_iterator_cf_opt(&embeddings_cf, read_opts);
    iter.seek_to_first();

    let mut index = HnswIndex::with_params(
        cfg.embedding.dim,
        cfg.hnsw.m,
        cfg.hnsw.ef_construction,
        cfg.hnsw.ef_search,
        cfg.hnsw.max_elements,
    );

    let mut inserted = 0usize;
    let mut skipped = 0usize;

    loop {
        let entries = load_hnsw_batch(&mut iter, cfg.embedding.dim, inserted, &mut skipped)?;
        if entries.is_empty() {
            break;
        }

        if inserted == 0 {
            println!(
                "[index] initializing HNSW with first batch of {}",
                entries.len()
            );
        }

        let data: Vec<(&Vec<f32>, usize)> = entries
            .iter()
            .enumerate()
            .map(|(i, (_, v))| (v, inserted + i))
            .collect();

        index.parallel_insert_slice(&data);
        for (chunk_id, _) in &entries {
            index.push_chunk_id(chunk_id.clone());
        }

        inserted += entries.len();

        println!(
            "[index] inserted batch_size={} cumulative_inserted={} skipped={}",
            entries.len(),
            inserted,
            skipped
        );
    }

    if inserted == 0 && skipped > 0 {
        return Err(format!(
            "all embeddings were skipped due to dim mismatch (expected dim={}). Re-run embed after clearing old vectors.",
            cfg.embedding.dim
        )
        .into());
    }

    if inserted == 0 {
        remove_stale_hnsw_artifacts(&cfg.paths.index_path)?;
        println!(
            "[index] no embeddings found; removed stale HNSW artifacts at {}",
            cfg.paths.index_path
        );
        return Ok(());
    }

    index.save_to_path(&cfg.paths.index_path)?;
    println!(
        "[index] done. backend=hnsw entries={} skipped={} saved_to={}",
        inserted, skipped, cfg.paths.index_path
    );

    Ok(())
}
