use rocksdb::{IteratorMode, WriteBatch};
use search_engine::{config, storage};

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

fn remove_dir_if_exists(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    match std::fs::remove_dir_all(path) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = storage::open_db_with_cache(&cfg.paths.db_path, cfg.rocksdb.block_cache_mb)?;

    if has_flag("--reset-derived") {
        clear_cf(&db, storage::cf(&db, storage::CF_CHUNKS)?)?;
        clear_cf(&db, storage::cf(&db, storage::CF_EMBEDDINGS)?)?;
        clear_cf(&db, storage::cf(&db, storage::CF_PAGE_STATE)?)?;
        clear_cf(&db, storage::cf(&db, storage::CF_EMBED_QUEUE)?)?;
        clear_cf(&db, storage::cf(&db, storage::CF_VECTOR_QUEUE)?)?;
        clear_cf(&db, storage::cf(&db, storage::CF_LEXICAL_QUEUE)?)?;
        clear_cf(&db, storage::cf(&db, storage::CF_VECTOR_TOMBSTONES)?)?;

        remove_file_if_exists(&cfg.paths.index_path)?;
        remove_file_if_exists(&format!("{}.hnsw.graph", cfg.paths.index_path))?;
        remove_file_if_exists(&format!("{}.hnsw.data", cfg.paths.index_path))?;
        remove_file_if_exists(&cfg.paths.vector_delta_path)?;
        remove_dir_if_exists(&cfg.paths.lexical_index_path)?;
    }

    let content_cf = storage::cf(&db, storage::CF_CONTENT)?;
    let normalize_queue_cf = storage::cf(&db, storage::CF_NORMALIZE_QUEUE)?;

    let mut queued = 0usize;
    let mut wb = WriteBatch::default();
    let mut pending = 0usize;
    for item in db.iterator_cf(content_cf, IteratorMode::Start) {
        let (key, _) = item?;
        wb.put_cf(normalize_queue_cf, key.as_ref(), []);
        queued += 1;
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

    println!(
        "[requeue_all_pages] queued {} stored pages for normalization{}",
        queued,
        if has_flag("--reset-derived") {
            " after clearing derived state"
        } else {
            ""
        }
    );

    Ok(())
}
