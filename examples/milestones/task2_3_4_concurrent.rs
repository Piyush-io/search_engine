// **** Tasks 2, 3, 4: Concurrent Web Fetcher ****
// Task 2: Concurrent fetching with tokio::spawn
// Task 3: Backpressure control with Semaphore
// Task 4: Graceful failure handling (timeout, 404, connection refused)

use std::sync::Arc;
use std::time;
use tokio::sync::Semaphore;

const LINKS: [&str; 20] = [
    "https://example.com",
    "https://httpbin.org/get",
    "https://jsonplaceholder.typicode.com/posts",
    "https://jsonplaceholder.typicode.com/posts/1",
    "https://jsonplaceholder.typicode.com/users",
    "https://api.github.com",
    "https://api.github.com/repos/rust-lang/rust",
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
];

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let start_time = time::Instant::now();
    let client: reqwest::Client = reqwest::Client::new();
    let semaphore = Arc::new(Semaphore::new(50));
    let mut handles = Vec::new();
    for (i, &link) in LINKS.iter().enumerate() {
        let sem = Arc::clone(&semaphore);
        let client_handle = client.clone();
        println!("Link {}: {}", i + 1, link);
        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("Waiting for semaphore");
            client_handle
                .get(link)
                .timeout(time::Duration::new(5, 0))
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            Ok::<_, reqwest::Error>((i, link))
        });
        handles.push(handle);
        println!("--------------------------");
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

    let elapsed_time = start_time.elapsed();
    println!("Elapsed time: {:?}", elapsed_time);

    Ok(())
}
