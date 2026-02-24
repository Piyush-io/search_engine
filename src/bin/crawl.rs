use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures::{StreamExt, stream};
use reqwest::{Client, header::CONTENT_TYPE};
use rocksdb::{IteratorMode, WriteBatch};
use scraper::{Html, Selector};
use search_engine::{
    Chunk,
    chunking::{chaining, context, sentencizer},
    config,
    crawler::{canon, dns, robots},
    extraction::normalizer,
    storage,
};
use sha2::{Digest, Sha256};
use tokio::time::sleep;
use url::Url;

const SEED_URLS: &[&str] = &[
    "https://doc.rust-lang.org/",
    "https://docs.python.org/3/",
    "https://developer.mozilla.org/en-US/docs/Web/JavaScript",
    "https://en.wikipedia.org/wiki/Computer_science",
    "https://stackoverflow.com/questions/tagged/algorithms",
    "https://blog.rust-lang.org/",
    "https://blog.cloudflare.com/",
    "https://jvns.ca/",
    "https://martinfowler.com/",
    "https://blog.acolyer.org/",
    "https://eng.uber.com/",
    "https://netflixtechblog.com/",
    "https://research.google/blog/",
    "https://cppreference.com/",
];

const MAX_DISCOVERED_PER_PAGE: usize = 200;
const MAX_CHUNKS_PER_PAGE: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum SourceClass {
    Docs,
    Engineering,
    Education,
    Qa,
    Wiki,
    Other,
}

fn classify_host(host: &str) -> SourceClass {
    if host.contains("wikipedia.org") {
        SourceClass::Wiki
    } else if host.contains("stackoverflow.") {
        SourceClass::Qa
    } else if host.contains("cs.stanford.edu") || host.contains("ocw.mit.edu") {
        SourceClass::Education
    } else if host.contains("blog")
        || host.contains("jvns.ca")
        || host.contains("martinfowler.com")
        || host.contains("uber.com")
        || host.contains("netflixtechblog.com")
        || host.contains("cloudflare.com")
        || host.contains("research.google")
    {
        SourceClass::Engineering
    } else if host.contains("docs.") || host.contains("doc.") || host.contains("cppreference.com") {
        SourceClass::Docs
    } else {
        SourceClass::Other
    }
}

fn domain_cap(host: &str) -> usize {
    match host {
        "cppreference.com" | "en.cppreference.com" | "ch.cppreference.com" => 2500,
        "developer.mozilla.org" => 3000,
        "docs.python.org" => 2500,
        "doc.rust-lang.org" => 2500,
        "en.wikipedia.org" => 3000,
        "stackoverflow.com" => 2000,
        "stackoverflow.blog" => 1000,
        "blog.rust-lang.org" => 1000,
        "blog.cloudflare.com" => 1500,
        "jvns.ca" => 1000,
        "martinfowler.com" => 1200,
        "blog.acolyer.org" => 1200,
        "eng.uber.com" => 1000,
        "netflixtechblog.com" => 1000,
        "research.google" => 1000,
        _ => 500,
    }
}

fn host_allowed(host: &str) -> bool {
    let allow = [
        "doc.rust-lang.org",
        "docs.python.org",
        "developer.mozilla.org",
        "en.wikipedia.org",
        "stackoverflow.com",
        "stackoverflow.blog",
        "cppreference.com",
        "blog.rust-lang.org",
        "blog.cloudflare.com",
        "jvns.ca",
        "martinfowler.com",
        "blog.acolyer.org",
        "eng.uber.com",
        "netflixtechblog.com",
        "research.google",
    ];

    allow.contains(&host)
}

fn path_looks_binary(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    [
        ".tar", ".tar.gz", ".tgz", ".zip", ".7z", ".gz", ".xz", ".bz2", ".pdf", ".png",
        ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".mp4", ".mp3", ".woff", ".woff2", ".ttf",
    ]
    .iter()
    .any(|ext| p.ends_with(ext))
}

