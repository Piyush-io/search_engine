use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use dashmap::DashMap;
use regex::Regex;
use reqwest::Client;
use tokio::sync::{Mutex, mpsc};
use tokio::time::{sleep, timeout};
use tracing_subscriber::EnvFilter;

use rocksdb::DB;
use search_engine::{
    config,
    crawler::{
        fetch, parse, persist,
        persist::PersistCommand,
        recover, robots,
        scheduler::CrawlScheduler,
        types::{FetchResult, RejectReason},
    },
    storage,
};

const PROCESS_ONE_TIMEOUT: Duration = Duration::from_secs(45);

fn load_seed_urls(path: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let re = Regex::new(r#"https://[^\s`|<>()\"]+"#)?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for m in re.find_iter(&text) {
        let url = m
            .as_str()
            .trim_end_matches(&['.', ',', ';', ')', ']'][..])
            .to_string();
        if seen.insert(url.clone()) {
            out.push(url);
        }
    }

    if out.is_empty() {
        return Err(format!(
            "no https seed URLs found in {}. Check that the seed file exists and contains at least one https:// URL.",
            path
        )
        .into());
    }

    Ok(out)
}

fn unique_aliases(urls: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for url in urls {
        if !url.is_empty() && seen.insert(url.clone()) {
            out.push(url);
        }
    }
    out
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if let Ok(directive) = "html5ever::tree_builder=off".parse() {
        env_filter = env_filter.add_directive(directive);
    }
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .try_init();

    let cfg = Arc::new(config::load()?);
    let seed_urls = load_seed_urls(&cfg.paths.seeds_path)?;
    let db = Arc::new(storage::open_db_with_cache(
        &cfg.paths.db_path,
        cfg.rocksdb.block_cache_mb,
    )?);

    let per_domain_counts = recover::load_existing_content_counts(&db)?;
    let existing_pages: usize = per_domain_counts.values().sum();
    let per_domain_processed: Arc<DashMap<String, usize>> = Arc::new(DashMap::new());
    for (host, count) in per_domain_counts {
        per_domain_processed.insert(host, count);
    }

    println!(
        "[crawl] start existing_pages={} target_max_pages={}",
        existing_pages, cfg.crawl.max_pages
    );

    let scheduler = Arc::new(CrawlScheduler::new(cfg.crawl.rate_limit_ms));
    let robots_cache = robots::new_cache();
    let (frontier_loaded, frontier_purged) = recover::load_frontier_into_scheduler(
        &db,
        &scheduler,
        &per_domain_processed,
        &robots_cache,
    )
    .await?;
    println!(
        "[crawl] frontier recovered: {} live, {} purged",
        frontier_loaded, frontier_purged
    );

    let seeded = recover::seed_frontier(
        &seed_urls,
        &db,
        &scheduler,
        &per_domain_processed,
        &robots_cache,
    )
    .await?;
    println!(
        "[crawl] loaded {} seeds from {} and enqueued {} fresh URLs",
        seed_urls.len(),
        cfg.paths.seeds_path,
        seeded
    );

    let recrawl_after_ms = (cfg.crawl.recrawl_days as i64) * 24 * 60 * 60 * 1000;
    let recrawl_seeded = recover::enqueue_due_recrawls(&db, &scheduler, recrawl_after_ms).await?;
    println!("[crawl] enqueued {} due recrawls", recrawl_seeded);

    let client = Arc::new(
        Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .read_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .user_agent("search-engine-crawler/0.2")
            .build()?,
    );

    let cpu_workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(2);

    let (parse_tx, parse_rx) = mpsc::channel::<FetchResult>(cfg.crawl.concurrency.max(1) * 2);
    let (persist_tx, persist_rx) =
        mpsc::channel::<PersistCommand>(cfg.crawl.concurrency.max(1) * 4);

    let dns_ok_cache: Arc<DashMap<String, bool>> = Arc::new(DashMap::new());
    let processed_pages = Arc::new(AtomicUsize::new(0));
    let new_pages_accepted = Arc::new(AtomicUsize::new(0));
    let fetch_inflight = Arc::new(AtomicUsize::new(0));
    let persist_pending = Arc::new(AtomicUsize::new(0));

    let writer_handle = tokio::spawn(writer_loop(
        Arc::clone(&db),
        Arc::clone(&scheduler),
        Arc::clone(&per_domain_processed),
        Arc::clone(&robots_cache),
        Arc::clone(&processed_pages),
        Arc::clone(&new_pages_accepted),
        Arc::clone(&persist_pending),
        persist_rx,
    ));

    let mut parse_handles = Vec::new();
    let parse_worker_count = cpu_workers.min(8);
    let parse_rx = Arc::new(Mutex::new(parse_rx));
    for _ in 0..parse_worker_count {
        let persist_tx = persist_tx.clone();
        let persist_pending = Arc::clone(&persist_pending);
        let parse_rx = Arc::clone(&parse_rx);
        parse_handles.push(tokio::spawn(async move {
            loop {
                let next = {
                    let mut guard = parse_rx.lock().await;
                    guard.recv().await
                };
                let Some(payload) = next else {
                    break;
                };

                let depth = payload.task.depth;
                let task_url = payload.task.url.clone();
                let task_host = payload.task.host.clone();
                let final_url = payload.final_url.clone();
                let final_host = payload.final_host.clone();
                let etag = payload.etag.clone();
                let last_modified = payload.last_modified.clone();

                let reject_host = if final_host.is_empty() {
                    task_host.clone()
                } else {
                    final_host.clone()
                };

                let command = if payload.not_modified {
                    PersistCommand::NotModified {
                        url: task_url.clone(),
                        host: task_host.clone(),
                        aliases: unique_aliases(vec![task_url.clone(), final_url.clone()]),
                        etag,
                        last_modified,
                    }
                } else {
                    match tokio::task::spawn_blocking(move || parse::parse_result(payload)).await {
                        Ok(Ok(page)) => {
                            let aliases = unique_aliases(vec![
                                task_url.clone(),
                                final_url.clone(),
                                page.final_url.clone(),
                                page.canonical_url.clone(),
                                page.page_record.url.clone(),
                            ]);
                            PersistCommand::Accept {
                                page,
                                aliases,
                                depth,
                            }
                        }
                        Ok(Err(reason)) => PersistCommand::Reject {
                            url: task_url.clone(),
                            host: reject_host.clone(),
                            aliases: unique_aliases(vec![task_url.clone(), final_url.clone()]),
                            outlinks: Vec::new(),
                            reason,
                            depth,
                        },
                        Err(_) => PersistCommand::Reject {
                            url: task_url.clone(),
                            host: reject_host.clone(),
                            aliases: unique_aliases(vec![task_url.clone(), final_url.clone()]),
                            outlinks: Vec::new(),
                            reason: RejectReason::ParsePanic,
                            depth,
                        },
                    }
                };

                persist_pending.fetch_add(1, Ordering::SeqCst);
                if persist_tx.send(command).await.is_err() {
                    persist_pending.fetch_sub(1, Ordering::SeqCst);
                    break;
                }
            }
        }));
    }

    let mut fetch_handles = Vec::new();
    for _ in 0..cfg.crawl.concurrency.max(1) {
        let db = Arc::clone(&db);
        let scheduler = Arc::clone(&scheduler);
        let client = Arc::clone(&client);
        let robots_cache = Arc::clone(&robots_cache);
        let dns_ok_cache = Arc::clone(&dns_ok_cache);
        let parse_tx = parse_tx.clone();
        let persist_tx = persist_tx.clone();
        let persist_pending = Arc::clone(&persist_pending);
        let fetch_inflight = Arc::clone(&fetch_inflight);

        fetch_handles.push(tokio::spawn(async move {
            while let Some(task) = scheduler.next_task().await {
                let host = task.host.clone();
                fetch_inflight.fetch_add(1, Ordering::SeqCst);

                let result = timeout(
                    PROCESS_ONE_TIMEOUT,
                    fetch::fetch_task(&db, &client, &robots_cache, &dns_ok_cache, task.clone()),
                )
                .await;

                match result {
                    Ok(Ok(payload)) => {
                        if parse_tx.send(payload).await.is_err() {
                            break;
                        }
                    }
                    Ok(Err(reason)) => {
                        persist_pending.fetch_add(1, Ordering::SeqCst);
                        let cmd = PersistCommand::Reject {
                            url: task.url.clone(),
                            host: task.host.clone(),
                            aliases: vec![task.url.clone()],
                            outlinks: Vec::new(),
                            reason,
                            depth: task.depth,
                        };
                        if persist_tx.send(cmd).await.is_err() {
                            persist_pending.fetch_sub(1, Ordering::SeqCst);
                            break;
                        }
                    }
                    Err(_) => {
                        persist_pending.fetch_add(1, Ordering::SeqCst);
                        let cmd = PersistCommand::Reject {
                            url: task.url.clone(),
                            host: task.host.clone(),
                            aliases: vec![task.url.clone()],
                            outlinks: Vec::new(),
                            reason: RejectReason::Timeout,
                            depth: task.depth,
                        };
                        if persist_tx.send(cmd).await.is_err() {
                            persist_pending.fetch_sub(1, Ordering::SeqCst);
                            break;
                        }
                    }
                }

                scheduler.complete_host(&host).await;
                fetch_inflight.fetch_sub(1, Ordering::SeqCst);
            }
        }));
    }

    let target_new_pages = cfg.crawl.max_pages.saturating_sub(existing_pages);
    let mut last_status = Instant::now();
    loop {
        let processed = processed_pages.load(Ordering::SeqCst);
        let accepted_new_pages = new_pages_accepted.load(Ordering::SeqCst);
        let (pending_urls, inflight_hosts, tracked_hosts) = scheduler.stats().await;

        if pending_urls == 0 && inflight_hosts == 0 && fetch_inflight.load(Ordering::SeqCst) == 0 {
            scheduler.close().await;
            break;
        }

        if target_new_pages > 0 && accepted_new_pages >= target_new_pages {
            scheduler.close().await;
            break;
        }

        if last_status.elapsed() >= Duration::from_secs(5) {
            println!(
                "[crawl] status pending_urls={} inflight_hosts={} tracked_hosts={} processed={} new_pages_accepted={} target_new_pages={} fetch_inflight={} persist_pending={}",
                pending_urls,
                inflight_hosts,
                tracked_hosts,
                processed,
                accepted_new_pages,
                target_new_pages,
                fetch_inflight.load(Ordering::SeqCst),
                persist_pending.load(Ordering::SeqCst)
            );
            last_status = Instant::now();
        }
        sleep(Duration::from_secs(1)).await;
    }

    for h in fetch_handles {
        let _ = h.await;
    }
    drop(parse_tx);
    for h in parse_handles {
        let _ = h.await;
    }
    drop(persist_tx);
    let _ = writer_handle.await;

    println!(
        "[crawl] finished. total processed this run: {}",
        processed_pages.load(Ordering::SeqCst)
    );
    Ok(())
}

