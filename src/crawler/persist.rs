use std::sync::Arc;
use std::collections::HashSet;
use dashmap::DashMap;
use rocksdb::{DB, WriteBatch};
use url::Url;
use crate::crawler::types::{UrlTask, ParsedPage, RejectReason};
use crate::crawler::{canon, policy, robots, scheduler::CrawlScheduler};
use crate::{storage, PageRecord};
use std::time::{SystemTime, UNIX_EPOCH};

pub enum PersistCommand {
    Reject {
        url: String,
        host: String,
        aliases: Vec<String>,
        outlinks: Vec<String>,
        reason: RejectReason,
        depth: u16,
    },
    Accept {
        page: ParsedPage,
        aliases: Vec<String>,
        depth: u16,
    },
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

const MAX_DEPTH: u16 = 6;

pub fn try_build_task(
    raw_url: &str,
    db: &DB,
    per_domain_processed: &DashMap<String, usize>,
    robots_cache: &robots::RobotsCache,
    depth: u16,
) -> Result<Option<UrlTask>, Box<dyn std::error::Error>> {
    if depth > MAX_DEPTH {
        return Ok(None);
    }
    let Some(url) = canon::canonicalize(raw_url) else {
        return Ok(None);
    };
    if !policy::url_allowed(&url) {
        return Ok(None);
    }

    let Some(host) = url.host_str().map(str::to_string) else {
        return Ok(None);
    };
    if per_domain_processed.get(&host).map(|v| *v).unwrap_or(0) >= policy::domain_cap(&host) {
        return Ok(None);
    }

    let seen_cf = storage::cf(db, storage::CF_SEEN)?;
    if db.key_may_exist_cf(seen_cf, url.as_str().as_bytes())
        && db.get_cf(seen_cf, url.as_str().as_bytes())?.is_some()
    {
        return Ok(None);
    }

    let content_cf = storage::cf(db, storage::CF_CONTENT)?;
    if db.get_cf(content_cf, url.as_str().as_bytes())?.is_some() {
        return Ok(None);
    }

    if let Some(rules) = robots::get_cached_disallowed(&host, db, robots_cache)? {
        if !robots::is_allowed(url.path(), &rules) {
            return Ok(None);
        }
    }

    let priority = policy::score_url(&url, depth);

    Ok(Some(UrlTask {
        url: url.to_string(),
        host,
        depth,
        priority,
    }))
}

pub async fn persist_command(
    db: &DB,
    scheduler: &Arc<CrawlScheduler>,
    per_domain_processed: &DashMap<String, usize>,
    robots_cache: &robots::RobotsCache,
    command: PersistCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    let seen_cf = storage::cf(db, storage::CF_SEEN)?;
    let to_crawl_cf = storage::cf(db, storage::CF_TO_CRAWL)?;
    let domains_cf = storage::cf(db, storage::CF_DOMAINS)?;
    let content_cf = storage::cf(db, storage::CF_CONTENT)?;
    let norm_queue_cf = storage::cf(db, storage::CF_NORMALIZE_QUEUE)?;

    let mut wb = WriteBatch::default();

    match command {
        PersistCommand::Reject { url: _, host, aliases, outlinks, reason: _, depth } => {
            for alias in aliases {
                wb.put_cf(seen_cf, alias.as_bytes(), []);
                wb.delete_cf(to_crawl_cf, alias.as_bytes());
            }
            wb.put_cf(domains_cf, host.as_bytes(), now_ms().to_string().as_bytes());

            let tasks = enqueue_outlinks(db, &mut wb, outlinks, per_domain_processed, robots_cache, depth + 1)?;
            db.write(wb)?;

            for task in tasks {
                scheduler.push_task(task).await;
            }
        }
        PersistCommand::Accept { page, aliases, depth } => {
            for alias in aliases {
                wb.put_cf(seen_cf, alias.as_bytes(), []);
                wb.delete_cf(to_crawl_cf, alias.as_bytes());
            }
            wb.put_cf(domains_cf, page.page_record.url.as_bytes(), now_ms().to_string().as_bytes());

            let url_bytes = page.page_record.url.as_bytes();
            wb.put_cf(content_cf, url_bytes, serde_json::to_vec(&page.page_record)?);
            wb.put_cf(norm_queue_cf, url_bytes, []);

            let host = Url::parse(&page.page_record.url)?
                .host_str()
                .unwrap_or_default()
                .to_string();
            *per_domain_processed.entry(host).or_insert(0) += 1;

            let tasks = enqueue_outlinks(db, &mut wb, page.outlinks, per_domain_processed, robots_cache, depth + 1)?;
            db.write(wb)?;

            for task in tasks {
                scheduler.push_task(task).await;
            }
        }
    }

    Ok(())
}

fn enqueue_outlinks(
    db: &DB,
    wb: &mut WriteBatch,
    outlinks: Vec<String>,
    per_domain_processed: &DashMap<String, usize>,
    robots_cache: &robots::RobotsCache,
    depth: u16,
) -> Result<Vec<UrlTask>, Box<dyn std::error::Error>> {
    let to_crawl_cf = storage::cf(db, storage::CF_TO_CRAWL)?;
    let mut admitted = Vec::new();
    let mut local_seen = HashSet::new();

    for raw_url in outlinks {
        if !local_seen.insert(raw_url.clone()) {
            continue;
        }

        let Some(task) = try_build_task(&raw_url, db, per_domain_processed, robots_cache, depth)? else {
            continue;
        };

        if db.get_cf(to_crawl_cf, task.url.as_bytes())?.is_some() {
            continue;
        }

        wb.put_cf(to_crawl_cf, task.url.as_bytes(), []);
        admitted.push(task);
    }

    Ok(admitted)
}
