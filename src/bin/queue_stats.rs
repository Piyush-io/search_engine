use rocksdb::IteratorMode;
use search_engine::{config, storage};

fn count_cf(db: &rocksdb::DB, name: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let cf = storage::cf(db, name)?;
    let mut n = 0usize;
    for item in db.iterator_cf(cf, IteratorMode::Start) {
        let _ = item?;
        n += 1;
    }
    Ok(n)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = storage::open_db_read_only(&cfg.paths.db_path)?;

    println!("to_crawl={}", count_cf(&db, storage::CF_TO_CRAWL)?);
    println!(
        "normalize_queue={}",
        count_cf(&db, storage::CF_NORMALIZE_QUEUE)?
    );
    println!("embed_queue={}", count_cf(&db, storage::CF_EMBED_QUEUE)?);
    println!("vector_queue={}", count_cf(&db, storage::CF_VECTOR_QUEUE)?);
    println!(
        "lexical_queue={}",
        count_cf(&db, storage::CF_LEXICAL_QUEUE)?
    );
    println!(
        "vector_tombstones={}",
        count_cf(&db, storage::CF_VECTOR_TOMBSTONES)?
    );
    Ok(())
}