fn url_allowed(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };

    if !host_allowed(host) || path_looks_binary(url.path()) {
        return false;
    }

    let path = url.path();

    // Prevent noisy wiki editor/history endpoints.
    if host == "en.wikipedia.org" {
        if path.starts_with("/w/") {
            return false;
        }
        if !path.starts_with("/wiki/") {
            return false;
        }
        if path.starts_with("/wiki/Special:") || path.starts_with("/wiki/Talk:") {
            return false;
        }
    }

    // Keep SO focused on questions/answers/tags pages.
    if host == "stackoverflow.com" {
        let ok_path = path.starts_with("/questions")
            || path.starts_with("/q/")
            || path.starts_with("/a/")
            || path.starts_with("/tags");
        if !ok_path {
            return false;
        }
    }

    // Drop known low-signal query params.
    let blocked_params = [
        "action",
        "oldid",
        "diff",
        "lastactivity",
        "answertab",
        "tab",
        "printable",
        "veaction",
    ];

    for (k, v) in url.query_pairs() {
        let key = k.to_ascii_lowercase();
        let value = v.to_ascii_lowercase();

        if blocked_params.contains(&key.as_str()) {
            return false;
        }
        if key == "action" && (value == "edit" || value == "history") {
            return false;
        }
    }

    true
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn pop_next_batch(
    db: &rocksdb::DB,
    batch_size: usize,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let to_crawl_cf = storage::cf(db, storage::CF_TO_CRAWL)?;

    let mut out = Vec::new();
    let mut class_counts: HashMap<SourceClass, usize> = HashMap::new();
    let mut selected = Vec::new();

    for item in db.iterator_cf(to_crawl_cf, IteratorMode::Start) {
        if out.len() >= batch_size {
            break;
        }

        let (k, _) = item?;
        let url = String::from_utf8(k.to_vec())?;
        let parsed = match Url::parse(&url) {
            Ok(u) => u,
            Err(_) => {
                selected.push(url);
                continue;
            }
        };

        let host = parsed.host_str().unwrap_or("unknown");
        let class = classify_host(host);
        let class_limit = (batch_size / 4).max(1);
        let c = class_counts.get(&class).copied().unwrap_or(0);

        if c >= class_limit {
            continue;
        }

        class_counts.insert(class, c + 1);
        selected.push(url.clone());
        out.push(url);
    }

    for s in selected {
        db.delete_cf(to_crawl_cf, s.as_bytes())?;
    }

    Ok(out)
}

fn mark_seen(db: &rocksdb::DB, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let seen_cf = storage::cf(db, storage::CF_SEEN)?;
    db.put_cf(seen_cf, url.as_bytes(), [])?;
    Ok(())
}

fn is_seen(db: &rocksdb::DB, url: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let seen_cf = storage::cf(db, storage::CF_SEEN)?;
    Ok(db.get_cf(seen_cf, url.as_bytes())?.is_some())
}

fn enqueue_url(
    db: &rocksdb::DB,
    url: &str,
    per_domain_processed: &Arc<Mutex<HashMap<String, usize>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    if is_seen(db, url)? {
        return Ok(());
    }

    let parsed = match Url::parse(url) {
        Ok(u) => u,
        Err(_) => return Ok(()),
    };

    let Some(host) = parsed.host_str() else {
        return Ok(());
    };

    if !url_allowed(&parsed) {
        return Ok(());
    }

    let count = per_domain_processed
        .lock()
        .ok()
        .and_then(|m| m.get(host).copied())
        .unwrap_or(0);

    if count >= domain_cap(host) {
        return Ok(());
    }

    let to_crawl_cf = storage::cf(db, storage::CF_TO_CRAWL)?;
    if db.get_cf(to_crawl_cf, url.as_bytes())?.is_none() {
        db.put_cf(to_crawl_cf, url.as_bytes(), [])?;
    }
    Ok(())
}

fn obey_domain_rate_limit(
    db: &rocksdb::DB,
    domain: &str,
    rate_limit_ms: u64,
) -> Result<Option<Duration>, Box<dyn std::error::Error>> {
    let domains_cf = storage::cf(db, storage::CF_DOMAINS)?;
    let current = now_ms();

    if let Some(bytes) = db.get_cf(domains_cf, domain.as_bytes())? {
        let raw = String::from_utf8(bytes.to_vec())?;
        if let Ok(last) = raw.parse::<i64>() {
            let elapsed = current.saturating_sub(last) as u64;
            if elapsed < rate_limit_ms {
                return Ok(Some(Duration::from_millis(rate_limit_ms - elapsed)));
            }
        }
    }

    Ok(None)
}

fn update_domain_visit(db: &rocksdb::DB, domain: &str) -> Result<(), Box<dyn std::error::Error>> {
    let domains_cf = storage::cf(db, storage::CF_DOMAINS)?;
    db.put_cf(domains_cf, domain.as_bytes(), now_ms().to_string().as_bytes())?;
    Ok(())
}

fn add_sub_links_to_frontier(
    db: &rocksdb::DB,
    base_url: &Url,
    body: &str,
    per_domain_processed: &Arc<Mutex<HashMap<String, usize>>>,
) -> Result<usize, Box<dyn std::error::Error>> {
    let selector = Selector::parse("a[href]")?;
    let document = Html::parse_document(body);

    let mut added = 0;
    for element in document.select(&selector) {
        if added >= MAX_DISCOVERED_PER_PAGE {
            break;
        }
        if let Some(href) = element.value().attr("href") {
            if let Ok(absolute) = base_url.join(href) {
                if let Some(canon_url) = canon::canonicalize(absolute.as_str()) {
                    enqueue_url(db, canon_url.as_str(), per_domain_processed)?;
                    added += 1;
                }
            }
        }
    }

    Ok(added)
}

