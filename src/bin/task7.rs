
// **** Task 7: Persistent Frontier with RocksDB with rate limiting per domain**** Goal : Add rate limiting per domain Extract domain from the URL, maintain per domain queues and wait 1s for link for same domain
use rocksdb::{ColumnFamilyDescriptor, ColumnFamilyRef, DB, IteratorMode, Options};
use scraper::{Html, Selector};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::u64::MAX;
use std::{str::from_utf8, thread::sleep};
use url::Url;

fn add_sub_links_to_frontier(
    frontier: &DB,
    url: &Url,
    body: &str,
    seen: ColumnFamilyRef,
    to_crawl: ColumnFamilyRef,
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
                        if !frontier.get_cf(seen, &abs_str)?.is_some()
                            && !frontier.get_cf(to_crawl, &abs_str)?.is_some()
                        {
                            frontier.put_cf(to_crawl, &abs_str, &[])?;
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

// struct Frontier();

// impl Frontier {
//     pub fn new() -> DB {
//         DB::open_cf_descriptors()
//     }
// }

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = "./crawl_data";
    let cf_ops = Options::default();
    let seen_links_descriptor = ColumnFamilyDescriptor::new("seen", cf_ops.clone());
    let to_crawl_descriptor = ColumnFamilyDescriptor::new("to_crawl", cf_ops.clone());
    let encountered_domains = ColumnFamilyDescriptor::new("domains", cf_ops.clone());
    let mut db_options = Options::default();
    db_options.create_if_missing(true);
    db_options.set_error_if_exists(false);
    db_options.create_missing_column_families(true);
    let frontier = DB::open_cf_descriptors(
        &db_options,
        path,
        vec![
            seen_links_descriptor,
            to_crawl_descriptor,
            encountered_domains,
        ],
    )
    .unwrap();

    let seen_links_handle = frontier
        .cf_handle("seen")
        .expect("Column family 'seen' not found");
    let to_crawl_handle = frontier
        .cf_handle("to_crawl")
        .expect("Column family 'to_crawl' not found");
    let domain_handle = frontier
        .cf_handle("domains")
        .expect("Column family 'domains' not found");
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

    for url in &seed_urls {
        // dont use is_ok() as confirmation as it will be true for both None and for some value
        match frontier.get_cf(seen_links_handle, &url.as_bytes()) {
            Ok(Some(..)) => {
                println!("Seen : {}", &url);
                continue;
            }
            Ok(None) => {}
            Err(e) => {
                eprint!("Error : {}", e)
            }
        }
        match frontier.put_cf(to_crawl_handle, &url.as_bytes(), &[]) {
            Ok(_val) => {}
            Err(e) => {
                eprintln!("{}", e)
            }
        }
    }

    loop {
        let snapshot = frontier.snapshot();
        if snapshot
            .iterator_cf(&to_crawl_handle, IteratorMode::Start)
            .next()
            .is_none()
        {
            println!("Processed all links. Hurray!");
            break;
        }
        for value in snapshot.iterator_cf(&to_crawl_handle, IteratorMode::Start) {
            match value {
                Ok((key, _val)) => {
                    match snapshot.get_cf(&seen_links_handle, &key) {
                        Ok(Some(..)) => {
                            println!("Seen : {}", from_utf8(&key)?);
                        }
                        Ok(None) => {}
                        Err(..) => {
                            eprintln!("error")
                        }
                    }
                    println!("Processing : {}", from_utf8(&key)?);
                    let url = Url::parse(from_utf8(&key)?)?;
                    let domain = url.host_str().unwrap_or("unknown");
                    let mut last_fetch: u64 = MAX;
                    match frontier.get_cf(&domain_handle, domain.as_bytes()) {
                        Ok(Some(x)) => {
                            last_fetch = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .expect("Time went backwards")
                                .as_secs()
                                - u64::from_be_bytes(x.try_into().unwrap());
                        }
                        Ok(None) => {
                            let current_time = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .expect("backward")
                                .as_secs();
                            frontier.put_cf(&domain_handle, &domain, current_time.to_be_bytes())?;
                        }
                        Err(e) => {
                            eprintln!("Error: {}", e);
                        }
                    }
                    if last_fetch < 1 {
                        println!("Timeout for : {}", &domain);
                        sleep(Duration::from_secs(1));
                    }
                    frontier.put_cf(
                        &domain_handle,
                        &domain,
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .expect("backward")
                            .as_secs()
                            .to_be_bytes(),
                    )?;
                    let body = match reqwest::get(url.as_str()).await {
                        Ok(resp) if resp.status().is_success() => resp.text().await?,
                        _ => {
                            println!("Error processing: {}", from_utf8(&key)?);
                            continue;
                        }
                    };

                    add_sub_links_to_frontier(
                        &frontier,
                        &url,
                        &body,
                        &seen_links_handle,
                        &to_crawl_handle,
                    )?;
                    frontier.put_cf(&seen_links_handle, &key, &[])?;
                    frontier.delete_cf(&to_crawl_handle, &key)?;
                }

                Err(e) => {
                    eprint!("Error : {}", e);
                }
            }
        }
    }

    Ok(())
}
