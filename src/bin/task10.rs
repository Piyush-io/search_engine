// **** Task 10: Parse raw HTML and remove nodes corresponding to unecessary tags****
use futures::StreamExt;
use futures::stream::iter;
use kuchiki::traits::TendrilSink;
use reqwest::Client;

async fn process_url(client: reqwest::Client, url: &'static str) -> Result<String, reqwest::Error> {
    let html = client.get(url).send().await?.text().await?;
    let doc = kuchiki::parse_html().one(html);
    for node in doc.select("script, style, nav, header, footer").unwrap() {
        node.as_node().detach();
    }

    let mut output = Vec::new();
    doc.serialize(&mut output).unwrap();
    println!("Finished parsing html for {}", &url);
    Ok(String::from_utf8(output).unwrap())
}

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
    let results = iter(URLS)
        .map(|&url| {
            let client = client.clone();
            async move {
                let cleaned = process_url(client, url).await;
                (url, cleaned)
            }
        })
        .buffer_unordered(10);

    results
        .for_each(|(url, result)| async move {
            match result {
                Ok(..) => {
                    println!("Processed: {}", url);
                    // use cleaned html here
                }
                Err(err) => {
                    println!("Error processing {}: {}", url, err);
                }
            }
        })
        .await;
    Ok(())
}
