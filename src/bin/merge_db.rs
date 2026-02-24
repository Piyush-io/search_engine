use rocksdb::IteratorMode;
use search_engine::storage;

fn merge_cf(
    src: &rocksdb::DB,
    dst: &rocksdb::DB,
    cf_name: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    let src_cf = storage::cf(src, cf_name)?;
    let dst_cf = storage::cf(dst, cf_name)?;

    let mut written = 0usize;

    for item in src.iterator_cf(src_cf, IteratorMode::Start) {
        let (k, v) = item?;

        if cf_name == storage::CF_TO_CRAWL {
            // Keep queue clean: don't enqueue URLs already seen in destination.
            let seen_cf = storage::cf(dst, storage::CF_SEEN)?;
            if dst.get_cf(seen_cf, &k)?.is_some() {
                continue;
            }

            if dst.get_cf(dst_cf, &k)?.is_none() {
                dst.put_cf(dst_cf, &k, &v)?;
                written += 1;
            }
            continue;
        }

        // Upsert for all other CFs.
        if dst.get_cf(dst_cf, &k)?.is_none() {
            written += 1;
        }
        dst.put_cf(dst_cf, &k, &v)?;
    }

    Ok(written)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let src_path = args
        .next()
        .ok_or("usage: cargo run --bin merge_db -- <source_db_path> [dest_db_path]")?;
    let dest_path = args.next().unwrap_or_else(|| "./crawl_data".to_string());

    let src = storage::open_db_read_only(&src_path)?;
    let dst = storage::open_db(&dest_path)?;

    let merge_order = [
        storage::CF_SEEN,
        storage::CF_ROBOTS,
        storage::CF_CONTENT,
        storage::CF_CHUNKS,
        storage::CF_EMBEDDINGS,
        storage::CF_WIKI,
        storage::CF_CLICKS,
        storage::CF_TO_CRAWL,
    ];

    println!("[merge_db] src={} -> dst={}", src_path, dest_path);
    for cf in merge_order {
        let n = merge_cf(&src, &dst, cf)?;
        println!("[merge_db] cf={} merged={}", cf, n);
    }

    dst.flush_wal(true)?;
    println!("[merge_db] done");

    Ok(())
}
