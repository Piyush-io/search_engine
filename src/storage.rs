use rocksdb::{ColumnFamilyDescriptor, Options, DB};

pub const CF_SEEN: &str = "seen";
pub const CF_TO_CRAWL: &str = "to_crawl";
pub const CF_DOMAINS: &str = "domains";
pub const CF_ROBOTS: &str = "robots";
pub const CF_CONTENT: &str = "content";
pub const CF_CHUNKS: &str = "chunks";
pub const CF_EMBEDDINGS: &str = "embeddings";
pub const CF_CLICKS: &str = "clicks";
pub const CF_WIKI: &str = "wiki";

pub fn all_cf_names() -> Vec<&'static str> {
    vec![
        CF_SEEN,
        CF_TO_CRAWL,
        CF_DOMAINS,
        CF_ROBOTS,
        CF_CONTENT,
        CF_CHUNKS,
        CF_EMBEDDINGS,
        CF_CLICKS,
        CF_WIKI,
    ]
}

pub fn open_db(path: &str) -> Result<DB, Box<dyn std::error::Error>> {
    let mut db_options = Options::default();
    db_options.create_if_missing(true);
    db_options.set_error_if_exists(false);
    db_options.create_missing_column_families(true);

    let cpus = std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(4);
    db_options.increase_parallelism(cpus);
    db_options.set_max_background_jobs(cpus);
    db_options.optimize_level_style_compaction(512 * 1024 * 1024);
    db_options.set_allow_mmap_reads(true);

    let mut cf_ops = Options::default();
    cf_ops.set_write_buffer_size(64 * 1024 * 1024);
    cf_ops.set_max_write_buffer_number(4);
    cf_ops.set_target_file_size_base(64 * 1024 * 1024);

    let descriptors = vec![
        ColumnFamilyDescriptor::new(CF_SEEN, cf_ops.clone()),
        ColumnFamilyDescriptor::new(CF_TO_CRAWL, cf_ops.clone()),
        ColumnFamilyDescriptor::new(CF_DOMAINS, cf_ops.clone()),
        ColumnFamilyDescriptor::new(CF_ROBOTS, cf_ops.clone()),
        ColumnFamilyDescriptor::new(CF_CONTENT, cf_ops.clone()),
        ColumnFamilyDescriptor::new(CF_CHUNKS, cf_ops.clone()),
        ColumnFamilyDescriptor::new(CF_EMBEDDINGS, cf_ops.clone()),
        ColumnFamilyDescriptor::new(CF_CLICKS, cf_ops.clone()),
        ColumnFamilyDescriptor::new(CF_WIKI, cf_ops),
    ];

    let db = DB::open_cf_descriptors(&db_options, path, descriptors)?;
    Ok(db)
}

pub fn open_db_read_only(path: &str) -> Result<DB, Box<dyn std::error::Error>> {
    let mut db_options = Options::default();
    db_options.set_allow_mmap_reads(true);
    let db = DB::open_cf_for_read_only(&db_options, path, all_cf_names(), false)?;
    Ok(db)
}

pub fn cf<'a>(db: &'a DB, name: &str) -> Result<rocksdb::ColumnFamilyRef<'a>, Box<dyn std::error::Error>> {
    db.cf_handle(name)
        .ok_or_else(|| format!("missing column family: {name}").into())
}
