// **** Task 6: Persistent Frontier with RocksDB ****
// Goal: Learn that crashes shouldn't lose state
// Store seen set and frontier queue in RocksDB for durability

use std::str::from_utf8;

use rocksdb::{ColumnFamilyDescriptor, DB, IteratorMode, Options};
use scraper::{Html, Selector};
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = "./crawl_data";
    let cf_ops = Options::default();
    let seen_links_descriptor = ColumnFamilyDescriptor::new("seen", cf_ops.clone());
    let to_crawl_descriptor = ColumnFamilyDescriptor::new("to_crawl", cf_ops.clone());
    let mut db_options = Options::default();
    db_options.create_if_missing(true);
    db_options.set_error_if_exists(false);
    db_options.create_missing_column_families(true);
    let frontier = DB::open_cf_descriptors(
        &db_options,
        path,
        vec![seen_links_descriptor, to_crawl_descriptor],
    )
    .unwrap();

    let seen_links_handle = frontier
        .cf_handle("seen")
        .expect("Column family 'seen' not found");
    let to_crawl_handle = frontier
        .cf_handle("to_crawl")
        .expect("Column family 'to_crawl' not found");

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
                    frontier.delete_cf(&to_crawl_handle, &key)?;
                    let body = match reqwest::get(from_utf8(&key)?).await {
                        Ok(resp) if resp.status().is_success() => resp.text().await?,
                        _ => continue,
                    };
                    let selector = Selector::parse("a[href]").unwrap();
                    let document = Html::parse_document(&body);
                    for element in document.select(&selector) {
                        if let Some(href) = element.attr("href") {
                            let base = match Url::parse(from_utf8(&key)?) {
                                Ok(b) => b,
                                Err(e) => {
                                    eprintln!("Invalid base URL {}: {}", from_utf8(&key)?, e);
                                    continue;
                                }
                            };

                            match base.join(href) {
                                Ok(absolute) => {
                                    let abs_str = absolute.to_string();
                                    // Very useful filter - skip non-http(s) schemes
                                    if abs_str.starts_with("http://")
                                        || abs_str.starts_with("https://")
                                    {
                                        // Optional: avoid adding same URL again (though seen will catch it later)
                                        match frontier
                                            .get_cf(&seen_links_handle, abs_str.as_bytes())
                                        {
                                            Ok(Some(..)) => {
                                                continue;
                                            }
                                            Ok(None) => frontier.put_cf(
                                                &to_crawl_handle,
                                                abs_str.as_bytes(),
                                                &[],
                                            )?,
                                            Err(..) => {
                                                eprintln!("error")
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    eprint!("error : {}", e);
                                }
                            }
                        }
                    }
                    frontier.put_cf(&seen_links_handle, &key, &[])?;
                }

                Err(e) => {
                    eprint!("Error : {}", e);
                }
            }
        }
    }

    Ok(())
}
