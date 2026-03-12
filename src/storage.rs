use rocksdb::{BlockBasedOptions, Cache, ColumnFamilyDescriptor, DB, Options};

pub const CF_SEEN: &str = "seen";
pub const CF_TO_CRAWL: &str = "to_crawl";
pub const CF_DOMAINS: &str = "domains";
pub const CF_ROBOTS: &str = "robots";
pub const CF_CONTENT: &str = "content";
pub const CF_CHUNKS: &str = "chunks";
pub const CF_EMBEDDINGS: &str = "embeddings";
pub const CF_CLICKS: &str = "clicks";
pub const CF_WIKI: &str = "wiki";
pub const CF_WIKI_EMBEDDINGS: &str = "wiki_embeddings";
pub const CF_NORMALIZE_QUEUE: &str = "normalize_queue";

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
        CF_WIKI_EMBEDDINGS,
        CF_NORMALIZE_QUEUE,
    ]
}

/// Normal open: balanced read/write tuning, shared block cache.
/// Use for the server and index binaries.
pub fn open_db(path: &str) -> Result<DB, Box<dyn std::error::Error>> {
    open_db_internal(path, DbProfile::Normal, 256)
}

/// Normal open with a specific block-cache size (in MB).
pub fn open_db_with_cache(
    path: &str,
    block_cache_mb: usize,
) -> Result<DB, Box<dyn std::error::Error>> {
    open_db_internal(path, DbProfile::Normal, block_cache_mb)
}

/// Bulk-write open: small write buffers, no block cache.
/// Use for embed / wiki_embed to keep memory under control.
pub fn open_db_for_bulk_write(path: &str) -> Result<DB, Box<dyn std::error::Error>> {
    open_db_internal(path, DbProfile::BulkWrite, 0)
}

pub fn open_db_read_only(path: &str) -> Result<DB, Box<dyn std::error::Error>> {
    let _ = rlimit::increase_nofile_limit(10240);

    let mut db_options = Options::default();
    db_options.set_allow_mmap_reads(true);

    let cf_names = DB::list_cf(&db_options, path)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| all_cf_names().into_iter().map(|s| s.to_string()).collect());

    let db = DB::open_cf_for_read_only(&db_options, path, cf_names, false)?;
    Ok(db)
}

pub fn cf<'a>(
    db: &'a DB,
    name: &str,
) -> Result<rocksdb::ColumnFamilyRef<'a>, Box<dyn std::error::Error>> {
    db.cf_handle(name)
        .ok_or_else(|| format!("missing column family: {name}").into())
}

// ── internals ──────────────────────────────────────────────────────────────

enum DbProfile {
    Normal,
    BulkWrite,
}

fn open_db_internal(
    path: &str,
    profile: DbProfile,
    block_cache_mb: usize,
) -> Result<DB, Box<dyn std::error::Error>> {
    let _ = rlimit::increase_nofile_limit(10240);

    let cpus = std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(4);

    let mut db_opts = Options::default();
    db_opts.create_if_missing(true);
    db_opts.set_error_if_exists(false);
    db_opts.create_missing_column_families(true);
    db_opts.set_allow_mmap_reads(true);
    db_opts.increase_parallelism(cpus);

    match profile {
        DbProfile::Normal => {
            db_opts.set_max_background_jobs(cpus);
            db_opts.optimize_level_style_compaction(256 * 1024 * 1024);
        }
        DbProfile::BulkWrite => {
            // During bulk embed we write sequentially and don't read much.
            // Keep background jobs low to leave cores for the ORT model.
            db_opts.set_max_background_jobs(2);
            db_opts.optimize_level_style_compaction(64 * 1024 * 1024);
            // Avoid pinning every SST descriptor for long-running bulk jobs.
            db_opts.set_max_open_files(512);
        }
    }

    let cf_ops = cf_options(&profile, block_cache_mb);

    let descriptors: Vec<ColumnFamilyDescriptor> = all_cf_names()
        .into_iter()
        .map(|name| ColumnFamilyDescriptor::new(name, cf_ops.clone()))
        .collect();

    let db = DB::open_cf_descriptors(&db_opts, path, descriptors)?;
    Ok(db)
}

fn cf_options(profile: &DbProfile, block_cache_mb: usize) -> Options {
    let mut opts = Options::default();

    match profile {
        DbProfile::Normal => {
            // Shared block cache across all CFs — use configured size.
            let cache_bytes = block_cache_mb.max(64) * 1024 * 1024;
            let cache = Cache::new_lru_cache(cache_bytes);
            let mut bb_opts = BlockBasedOptions::default();
            bb_opts.set_block_cache(&cache);
            bb_opts.set_bloom_filter(10.0, false);
            opts.set_block_based_table_factory(&bb_opts);

            opts.set_write_buffer_size(32 * 1024 * 1024); // 32 MB per CF
            opts.set_max_write_buffer_number(2);
            opts.set_target_file_size_base(64 * 1024 * 1024);
        }
        DbProfile::BulkWrite => {
            // No block cache — we're not doing reads.
            // Tiny write buffers to cap RAM usage.
            opts.set_write_buffer_size(8 * 1024 * 1024); // 8 MB per CF
            opts.set_max_write_buffer_number(2);
            opts.set_target_file_size_base(64 * 1024 * 1024);
            // WAL is already disabled per-write in embed.rs; this is a safety net.
            opts.set_disable_auto_compactions(true);
        }
    }

    opts
}
