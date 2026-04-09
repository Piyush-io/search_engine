use std::path::Path;

use rocksdb::{IteratorMode, ReadOptions, WriteBatch};
use search_engine::{
    Chunk, config, pipeline::IndexOperation, search::lexical::LexicalIndex, storage,
};

const READAHEAD_BYTES: usize = 8 * 1024 * 1024;
const WRITER_HEAP_BYTES: usize = 256 * 1024 * 1024;
const COMMIT_EVERY: usize = 50_000;

fn has_flag(flag: &str) -> bool {
    std::env::args().any(|arg| arg == flag)
}

fn rebuild_full_index(cfg: &config::Config) -> Result<(), Box<dyn std::error::Error>> {
    let db = storage::open_db_with_cache(&cfg.paths.db_path, cfg.rocksdb.block_cache_mb)?;
    let chunks_cf = storage::cf(&db, storage::CF_CHUNKS)?;
    let lexical_queue_cf = storage::cf(&db, storage::CF_LEXICAL_QUEUE)?;

    let lexical = LexicalIndex::create_or_open(&cfg.paths.lexical_index_path)?;
    let mut writer = lexical.writer(WRITER_HEAP_BYTES)?;
    writer.delete_all_documents()?;

    let mut read_opts = ReadOptions::default();
    read_opts.fill_cache(false);
    read_opts.set_readahead_size(READAHEAD_BYTES);
    read_opts.set_auto_readahead_size(true);

    let mut indexed = 0usize;
    let mut iter = db.raw_iterator_cf_opt(&chunks_cf, read_opts);
    iter.seek_to_first();
    while iter.valid() {
        let Some(value) = iter.value() else {
            break;
        };

        if let Ok(chunk) = serde_json::from_slice::<Chunk>(value) {
            if let Some(doc) = lexical.document_for_chunk(&chunk) {
                writer.add_document(doc)?;
                indexed += 1;
                if indexed % COMMIT_EVERY == 0 {
                    writer.commit()?;
                    println!("[lexical_index] indexed={} (committed)", indexed);
                }
            }
        }

        iter.next();
    }

    writer.commit()?;
    let mut wb = WriteBatch::default();
    for item in db.iterator_cf(lexical_queue_cf, IteratorMode::Start) {
        let (key, _) = item?;
        wb.delete_cf(lexical_queue_cf, key.as_ref());
    }
    db.write(wb)?;
    println!(
        "[lexical_index] done. indexed={} path={}",
        indexed, cfg.paths.lexical_index_path
    );
    Ok(())
}

fn apply_incremental_updates(cfg: &config::Config) -> Result<(), Box<dyn std::error::Error>> {
    let db = storage::open_db_with_cache(&cfg.paths.db_path, cfg.rocksdb.block_cache_mb)?;
    let lexical_queue_cf = storage::cf(&db, storage::CF_LEXICAL_QUEUE)?;
    let chunks_cf = storage::cf(&db, storage::CF_CHUNKS)?;

    if db
        .iterator_cf(lexical_queue_cf, IteratorMode::Start)
        .next()
        .is_none()
    {
        println!("[lexical_index] lexical queue empty; nothing to update");
        return Ok(());
    }

    let lexical = LexicalIndex::create_or_open(&cfg.paths.lexical_index_path)?;
    let mut writer = lexical.writer(WRITER_HEAP_BYTES)?;
    let mut processed = 0usize;

    loop {
        let mut batch = Vec::new();
        for item in db
            .iterator_cf(lexical_queue_cf, IteratorMode::Start)
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
            writer.delete_term(lexical.chunk_term(&chunk_id));

            if matches!(
                IndexOperation::from_bytes(&value),
                Some(IndexOperation::Upsert)
            ) {
                if let Some(bytes) = db.get_cf(chunks_cf, key.as_slice())? {
                    if let Ok(chunk) = serde_json::from_slice::<Chunk>(&bytes) {
                        if let Some(doc) = lexical.document_for_chunk(&chunk) {
                            writer.add_document(doc)?;
                        }
                    }
                }
            }

            wb.delete_cf(lexical_queue_cf, key.as_slice());
            processed += 1;
        }

        db.write(wb)?;
        if processed % COMMIT_EVERY == 0 {
            writer.commit()?;
            println!(
                "[lexical_index] processed={} incremental updates",
                processed
            );
        }
    }

    writer.commit()?;
    println!("[lexical_index] applied {} incremental updates", processed);
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let meta_path = Path::new(&cfg.paths.lexical_index_path).join("meta.json");
    if has_flag("--full") || !meta_path.exists() {
        return rebuild_full_index(&cfg);
    }

    apply_incremental_updates(&cfg)
}
