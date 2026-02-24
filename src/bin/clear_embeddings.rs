use rocksdb::IteratorMode;
use search_engine::{config, storage};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = storage::open_db(&cfg.paths.db_path)?;
    let embeddings_cf = storage::cf(&db, storage::CF_EMBEDDINGS)?;

    let mut keys = Vec::new();
    for item in db.iterator_cf(embeddings_cf, IteratorMode::Start) {
        let (k, _) = item?;
        keys.push(k.to_vec());
    }

    for k in &keys {
        db.delete_cf(embeddings_cf, k)?;
    }

    db.flush_wal(true)?;
    println!("[clear_embeddings] removed={}", keys.len());
    Ok(())
}
