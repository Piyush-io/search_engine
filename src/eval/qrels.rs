use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};

use super::url_match::canonical_doc_key;

pub struct Qrel {
    pub query_id: String,
    pub doc_id: String,
    pub relevance: u32,
}

pub fn load_qrels(path: &str) -> Result<HashMap<String, Vec<Qrel>>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut map: HashMap<String, Vec<Qrel>> = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 4 {
            continue;
        }
        let query_id = parts[0].to_string();
        let doc_id = canonical_doc_key(parts[2]);
        let relevance: u32 = match parts[3].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        map.entry(query_id.clone()).or_default().push(Qrel {
            query_id,
            doc_id,
            relevance,
        });
    }

    Ok(map)
}