fn make_chunk_id(url: &str, pos: usize) -> String {
    let mut h = Sha256::new();
    h.update(url.as_bytes());
    h.update(b"#");
    h.update(pos.to_string().as_bytes());
    format!("{:x}", h.finalize())
}

fn store_page_and_chunks(
    db: &rocksdb::DB,
    cfg: &config::Config,
    page: search_engine::PageRecord,
) -> Result<usize, Box<dyn std::error::Error>> {
    let content_cf = storage::cf(db, storage::CF_CONTENT)?;
    let chunks_cf = storage::cf(db, storage::CF_CHUNKS)?;

    let mut wb = WriteBatch::default();
    wb.put_cf(content_cf, page.url.as_bytes(), serde_json::to_vec(&page)?);

    let mut chunk_count = 0usize;
    let mut preceding: Vec<String> = Vec::new();

    for block in &page.blocks {
        if chunk_count >= MAX_CHUNKS_PER_PAGE {
            break;
        }

        let sentences = sentencizer::split_sentences(&block.text);
        for sentence in sentences {
            if chunk_count >= MAX_CHUNKS_PER_PAGE {
                break;
            }

            let with_headings = context::with_context_depth(
                &block.heading_chain,
                &sentence,
                cfg.chunking.context_depth,
            );
            let (final_text, is_leaf) = chaining::apply_statement_chaining(&with_headings, &preceding);
            let chunk_id = make_chunk_id(&page.url, chunk_count);

            let chunk = Chunk {
                id: chunk_id.clone(),
                source_url: page.url.clone(),
                heading_chain: block.heading_chain.clone(),
                text: final_text.clone(),
                is_leaf,
            };

            wb.put_cf(chunks_cf, chunk_id.as_bytes(), serde_json::to_vec(&chunk)?);
            preceding.push(final_text);
            if preceding.len() > 3 {
                preceding.remove(0);
            }
            chunk_count += 1;
        }
    }

    db.write(wb)?;
    Ok(chunk_count)
}

struct CrawlOutcome {
    processed: bool,
    url: String,
    chunks: usize,
    discovered: usize,
    host: Option<String>,
}

async fn process_one(
    db: Arc<rocksdb::DB>,
    cfg: Arc<config::Config>,
    client: Client,
    raw_url: String,
    per_domain_processed: Arc<Mutex<HashMap<String, usize>>>,
    dns_ok_cache: Arc<Mutex<HashMap<String, bool>>>,
) -> CrawlOutcome {
    if is_seen(&db, &raw_url).unwrap_or(false) {
        return CrawlOutcome { processed: false, url: raw_url, chunks: 0, discovered: 0, host: None };
    }

    let Some(url) = canon::canonicalize(&raw_url) else {
        let _ = mark_seen(&db, &raw_url);
        return CrawlOutcome { processed: false, url: raw_url, chunks: 0, discovered: 0, host: None };
    };

    let Some(domain) = url.host_str() else {
        let _ = mark_seen(&db, url.as_str());
        return CrawlOutcome { processed: false, url: url.to_string(), chunks: 0, discovered: 0, host: None };
    };

    if !url_allowed(&url) {
        let _ = mark_seen(&db, url.as_str());
        return CrawlOutcome { processed: false, url: url.to_string(), chunks: 0, discovered: 0, host: None };
    }

    let current_count = per_domain_processed
        .lock()
        .ok()
        .and_then(|m| m.get(domain).copied())
        .unwrap_or(0);
    if current_count >= domain_cap(domain) {
        let _ = mark_seen(&db, url.as_str());
        return CrawlOutcome { processed: false, url: url.to_string(), chunks: 0, discovered: 0, host: None };
    }

    if let Ok(Some(wait)) = obey_domain_rate_limit(&db, domain, cfg.crawl.rate_limit_ms) {
        sleep(wait).await;
    }

    let dns_ok = if let Ok(cache) = dns_ok_cache.lock() {
        cache.get(domain).copied()
    } else {
        None
    };

    let dns_pass = match dns_ok {
        Some(v) => v,
        None => {
            let ok = dns::resolve_and_check(domain).await.is_ok();
            if let Ok(mut cache) = dns_ok_cache.lock() {
                cache.insert(domain.to_string(), ok);
            }
            ok
        }
    };

    if !dns_pass {
        let _ = mark_seen(&db, url.as_str());
        return CrawlOutcome { processed: false, url: url.to_string(), chunks: 0, discovered: 0, host: None };
    }

    let disallowed = robots::get_disallowed(domain, &db).await.unwrap_or_default();
    if !robots::is_allowed(url.path(), &disallowed) {
        let _ = mark_seen(&db, url.as_str());
        return CrawlOutcome { processed: false, url: url.to_string(), chunks: 0, discovered: 0, host: None };
    }

    let body = match client.get(url.as_str()).send().await {
        Ok(resp) if resp.status().is_success() => {
            let content_type = resp
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !content_type.contains("text/html") && !content_type.contains("application/xhtml+xml") {
                let _ = mark_seen(&db, url.as_str());
                let _ = update_domain_visit(&db, domain);
                return CrawlOutcome { processed: false, url: url.to_string(), chunks: 0, discovered: 0, host: None };
            }

            match resp.text().await {
                Ok(t) => t,
                Err(_) => {
                    let _ = mark_seen(&db, url.as_str());
                    let _ = update_domain_visit(&db, domain);
                    return CrawlOutcome { processed: false, url: url.to_string(), chunks: 0, discovered: 0, host: None };
                }
            }
        }
        _ => {
            let _ = mark_seen(&db, url.as_str());
            let _ = update_domain_visit(&db, domain);
            return CrawlOutcome { processed: false, url: url.to_string(), chunks: 0, discovered: 0, host: None };
        }
    };

    let page = normalizer::normalize(&body, url.as_str());
    let chunk_count = store_page_and_chunks(&db, &cfg, page).unwrap_or(0);
    let discovered = add_sub_links_to_frontier(&db, &url, &body, &per_domain_processed).unwrap_or(0);

    let _ = mark_seen(&db, url.as_str());
    let _ = update_domain_visit(&db, domain);

    CrawlOutcome {
        processed: true,
        url: url.to_string(),
        chunks: chunk_count,
        discovered,
        host: Some(domain.to_string()),
    }
}

