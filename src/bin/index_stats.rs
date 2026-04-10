use std::path::Path;

use rocksdb::IteratorMode;
use search_engine::{config, search::bruteforce::BruteForceIndex, storage};

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

    let delta_entries = if Path::new(&cfg.paths.vector_delta_path).exists() {
        BruteForceIndex::load_from_path(&cfg.paths.vector_delta_path)?.len()
    } else {
        0
    };

    println!("base_index_path={}", cfg.paths.index_path);
    println!(
        "base_index_exists={}",
        Path::new(&cfg.paths.index_path).exists()
    );
    println!("vector_delta_path={}", cfg.paths.vector_delta_path);
    println!("vector_delta_entries={}", delta_entries);
    println!(
        "vector_tombstones={}",
        count_cf(&db, storage::CF_VECTOR_TOMBSTONES)?
    );
    println!("vector_queue={}", count_cf(&db, storage::CF_VECTOR_QUEUE)?);
    println!(
        "lexical_index_exists={}",
        Path::new(&cfg.paths.lexical_index_path)
            .join("meta.json")
            .exists()
    );
    Ok(())
}
