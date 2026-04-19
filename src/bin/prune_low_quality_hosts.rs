use std::collections::HashMap;

use rocksdb::{IteratorMode, WriteBatch};
use search_engine::{
    Chunk, config,
    crawler::policy,
    pipeline::{IndexOperation, PageState},
    storage,
};
use url::Url;

const WRITE_BATCH_LIMIT: usize = 10_000;

#[derive(Debug, Clone, Copy)]
enum PruneReason {
    InvalidUrl,
    DisallowedByPolicy,
    NonHighQualityHost,
}

impl PruneReason {
    fn label(self) -> &'static str {
        match self {
            Self::InvalidUrl => "invalid_url",
            Self::DisallowedByPolicy => "disallowed_by_policy",
            Self::NonHighQualityHost => "non_high_quality_host",
        }
    }
}

#[derive(Default)]
struct Stats {
    content_rows_deleted: usize,
    page_state_rows_deleted: usize,
    normalize_queue_rows_deleted: usize,
    frontier_rows_deleted: usize,
    chunk_rows_matched: usize,
    chunk_ids_from_page_state: usize,
    empty_page_states: usize,
    invalid_page_states: usize,
    invalid_content_keys: usize,
    invalid_page_state_keys: usize,
    invalid_normalize_queue_keys: usize,
    invalid_frontier_keys: usize,
    invalid_chunks: usize,
    reason_counts: HashMap<&'static str, usize>,
    host_counts: HashMap<String, usize>,
}

fn has_flag(flag: &str) -> bool {
    std::env::args().any(|arg| arg == flag)
}

fn classify_url(raw_url: &str, prune_non_high_quality: bool) -> Option<(String, PruneReason)> {
    let Ok(url) = Url::parse(raw_url) else {
        return Some(("<invalid-url>".to_string(), PruneReason::InvalidUrl));
    };

    let host = url.host_str().unwrap_or("<invalid-url>").to_string();
    if !policy::url_allowed(&url) {
        return Some((host, PruneReason::DisallowedByPolicy));
    }

    if prune_non_high_quality && !policy::host_is_high_quality(&host) {
        return Some((host, PruneReason::NonHighQualityHost));
    }

    None
}

fn note_reason(stats: &mut Stats, host: &str, reason: PruneReason) {
    *stats.reason_counts.entry(reason.label()).or_insert(0) += 1;
    *stats.host_counts.entry(host.to_string()).or_insert(0) += 1;
}

fn maybe_write_batch(
    db: &rocksdb::DB,
    wb: &mut WriteBatch,
    pending: &mut usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if *pending < WRITE_BATCH_LIMIT {
        return Ok(());
    }

    db.write(std::mem::take(wb))?;
    *pending = 0;
    Ok(())
}

fn flush_batch(
    db: &rocksdb::DB,
    wb: &mut WriteBatch,
    pending: &mut usize,
) -> Result<(), Box<dyn std::error::Error>> {
    if *pending > 0 {
        db.write(std::mem::take(wb))?;
        *pending = 0;
    }
    Ok(())
}

fn enqueue_chunk_delete(
    wb: &mut WriteBatch,
    chunk_id: &[u8],
    chunks_cf: rocksdb::ColumnFamilyRef<'_>,
    embeddings_cf: rocksdb::ColumnFamilyRef<'_>,
    embed_queue_cf: rocksdb::ColumnFamilyRef<'_>,
    vector_queue_cf: rocksdb::ColumnFamilyRef<'_>,
    lexical_queue_cf: rocksdb::ColumnFamilyRef<'_>,
    tombstones_cf: rocksdb::ColumnFamilyRef<'_>,
) {
    wb.delete_cf(chunks_cf, chunk_id);
    wb.delete_cf(embeddings_cf, chunk_id);
    wb.delete_cf(embed_queue_cf, chunk_id);
    wb.put_cf(vector_queue_cf, chunk_id, IndexOperation::Delete.as_bytes());
    wb.put_cf(
        lexical_queue_cf,
        chunk_id,
        IndexOperation::Delete.as_bytes(),
    );
    wb.put_cf(tombstones_cf, chunk_id, []);
}

