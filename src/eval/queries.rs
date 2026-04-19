use std::io::BufRead;

pub fn load_queries(path: &str) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut queries = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() < 2 {
            continue;
        }

        queries.push((parts[0].to_string(), parts[1].to_string()));
    }

    Ok(queries)
}
