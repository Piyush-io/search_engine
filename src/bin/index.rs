use std::path::Path;

use rocksdb::{IteratorMode, ReadOptions, WriteBatch};
use search_engine::{
    config,
    pipeline::IndexOperation,
    search::{bruteforce::BruteForceIndex, hnsw::HnswIndex},
    storage,
};

const READAHEAD_BYTES: usize = 8 * 1024 * 1024;
const PARALLEL_CHUNK: usize = 50_000;
const DELTA_REBUILD_HINT: usize = 100_000;

fn decode_vector(value: &[u8], dim: usize) -> Option<Vec<f32>> {
    if value.len() == dim * std::mem::size_of::<f32>() {
        let mut out = vec![0f32; dim];
        let dst: &mut [u8] = bytemuck::cast_slice_mut(&mut out);
        dst.copy_from_slice(value);
        return Some(out);
    }

    if let Ok(vector_vec) = bincode::deserialize::<Vec<f32>>(value) {
        if vector_vec.len() == dim {
            return Some(vector_vec);
        }
    }

    None
}

fn has_flag(flag: &str) -> bool {
    std::env::args().any(|arg| arg == flag)
}

fn clear_cf(
    db: &rocksdb::DB,
    cf: rocksdb::ColumnFamilyRef<'_>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut wb = WriteBatch::default();
    let mut pending = 0usize;

    for item in db.iterator_cf(cf, IteratorMode::Start) {
        let (key, _) = item?;
        wb.delete_cf(cf, key.as_ref());
        pending += 1;
        if pending >= 10_000 {
            db.write(wb)?;
            wb = WriteBatch::default();
            pending = 0;
        }
    }

    if pending > 0 {
        db.write(wb)?;
    }

    Ok(())
}

fn remove_file_if_exists(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    match std::fs::remove_file(path) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn remove_stale_hnsw_artifacts(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    remove_file_if_exists(path)?;
    remove_file_if_exists(&format!("{path}.hnsw.graph"))?;
    remove_file_if_exists(&format!("{path}.hnsw.data"))?;
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

fn rebuild_full_index(
    cfg: &config::Config,
    db: &rocksdb::DB,
) -> Result<(), Box<dyn std::error::Error>> {
    let embeddings_cf = storage::cf(db, storage::CF_EMBEDDINGS)?;

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
    } else {
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
                .map(|(i, (_, vector))| (vector, inserted + i))
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
    }

    clear_cf(db, storage::cf(db, storage::CF_VECTOR_QUEUE)?)?;
    clear_cf(db, storage::cf(db, storage::CF_VECTOR_TOMBSTONES)?)?;
    remove_file_if_exists(&cfg.paths.vector_delta_path)?;
    Ok(())
}

fn apply_incremental_vector_updates(
    cfg: &config::Config,
    db: &rocksdb::DB,
) -> Result<(), Box<dyn std::error::Error>> {
    let vector_queue_cf = storage::cf(db, storage::CF_VECTOR_QUEUE)?;
    let tombstones_cf = storage::cf(db, storage::CF_VECTOR_TOMBSTONES)?;
    let embeddings_cf = storage::cf(db, storage::CF_EMBEDDINGS)?;

    if db
        .iterator_cf(vector_queue_cf, IteratorMode::Start)
        .next()
        .is_none()
    {
        println!("[index] vector queue empty; nothing to update");
        return Ok(());
    }

    let mut delta = if Path::new(&cfg.paths.vector_delta_path).exists() {
        BruteForceIndex::load_from_path(&cfg.paths.vector_delta_path)?
    } else {
        BruteForceIndex::new(cfg.embedding.dim)
    };

    let mut processed = 0usize;
    loop {
        let mut batch = Vec::new();
        for item in db
            .iterator_cf(vector_queue_cf, IteratorMode::Start)
            .take(10_000)
        {
            let (key, value) = item?;
            batch.push((key.to_vec(), value.to_vec()));
        }

        if batch.is_empty() {
            break;
        }

        let mut wb = WriteBatch::default();
        for (key, value) in batch {
            let chunk_id = String::from_utf8(key.clone())?;
            match IndexOperation::from_bytes(&value) {
                Some(IndexOperation::Delete) => {
                    delta.remove(&chunk_id);
                    wb.put_cf(tombstones_cf, key.as_slice(), []);
                }
                Some(IndexOperation::Upsert) => match db.get_cf(embeddings_cf, key.as_slice())? {
                    Some(bytes) => {
                        if let Some(vector) = decode_vector(&bytes, cfg.embedding.dim) {
                            delta.upsert(chunk_id, vector);
                            wb.delete_cf(tombstones_cf, key.as_slice());
                        } else {
                            delta.remove(&chunk_id);
                            wb.put_cf(tombstones_cf, key.as_slice(), []);
                        }
                    }
                    None => {
                        delta.remove(&chunk_id);
                        wb.put_cf(tombstones_cf, key.as_slice(), []);
                    }
                },
                None => {}
            }

            wb.delete_cf(vector_queue_cf, key.as_slice());
            processed += 1;
        }

        db.write(wb)?;
    }

    if delta.is_empty() {
        remove_file_if_exists(&cfg.paths.vector_delta_path)?;
    } else {
        delta.save_to_path(&cfg.paths.vector_delta_path)?;
    }

    println!(
        "[index] applied {} incremental vector updates; delta_entries={}",
        processed,
        delta.len()
    );
    if delta.len() >= DELTA_REBUILD_HINT {
        println!(
            "[index] delta has reached {} entries; consider running `index --full` to compact it into the base HNSW snapshot",
            delta.len()
        );
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = storage::open_db(&cfg.paths.db_path)?;

    if has_flag("--full") || !Path::new(&cfg.paths.index_path).exists() {
        return rebuild_full_index(&cfg, &db);
    }

    apply_incremental_vector_updates(&cfg, &db)
}
