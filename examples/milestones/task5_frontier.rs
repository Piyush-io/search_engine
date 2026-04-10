// **** Task 5: Naive In-Memory Frontier ****
// Goal: Understand what a "frontier" actually does
// This is BFS graph traversal, but for the web

use scraper::{Html, Selector};
use std::collections::{HashSet, VecDeque};
use url::Url;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    let mut seen: HashSet<String> = HashSet::new();
    let mut frontier: VecDeque<String> = seed_urls.into_iter().map(String::from).collect();

    while let Some(link) = frontier.pop_front() {
        if seen.contains(&link) {
            continue;
        }
        println!("Processing : {}", &link);
        seen.insert(link.clone());
        let body = match reqwest::get(&link).await {
            Ok(resp) if resp.status().is_success() => resp.text().await?,
            _ => continue,
        };
        let selector = Selector::parse("a[href]").unwrap();
        let document = Html::parse_document(&body);
        for element in document.select(&selector) {
            if let Some(href) = element.attr("href") {
                let base = match Url::parse(&link) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("Invalid base URL {}: {}", link, e);
                        continue;
                    }
                };

                match base.join(href) {
                    Ok(absolute) => {
                        let abs_str = absolute.to_string();
                        // Very useful filter - skip non-http(s) schemes
                        if abs_str.starts_with("http://") || abs_str.starts_with("https://") {
                            // Optional: avoid adding same URL again (though seen will catch it later)
                            if !seen.contains(&abs_str) {
                                frontier.push_back(abs_str);
                            }
                        }
                    }
                    Err(_e) => {}
                }
            }
        }
    }

    Ok(())
}