fn load_existing_content_counts(
    db: &rocksdb::DB,
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Arc::new(config::load()?);
    let db = Arc::new(storage::open_db(&cfg.paths.db_path)?);

    let per_domain_counts = load_existing_content_counts(&db)?;
    let existing_pages: usize = per_domain_counts.values().sum();
    let per_domain_processed = Arc::new(Mutex::new(per_domain_counts));

    println!(
        "[crawl] start existing_pages={} target_max_pages={}",
        existing_pages, cfg.crawl.max_pages
    );

    if existing_pages >= cfg.crawl.max_pages {
        println!(
            "[crawl] target already reached ({} >= {}), nothing to do",
            existing_pages, cfg.crawl.max_pages
        );
        return Ok(());
    }

    for seed in SEED_URLS {
        if let Some(u) = canon::canonicalize(seed) {
            enqueue_url(&db, u.as_str(), &per_domain_processed)?;
        }
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent("search-engine-crawler/0.1")
        .build()?;

    let mut processed_this_run = 0usize;
    let dns_ok_cache: Arc<Mutex<HashMap<String, bool>>> = Arc::new(Mutex::new(HashMap::new()));

    while existing_pages + processed_this_run < cfg.crawl.max_pages {
        let remaining_global = cfg.crawl.max_pages - (existing_pages + processed_this_run);
        let batch_size = remaining_global.min(cfg.crawl.concurrency.max(1));
        let batch = pop_next_batch(&db, batch_size)?;

        if batch.is_empty() {
            println!("[crawl] frontier empty, stopping");
            break;
        }

        let mut stream = stream::iter(batch.into_iter().map(|raw_url| {
            let db = Arc::clone(&db);
            let cfg = Arc::clone(&cfg);
            let client = client.clone();
            let per_domain_processed = Arc::clone(&per_domain_processed);
            let dns_ok_cache = Arc::clone(&dns_ok_cache);
            async move { process_one(db, cfg, client, raw_url, per_domain_processed, dns_ok_cache).await }
        }))
        .buffer_unordered(cfg.crawl.concurrency.max(1));

        while let Some(outcome) = stream.next().await {
            if outcome.processed {
                processed_this_run += 1;

                if let Some(host) = outcome.host {
                    if let Ok(mut m) = per_domain_processed.lock() {
                        *m.entry(host).or_insert(0) += 1;
                    }
                }

                let total_processed = existing_pages + processed_this_run;
                println!(
                    "[crawl] processed_this_run={} total={} url={} chunks={} discovered_links={}",
                    processed_this_run, total_processed, outcome.url, outcome.chunks, outcome.discovered
                );

                if existing_pages + processed_this_run >= cfg.crawl.max_pages {
                    break;
                }
            }
        }

        let _ = db.flush_wal(true);
    }

    let _ = db.flush_wal(true);
    println!(
        "[crawl] done. processed_this_run={} total_now={}",
        processed_this_run,
        existing_pages + processed_this_run
    );
    println!("[crawl] persistent store path: {}", cfg.paths.db_path);
    Ok(())
}