fn print_top_hosts(host_counts: &HashMap<String, usize>) {
    let mut hosts: Vec<_> = host_counts.iter().collect();
    hosts.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

    if hosts.is_empty() {
        return;
    }

    println!("top_pruned_hosts:");
    for (host, count) in hosts.into_iter().take(20) {
        println!("  {host:<40} {count}");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dry_run = has_flag("--dry-run");
    let prune_non_high_quality = has_flag("--prune-non-high-quality");

    let cfg = config::load()?;
    let db = storage::open_db_with_cache(&cfg.paths.db_path, cfg.rocksdb.block_cache_mb)?;

    let content_cf = storage::cf(&db, storage::CF_CONTENT)?;
    let page_state_cf = storage::cf(&db, storage::CF_PAGE_STATE)?;
    let normalize_queue_cf = storage::cf(&db, storage::CF_NORMALIZE_QUEUE)?;
    let to_crawl_cf = storage::cf(&db, storage::CF_TO_CRAWL)?;
    let chunks_cf = storage::cf(&db, storage::CF_CHUNKS)?;
    let embeddings_cf = storage::cf(&db, storage::CF_EMBEDDINGS)?;
    let embed_queue_cf = storage::cf(&db, storage::CF_EMBED_QUEUE)?;
    let vector_queue_cf = storage::cf(&db, storage::CF_VECTOR_QUEUE)?;
    let lexical_queue_cf = storage::cf(&db, storage::CF_LEXICAL_QUEUE)?;
    let tombstones_cf = storage::cf(&db, storage::CF_VECTOR_TOMBSTONES)?;

    let mut stats = Stats::default();

    println!(
        "[prune_low_quality_hosts] mode={} prune_non_high_quality={}",
        if dry_run { "dry-run" } else { "apply" },
        prune_non_high_quality
    );
    println!(
        "[prune_low_quality_hosts] pruning stored pages and chunk lineage offline; stop crawl/normalize/embed/index jobs first"
    );
    if prune_non_high_quality {
        println!(
            "[prune_low_quality_hosts] warning: --prune-non-high-quality uses the stricter host_is_high_quality set, not the broader crawl allowlist; review the dry-run host list before applying"
        );
    }

    {
        let mut wb = WriteBatch::default();
        let mut pending = 0usize;

        for item in db.iterator_cf(content_cf, IteratorMode::Start) {
            let (key, _) = item?;
            let raw_url = match String::from_utf8(key.to_vec()) {
                Ok(url) => url,
                Err(_) => {
                    stats.invalid_content_keys += 1;
                    continue;
                }
            };

            let Some((host, reason)) = classify_url(&raw_url, prune_non_high_quality) else {
                continue;
            };

            stats.content_rows_deleted += 1;
            note_reason(&mut stats, &host, reason);

            if !dry_run {
                wb.delete_cf(content_cf, key.as_ref());
                pending += 1;
                maybe_write_batch(&db, &mut wb, &mut pending)?;
            }
        }

        flush_batch(&db, &mut wb, &mut pending)?;
    }

    {
        let mut wb = WriteBatch::default();
        let mut pending = 0usize;

        for item in db.iterator_cf(page_state_cf, IteratorMode::Start) {
            let (key, value) = item?;
            let raw_url = match String::from_utf8(key.to_vec()) {
                Ok(url) => url,
                Err(_) => {
                    stats.invalid_page_state_keys += 1;
                    continue;
                }
            };

            if classify_url(&raw_url, prune_non_high_quality).is_none() {
                continue;
            }

            stats.page_state_rows_deleted += 1;

            match serde_json::from_slice::<PageState>(&value) {
                Ok(state) => {
                    if state.chunk_ids.is_empty() {
                        stats.empty_page_states += 1;
                    } else {
                        stats.chunk_ids_from_page_state += state.chunk_ids.len();
                        for chunk_id in state.chunk_ids {
                            if !dry_run {
                                enqueue_chunk_delete(
                                    &mut wb,
                                    chunk_id.as_bytes(),
                                    chunks_cf,
                                    embeddings_cf,
                                    embed_queue_cf,
                                    vector_queue_cf,
                                    lexical_queue_cf,
                                    tombstones_cf,
                                );
                                pending += 1;
                            }
                        }
                    }
                }
                Err(_) => {
                    stats.invalid_page_states += 1;
                }
            }

            if !dry_run {
                wb.delete_cf(page_state_cf, key.as_ref());
                pending += 1;
                maybe_write_batch(&db, &mut wb, &mut pending)?;
            }
        }

        flush_batch(&db, &mut wb, &mut pending)?;
    }

    {
        let mut wb = WriteBatch::default();
        let mut pending = 0usize;

        for item in db.iterator_cf(normalize_queue_cf, IteratorMode::Start) {
            let (key, _) = item?;
            let raw_url = match String::from_utf8(key.to_vec()) {
                Ok(url) => url,
                Err(_) => {
                    stats.invalid_normalize_queue_keys += 1;
                    continue;
                }
            };

            if classify_url(&raw_url, prune_non_high_quality).is_none() {
                continue;
            }

            stats.normalize_queue_rows_deleted += 1;

            if !dry_run {
                wb.delete_cf(normalize_queue_cf, key.as_ref());
                pending += 1;
                maybe_write_batch(&db, &mut wb, &mut pending)?;
            }
        }

        flush_batch(&db, &mut wb, &mut pending)?;
    }

    {
        let mut wb = WriteBatch::default();
        let mut pending = 0usize;

        for item in db.iterator_cf(to_crawl_cf, IteratorMode::Start) {
            let (key, _) = item?;
            let raw_url = match String::from_utf8(key.to_vec()) {
                Ok(url) => url,
                Err(_) => {
                    stats.invalid_frontier_keys += 1;
                    continue;
                }
            };

            if classify_url(&raw_url, prune_non_high_quality).is_none() {
                continue;
            }

            stats.frontier_rows_deleted += 1;

            if !dry_run {
                wb.delete_cf(to_crawl_cf, key.as_ref());
                pending += 1;
                maybe_write_batch(&db, &mut wb, &mut pending)?;
            }
        }

        flush_batch(&db, &mut wb, &mut pending)?;
    }

    {
        let mut wb = WriteBatch::default();
        let mut pending = 0usize;

        for item in db.iterator_cf(chunks_cf, IteratorMode::Start) {
            let (key, value) = item?;
            let chunk = match serde_json::from_slice::<Chunk>(&value) {
                Ok(chunk) => chunk,
                Err(_) => {
                    stats.invalid_chunks += 1;
                    continue;
                }
            };

            if classify_url(&chunk.source_url, prune_non_high_quality).is_none() {
                continue;
            }

            stats.chunk_rows_matched += 1;

            if !dry_run {
                enqueue_chunk_delete(
                    &mut wb,
                    key.as_ref(),
                    chunks_cf,
                    embeddings_cf,
                    embed_queue_cf,
                    vector_queue_cf,
                    lexical_queue_cf,
                    tombstones_cf,
                );
                pending += 1;
                maybe_write_batch(&db, &mut wb, &mut pending)?;
            }
        }

        flush_batch(&db, &mut wb, &mut pending)?;
    }

    println!("content_rows_deleted={}", stats.content_rows_deleted);
    println!("page_state_rows_deleted={}", stats.page_state_rows_deleted);
    println!(
        "normalize_queue_rows_deleted={}",
        stats.normalize_queue_rows_deleted
    );
    println!("frontier_rows_deleted={}", stats.frontier_rows_deleted);
    println!("chunk_rows_matched={}", stats.chunk_rows_matched);
    println!(
        "chunk_ids_from_page_state={}",
        stats.chunk_ids_from_page_state
    );
    println!("empty_page_states={}", stats.empty_page_states);
    println!("invalid_page_states={}", stats.invalid_page_states);
    println!("invalid_content_keys={}", stats.invalid_content_keys);
    println!("invalid_page_state_keys={}", stats.invalid_page_state_keys);
    println!(
        "invalid_normalize_queue_keys={}",
        stats.invalid_normalize_queue_keys
    );
    println!("invalid_frontier_keys={}", stats.invalid_frontier_keys);
    println!("invalid_chunks={}", stats.invalid_chunks);

    if !stats.reason_counts.is_empty() {
        println!("reason_counts:");
        let mut reasons: Vec<_> = stats.reason_counts.iter().collect();
        reasons.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (reason, count) in reasons {
            println!("  {reason}={count}");
        }
    }
    print_top_hosts(&stats.host_counts);

    if dry_run {
        println!(
            "[prune_low_quality_hosts] dry run only; rerun without --dry-run to apply deletions"
        );
    } else {
        let estimated_unique_chunk_deletes = stats
            .chunk_rows_matched
            .max(stats.chunk_ids_from_page_state);
        println!(
            "[prune_low_quality_hosts] apply lexical/vector deletes with: cargo run --release --bin lexical_index && cargo run --release --bin index"
        );
        if estimated_unique_chunk_deletes >= 100_000 {
            println!(
                "[prune_low_quality_hosts] large delete set detected; consider ./start_pipeline.sh afterward to compact tombstones into a fresh base snapshot"
            );
        }
    }

    Ok(())
}
