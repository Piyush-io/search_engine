use crate::crawler::{persist, policy, robots, scheduler::CrawlScheduler, types::UrlTask};
use crate::{pipeline::PageState, storage};
use dashmap::DashMap;
use rocksdb::{DB, IteratorMode, WriteBatch};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

#[derive(Default)]
struct RecoveryCounters {
    invalid_frontier_urls: usize,
    invalid_content_urls: usize,
    invalid_page_states: usize,
    skipped_low_quality_hosts: usize,
}

pub async fn load_frontier_into_scheduler(
    db: &DB,
    scheduler: &Arc<CrawlScheduler>,
    per_domain_processed: &DashMap<String, usize>,
    robots_cache: &robots::RobotsCache,
) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    let to_crawl_cf = storage::cf(db, storage::CF_TO_CRAWL)?;
    let mut loaded = 0usize;
    let mut purged = 0usize;
    let mut delete_keys = Vec::new();
    let mut counters = RecoveryCounters::default();

    for item in db.iterator_cf(to_crawl_cf, IteratorMode::Start) {
        let (key, _) = item?;
        let raw_url = match String::from_utf8(key.to_vec()) {
            Ok(url) => url,
            Err(_) => {
                delete_keys.push(key.to_vec());
                purged += 1;
                counters.invalid_frontier_urls += 1;
                continue;
            }
        };

        match persist::try_build_task(&raw_url, db, per_domain_processed, robots_cache, 0)? {
            Some(task) => {
                scheduler.push_task(task).await;
                loaded += 1;
            }
            None => {
                delete_keys.push(key.to_vec());
                purged += 1;
            }
        }

        if (loaded + purged) % 10_000 == 0 {
            println!(
                "[recover] processed {} frontier items (loaded={}, purged={}, invalid_frontier_urls={})",
                loaded + purged,
                loaded,
                purged,
                counters.invalid_frontier_urls
            );
            if !delete_keys.is_empty() {
                let mut wb = WriteBatch::default();
                for key in delete_keys.drain(..) {
                    wb.delete_cf(to_crawl_cf, key);
                }
                db.write(wb)?;
            }
        }
    }

    if !delete_keys.is_empty() {
        let mut wb = WriteBatch::default();
        for key in delete_keys {
            wb.delete_cf(to_crawl_cf, key);
        }
        db.write(wb)?;
    }

    if counters.invalid_frontier_urls > 0 {
        eprintln!(
            "[recover] purged {} invalid frontier entries with non-UTF8 or unreadable URLs",
            counters.invalid_frontier_urls
        );
    }

    Ok((loaded, purged))
}

pub async fn seed_frontier(
    seeds: &[String],
    db: &DB,
    scheduler: &Arc<CrawlScheduler>,
    per_domain_processed: &DashMap<String, usize>,
    robots_cache: &robots::RobotsCache,
) -> Result<usize, Box<dyn std::error::Error>> {
    let to_crawl_cf = storage::cf(db, storage::CF_TO_CRAWL)?;
    let mut wb = WriteBatch::default();
    let mut seeded = 0usize;

    for seed in seeds {
        let Some(task) = persist::try_build_task(seed, db, per_domain_processed, robots_cache, 0)?
        else {
            continue;
        };

        if db.get_cf(to_crawl_cf, task.url.as_bytes())?.is_none() {
            wb.put_cf(to_crawl_cf, task.url.as_bytes(), []);
            seeded += 1;
        }
        scheduler.push_task(task).await;
    }

    if seeded > 0 {
        db.write(wb)?;
    }

    Ok(seeded)
}

pub async fn enqueue_due_recrawls(
    db: &DB,
    scheduler: &Arc<CrawlScheduler>,
    default_recrawl_after_ms: i64,
) -> Result<usize, Box<dyn std::error::Error>> {
    let content_cf = storage::cf(db, storage::CF_CONTENT)?;
    let to_crawl_cf = storage::cf(db, storage::CF_TO_CRAWL)?;
    let page_state_cf = storage::cf(db, storage::CF_PAGE_STATE)?;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default();

    let mut wb = WriteBatch::default();
    let mut queued = 0usize;
    let mut counters = RecoveryCounters::default();

    for item in db.iterator_cf(content_cf, IteratorMode::Start) {
        let (key, _) = item?;
        let raw_url = match String::from_utf8(key.to_vec()) {
            Ok(url) => url,
            Err(_) => {
                counters.invalid_content_urls += 1;
                continue;
            }
        };

        let Ok(url) = Url::parse(&raw_url) else {
            counters.invalid_content_urls += 1;
            continue;
        };

        if !policy::url_allowed(&url) {
            continue;
        }

        let Some(host) = url.host_str().map(str::to_string) else {
            counters.invalid_content_urls += 1;
            continue;
        };

        if !policy::host_is_high_quality(host.as_str()) {
            counters.skipped_low_quality_hosts += 1;
            continue;
        }

        let recrawl_after_ms = (policy::recrawl_days_for_host(
            (default_recrawl_after_ms / (24 * 60 * 60 * 1000)) as u64,
            host.as_str(),
        ) as i64)
            * 24
            * 60
            * 60
            * 1000;
        let recrawl_after_ms = if recrawl_after_ms > 0 {
            recrawl_after_ms
        } else {
            default_recrawl_after_ms
        };

        let last_fetch_ms = match db.get_cf(page_state_cf, raw_url.as_bytes())? {
            Some(bytes) => match serde_json::from_slice::<PageState>(&bytes) {
                Ok(state) => state.last_fetch_ms.max(state.last_crawled_ms),
                Err(_) => {
                    counters.invalid_page_states += 1;
                    0
                }
            },
            None => 0,
        };

        if last_fetch_ms > 0 && now_ms.saturating_sub(last_fetch_ms) < recrawl_after_ms {
            continue;
        }

        if db.get_cf(to_crawl_cf, raw_url.as_bytes())?.is_some() {
            continue;
        }

        let task = UrlTask {
            url: raw_url.clone(),
            host,
            depth: 0,
            priority: policy::score_url(&url, 0),
        };

        wb.put_cf(to_crawl_cf, raw_url.as_bytes(), []);
        scheduler.push_task(task).await;
        queued += 1;
    }

    if queued > 0 {
        db.write(wb)?;
    }

    if counters.invalid_content_urls > 0
        || counters.invalid_page_states > 0
        || counters.skipped_low_quality_hosts > 0
    {
        eprintln!(
            "[recover] due-recrawl scan skipped invalid_content_urls={} invalid_page_states={} low_quality_pages={}",
            counters.invalid_content_urls,
            counters.invalid_page_states,
            counters.skipped_low_quality_hosts
        );
    }

    Ok(queued)
}

pub fn load_existing_content_counts(
    db: &DB,
) -> Result<HashMap<String, usize>, Box<dyn std::error::Error>> {
    let content_cf = storage::cf(db, storage::CF_CONTENT)?;
    let mut map = HashMap::new();

    for item in db.iterator_cf(content_cf, IteratorMode::Start) {
        let (k, _) = item?;
        let url = String::from_utf8(k.to_vec())?;
        if let Ok(parsed) = Url::parse(&url) {
            if let Some(host) = parsed.host_str() {
                *map.entry(host.to_string()).or_insert(0) += 1;
            }
        }
    }

    Ok(map)
}
