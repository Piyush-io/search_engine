use rocksdb::{DB, Options};

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "./crawl_data.niche".to_string());
    println!("Repairing {path}...");
    let mut opts = Options::default();
    opts.create_if_missing(false);
    opts.create_missing_column_families(true);
    match DB::repair(&opts, &path) {
        Ok(()) => println!("Repair succeeded!"),
        Err(e) => eprintln!("Repair failed: {e}"),
    }
}
