// **** Task 9: Parse Raw HTML****

use reqwest::Client;
use scraper::{self, Html};
use std::sync::Arc;
use tokio::sync::Semaphore;

pub const URLS: &[&str] = &[
    // News & Media Sites (10 URLs)
    "https://news.ycombinator.com/",
    "https://www.bbc.com/",
    "https://www.cnn.com/",
    "https://www.theguardian.com/",
    "https://www.nytimes.com/",
    "https://www.reddit.com/",
    "https://www.techcrunch.com/",
    "https://www.arstechnica.com/",
    "https://www.wired.com/",
    "https://medium.com/",
    // E-Commerce Sites (10 URLs)
    "https://books.toscrape.com/",
    "https://webscraper.io/test-sites/e-commerce",
    "https://webscraper.io/test-sites/e-commerce/allinone",
    "https://webscraper.io/test-sites/e-commerce/pagination",
    "https://webscraper.io/test-sites/e-commerce/ajax",
    "https://www.scrapingcourse.com/ecommerce",
    "https://www.amazon.com/",
    "https://www.ebay.com/",
    "https://www.aliexpress.com/",
    "https://www.etsy.com/",
    // Forums & Community (8 URLs)
    "https://stackoverflow.com/",
    "https://www.reddit.com/r/programming/",
    "https://www.reddit.com/r/learnprogramming/",
    "https://dev.to/",
    "https://news.ycombinator.com/newest",
    "https://forum.rust-lang.org/",
    "https://www.quora.com/",
    "https://www.discourse.org/",
    // Blogs & Content Sites (10 URLs)
    "https://www.freecodecamp.org/",
    "https://blog.rust-lang.org/",
    "https://tokio.rs/",
    "https://doc.rust-lang.org/",
    "https://www.digitalocean.com/community/tutorials",
    "https://www.smashingmagazine.com/",
    "https://alistapart.com/",
    "https://css-tricks.com/",
    "https://www.webdesignerdepot.com/",
    "https://www.designernews.co/",
    // Documentation & Reference (8 URLs)
    "https://docs.rs/scraper/",
    "https://developer.mozilla.org/en-US/docs/Web/HTML",
    "https://www.w3schools.com/",
    "https://www.w3.org/",
    "https://html.spec.whatwg.org/",
    "https://crates.io/",
    "https://docs.rs/",
    "https://github.com/",
    // Special Test Sites (4 URLs)
    "https://toscrape.com/",
    "https://testpages.herokuapp.com/pages/",
    "https://www.scrapingcourse.com/",
    "https://proxyway.com/guides/best-websites-to-practice-your-web-scraping-skills",
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let semaphore = Arc::new(Semaphore::new(10));
    let mut handles = Vec::new();
    for (i, &link) in URLS.iter().enumerate() {
        let req_client = client.clone();
        let sem = semaphore.clone();
        println!("Processing {} : {}", i, &link);
        let req_handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("Waiting for the semaphore");
            let html = req_client.get(link).send().await?.text().await?;
            Html::parse_document(&html);
            if i % 10 == 0 {
                println!(
                    "Finished parsing html for {}, its content is : {}",
                    &i, html
                );
            }
            println!("Finished parsing html for {}", &i);
            Ok::<_, reqwest::Error>((i, link))
        });
        handles.push(req_handle);
    }

    for handle in handles {
        match handle.await {
            Ok(Ok((index, url))) => {
                println!("Link {}: {}", index + 1, url);
                println!("--------------------------");
            }
            Ok(Err(err)) => {
                eprintln!("Error fetching link: {}", err);
            }
            Err(err) => {
                panic!("Task Panicked: {}", err);
            }
        };
    }
    Ok(())
}
