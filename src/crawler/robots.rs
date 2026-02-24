/// Fetch and parse robots.txt for `domain`, caching wildcard disallow rules in RocksDB.
///
/// CF expected: `robots` with key=domain and value=newline-joined disallow paths.
pub async fn get_disallowed(
    domain: &str,
    db: &rocksdb::DB,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let robots_cf = db
        .cf_handle("robots")
        .ok_or("missing 'robots' column family")?;

    if let Some(cached) = db.get_cf(robots_cf, domain.as_bytes())? {
        let s = String::from_utf8(cached)?;
        let rules = s
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        return Ok(rules);
    }

    let https_url = format!("https://{domain}/robots.txt");
    let http_url = format!("http://{domain}/robots.txt");

    let txt = match reqwest::get(&https_url).await {
        Ok(resp) if resp.status().is_success() => resp.text().await?,
        _ => {
            let fallback = reqwest::get(&http_url).await?;
            if fallback.status().is_success() {
                fallback.text().await?
            } else {
                String::new()
            }
        }
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

    db.put_cf(robots_cf, domain.as_bytes(), disallowed.join("\n").as_bytes())?;
    Ok(disallowed)
}

/// Return true if `path` is allowed to crawl given the disallow list.
pub fn is_allowed(path: &str, disallowed: &[String]) -> bool {
    !disallowed.iter().any(|rule| !rule.is_empty() && path.starts_with(rule))
}