async fn writer_loop(
    db: Arc<DB>,
    scheduler: Arc<CrawlScheduler>,
    per_domain_processed: Arc<DashMap<String, usize>>,
    robots_cache: robots::RobotsCache,
    processed_pages: Arc<AtomicUsize>,
    new_pages_accepted: Arc<AtomicUsize>,
    persist_pending: Arc<AtomicUsize>,
    mut persist_rx: mpsc::Receiver<PersistCommand>,
) {
    while let Some(command) = persist_rx.recv().await {
        let accepted_existing = match &command {
            PersistCommand::Accept { page, .. } => db
                .get_cf(
                    storage::cf(&db, storage::CF_CONTENT).expect("content cf"),
                    page.page_record.url.as_bytes(),
                )
                .ok()
                .flatten()
                .is_some(),
            _ => false,
        };
        let is_accept = matches!(command, PersistCommand::Accept { .. });
        if let Err(err) = persist::persist_command(
            &db,
            &scheduler,
            &per_domain_processed,
            &robots_cache,
            command,
        )
        .await
        {
            eprintln!("[crawl] persist error: {err}");
        } else if is_accept {
            processed_pages.fetch_add(1, Ordering::SeqCst);
            if !accepted_existing {
                new_pages_accepted.fetch_add(1, Ordering::SeqCst);
            }
        }
        persist_pending.fetch_sub(1, Ordering::SeqCst);
    }
}
