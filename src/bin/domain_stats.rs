use std::collections::HashMap;

use rocksdb::IteratorMode;
use search_engine::{config, storage};
use url::Url;

fn tally_cf(
    db: &rocksdb::DB,
    cf_name: &str,
    map: &mut HashMap<String, usize>,
) -> Result<usize, Box<dyn std::error::Error>> {
    let cf = storage::cf(db, cf_name)?;
    let mut total = 0usize;

    for item in db.iterator_cf(cf, IteratorMode::Start) {
        let (k, _) = item?;
        let url = String::from_utf8(k.to_vec())?;
        if let Ok(parsed) = Url::parse(&url) {
            if let Some(host) = parsed.host_str() {
                *map.entry(host.to_string()).or_insert(0) += 1;
                total += 1;
            }
        }
    }

    Ok(total)
}

fn parse_limit() -> usize {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--limit" {
            if let Some(value) = args.next() {
                if let Ok(limit) = value.parse::<usize>() {
                    return limit.max(1);
                }
            }
        }
    }
    20
}

fn print_top(title: &str, map: &HashMap<String, usize>, limit: usize) {
    let mut v: Vec<_> = map.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));

    println!("{}", title);
    for (idx, (domain, count)) in v.into_iter().take(limit).enumerate() {
        println!("{:>2}. {:<40} {}", idx + 1, domain, count);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limit = parse_limit();
    let cfg = config::load()?;
    let db = storage::open_db_read_only(&cfg.paths.db_path)?;

    let mut seen_map = HashMap::new();
    let mut queued_map = HashMap::new();

    let seen_total = tally_cf(&db, storage::CF_SEEN, &mut seen_map)?;
    let queued_total = tally_cf(&db, storage::CF_TO_CRAWL, &mut queued_map)?;

    println!("seen_total={} queued_total={}", seen_total, queued_total);
    println!();
    print_top("Top domains in seen:", &seen_map, limit);
    println!();
    print_top("Top domains in queue:", &queued_map, limit);

    Ok(())
}
