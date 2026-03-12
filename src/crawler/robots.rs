use dashmap::DashMap;
use std::sync::Arc;

/// In-memory + RocksDB cache for robots.txt disallow rules.
/// The DashMap avoids hitting RocksDB on every call for the same domain.
pub type RobotsCache = Arc<DashMap<String, Vec<String>>>;

pub fn new_cache() -> RobotsCache {
    Arc::new(DashMap::new())
}

pub fn get_cached_disallowed(
    domain: &str,
    db: &rocksdb::DB,
    cache: &RobotsCache,
) -> Result<Option<Vec<String>>, Box<dyn std::error::Error>> {
    if let Some(entry) = cache.get(domain) {
        return Ok(Some(entry.clone()));
    }

    let robots_cf = db
        .cf_handle("robots")
        .ok_or("missing 'robots' column family")?;

    let Some(cached) = db.get_cf(robots_cf, domain.as_bytes())? else {
        return Ok(None);
    };

    let s = String::from_utf8(cached)?;
    let rules: Vec<String> = s
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    cache.insert(domain.to_string(), rules.clone());
    Ok(Some(rules))
}

/// Fetch and parse robots.txt for `domain`, with a two-tier cache:
/// 1. In-memory DashMap (fast path, no DB read)
/// 2. RocksDB `robots` CF (persistent across runs)
/// 3. HTTP fetch (only on cold miss)
///
/// Uses the caller-provided `client` for connection reuse and shared timeouts.
pub async fn get_disallowed(
    domain: &str,
    db: &rocksdb::DB,
    client: &reqwest::Client,
    cache: &RobotsCache,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // Fast path: in-memory cache.
    if let Some(entry) = cache.get(domain) {
        return Ok(entry.clone());
    }

    // Second tier: RocksDB persistent cache.
    if let Some(rules) = get_cached_disallowed(domain, db, cache)? {
        return Ok(rules);
    }

    let robots_cf = db
        .cf_handle("robots")
        .ok_or("missing 'robots' column family")?;

    // Cold miss: fetch via shared client.
    let https_url = format!("https://{domain}/robots.txt");
    let http_url = format!("http://{domain}/robots.txt");

    let txt = match client.get(&https_url).send().await {
        Ok(resp) if resp.status().is_success() => resp.text().await?,
        _ => match client.get(&http_url).send().await {
            Ok(fallback) if fallback.status().is_success() => fallback.text().await?,
            _ => String::new(),
        },
    };

    let mut disallowed = Vec::new();
    let mut in_wildcard = false;

    for line in txt.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut parts = line.splitn(2, ':');
        let key = parts.next().unwrap_or("").trim().to_ascii_lowercase();
        let value = parts.next().unwrap_or("").trim();

        match key.as_str() {
            "user-agent" => in_wildcard = value == "*",
            "disallow" if in_wildcard && !value.is_empty() => disallowed.push(value.to_string()),
            _ => {}
        }
    }

    db.put_cf(
        robots_cf,
        domain.as_bytes(),
        disallowed.join("\n").as_bytes(),
    )?;
    cache.insert(domain.to_string(), disallowed.clone());
    Ok(disallowed)
}

/// Return true if `path` is allowed to crawl given the disallow list.
pub fn is_allowed(path: &str, disallowed: &[String]) -> bool {
    !disallowed
        .iter()
        .any(|rule| !rule.is_empty() && path.starts_with(rule))
}
