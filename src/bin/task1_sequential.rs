// **** Task 1: Sequential Web Fetcher ****
// Goal: Understand the pain of sequential I/O

use std::time;

fn main() -> Result<(), ureq::Error> {
    let start_time = time::Instant::now();
    let links: [&str; 20] = [
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
    for (i, &link) in links.iter().enumerate() {
        println!("Link {}: {}", i + 1, link);
        println!("--------------------------");
        let body = ureq::get(link).call()?.body_mut().read_to_string()?;
        println!("{}", body);
        println!("--------------------------");
    }
    let elapsed_time = start_time.elapsed();
    println!("Elapsed time: {:?}", elapsed_time);
    Ok(())
}
