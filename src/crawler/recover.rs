use std::collections::HashMap;
use std::sync::Arc;
use dashmap::DashMap;
use rocksdb::{DB, IteratorMode, WriteBatch};
use crate::crawler::{robots, scheduler::CrawlScheduler, persist};
use crate::storage;
use url::Url;

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

    for item in db.iterator_cf(to_crawl_cf, IteratorMode::Start) {
        let (key, _) = item?;
        let raw_url = String::from_utf8(key.to_vec())?;
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
            println!("[recover] processed {} frontier items (loaded={}, purged={})", loaded + purged, loaded, purged);
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

    Ok((loaded, purged))
}

pub async fn seed_frontier(
    seeds: &[&str],
    db: &DB,
    scheduler: &Arc<CrawlScheduler>,
    per_domain_processed: &DashMap<String, usize>,
    robots_cache: &robots::RobotsCache,
) -> Result<usize, Box<dyn std::error::Error>> {
    let to_crawl_cf = storage::cf(db, storage::CF_TO_CRAWL)?;
    let mut wb = WriteBatch::default();
    let mut seeded = 0usize;

    for seed in seeds {
        let Some(task) = persist::try_build_task(seed, db, per_domain_processed, robots_cache, 0)? else {
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
