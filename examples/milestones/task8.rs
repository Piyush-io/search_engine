// **** Task 7: Persistent Frontier with RocksDB with rate limiting per domain**** Goal : Add rate limiting per domain Extract domain from the URL, maintain per domain queues and wait 1s for link for same domain
mod frontier;

use frontier::Frontier;
use rocksdb::IteratorMode;
use scraper::{Html, Selector};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::u64::MAX;
use std::{str::from_utf8, thread::sleep};
use url::Url;

fn allowed_to_crawl(url: &Url, not_allowed: &Vec<String>) -> bool {
    let path = Url::path(&url);
    for rule in not_allowed {
        if path.starts_with(rule) {
            return false;
        }
    }
    return true;
}

async fn build_robots_column(
    frontier: &Frontier,
    domain: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let https_url = format!("https://{}/robots.txt", domain);
    let http_url = format!("http://{}/robots.txt", domain);

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
        let key = parts.next().unwrap().trim().to_lowercase();
        let value = parts.next().unwrap_or("").trim();

        match key.as_str() {
            "user-agent" => in_wildcard = value == "*",
            "disallow" if in_wildcard && !value.is_empty() => disallowed.push(value.to_string()),
            _ => {}
        }
    }

    let blob = disallowed.join("\n");
    frontier.add_to_robots(domain, &blob)?;
    return Ok(blob);
}

fn add_sub_links_to_frontier(
    frontier: &Frontier,
    url: &Url,
    body: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let selector = Selector::parse("a[href]").unwrap();
    let document = Html::parse_document(&body);
    for element in document.select(&selector) {
        if let Some(href) = element.attr("href") {
            let base = match Url::parse(url.as_str()) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("Invalid base URL {}: {}", url, e);
                    continue;
                }
            };

            match base.join(href) {
                Ok(absolute) => {
                    let abs_str = absolute.to_string();
                    // Very useful filter - skip non-http(s) schemes
                    if abs_str.starts_with("http://") || abs_str.starts_with("https://") {
                        if !frontier.is_seen(&abs_str)?
                            && !frontier.get_from_crawl(&abs_str)?.is_some()
                        {
                            frontier.add_to_crawl(&abs_str)?;
                        }
                    }
                }
                Err(e) => {
                    eprint!("error : {}", e);
                }
            }
        }
    }
    Ok(())
}

fn add_seed_urls(
    frontier: &Frontier,
    seed_urls: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    for url in seed_urls {
        // dont use is_ok() as confirmation as it will be true for both None and for some value
        if frontier.is_seen(url)? == true {
            continue;
        }
        frontier.add_to_crawl(url)?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // DB definition
    let frontier =
        Frontier::new().expect("Error creating rocksdb persistent Frontier using Frotier crate");

    // Defining the seed URLs
    let seed_urls = vec![
        "https://raw.githubusercontent.com/rust-lang/rust/master/README.md",
        "https://dog.ceo/api/breeds/list/all",
        "https://catfact.ninja/fact",
        "https://www.testing.com/",
        "https://www.timeanddate.com/worldclock/timezone/utc",
        "https://api.agify.io/?name=michael",
        "https://api.genderize.io/?name=alex",
        "https://api.nationalize.io/?name=arjun",
        "https://pokeapi.co/api/v2/pokemon/ditto",
        "https://api.spacexdata.com/v4/launches/latest",
        "https://api.open-meteo.com/v1/forecast?latitude=28.6&longitude=77.2&current_weather=true",
        "https://www.rust-lang.org",
        "https://bruh.xyz/", // Test request timeouts (2-second delay)
        "https://example.com",
        "https://httpbin.org/get",
        "https://jsonplaceholder.typicode.com/posts",
        "https://jsonplaceholder.typicode.com/posts/1",
        "https://jsonplaceholder.typicode.com/users",
    ];

    match add_seed_urls(&frontier, &seed_urls) {
        Ok(..) => {}
        Err(..) => eprintln!("Error adding seed links"),
    }

    loop {
        let snapshot = match frontier.get_snapshot()? {
            Some(x) => x,
            None => break,
        };

        for value in snapshot.iterator_cf(frontier.to_crawl_handle(), IteratorMode::Start) {
            match value {
                Ok((key, _val)) => {
                    let key_as_str = from_utf8(&key)?;

                    if frontier.is_seen(key_as_str)? {
                        println!("Seen : {}", key_as_str);
                        continue;
                    }

                    println!("Processing : {}", key_as_str);
                    let url = Url::parse(key_as_str)?;
                    let domain = url.host_str().unwrap_or("unknown");
                    let mut not_allowed: Vec<String> = Vec::new();
                    match frontier.get_from_robots(domain) {
                        Ok(Some(x)) => {
                            not_allowed = x.lines().map(String::from).collect();
                        }
                        Ok(None) => match build_robots_column(&frontier, domain).await {
                            Ok(x) => {
                                not_allowed = x.lines().map(String::from).collect();
                            }

                            Err(..) => {
                                eprintln!("error parsing robots.txt for {}, so skipping it", &url);
                                frontier.mark_seen(&key_as_str)?;
                                frontier.delete_from_crawl_cf(&key_as_str)?;
                                continue;
                            }
                        },
                        Err(e) => {
                            eprintln!("Error : {}", e)
                        }
                    }

                    let mut last_fetch: u64 = MAX;
                    match frontier.get_last_visit(domain)? {
                        Some(last_visit_time) => {
                            last_fetch = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .expect("Time went backwards")
                                .as_secs()
                                - last_visit_time;
                        }
                        None => {
                            let current_time = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .expect("backward")
                                .as_secs();
                            frontier.add_to_domain(&domain, current_time.to_be_bytes())?;
                        }
                    }

                    if last_fetch < 1 {
                        println!("Timeout for : {}", &domain);
                        sleep(Duration::from_secs(1));
                    }

                    frontier.add_to_domain(
                        &domain,
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .expect("backward")
                            .as_secs()
                            .to_be_bytes(),
                    )?;

                    if !allowed_to_crawl(&url, &not_allowed) {
                        println!("Not allowed to crawl : {}", url);
                        frontier.mark_seen(&key_as_str)?;
                        frontier.delete_from_crawl_cf(&key_as_str)?;
                        continue;
                    }
                    let body = match reqwest::get(url.as_str()).await {
                        Ok(resp) if resp.status().is_success() => resp.text().await?,
                        _ => {
                            println!("Error processing: {}", key_as_str);
                            continue;
                        }
                    };

                    add_sub_links_to_frontier(&frontier, &url, &body)?;
                    frontier.mark_seen(&key_as_str)?;
                    frontier.delete_from_crawl_cf(&key_as_str)?;
                }

                Err(e) => {
                    eprint!("Error : {}", e);
                }
            }
        }
    }

    Ok(())
}
