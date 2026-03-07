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

    let seen = count_cf(&db, storage::CF_SEEN)?;
    let to_crawl = count_cf(&db, storage::CF_TO_CRAWL)?;
    let content = count_cf(&db, storage::CF_CONTENT)?;
    let chunks = count_cf(&db, storage::CF_CHUNKS)?;
    let embeddings = count_cf(&db, storage::CF_EMBEDDINGS)?;
    let wiki_embeddings = count_cf(&db, storage::CF_WIKI_EMBEDDINGS).unwrap_or(0);

    println!("seen_links={}", seen);
    println!("queued_links={}", to_crawl);
    println!("stored_pages={}", content);
    println!("stored_chunks={}", chunks);
    println!("stored_embeddings={}", embeddings);
    println!("stored_wiki_embeddings={}", wiki_embeddings);
    println!("total_known_links={}", seen + to_crawl);
    Ok(())
}
