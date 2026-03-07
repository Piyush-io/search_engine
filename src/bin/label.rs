use std::io::{self, Write};

use rocksdb::IteratorMode;
use search_engine::{Chunk, config, storage};
use serde::Serialize;

#[derive(Serialize)]
struct LabelRecord {
    current_sentence: String,
    prev_1: Option<String>,
    prev_2: Option<String>,
    prev_3: Option<String>,
    label: u8,
}

fn prompt_label() -> Result<u8, Box<dyn std::error::Error>> {
    loop {
        print!("Label (0=none, 1=prev1, 2=prev2, 3=prev3, q=quit): ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let v = input.trim();

        if v.eq_ignore_ascii_case("q") {
            return Err("quit".into());
        }

        if let Ok(n) = v.parse::<u8>() {
            if n <= 3 {
                return Ok(n);
            }
        }

        println!("Invalid input. Enter 0/1/2/3 or q.");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = storage::open_db(&cfg.paths.db_path)?;
    let chunks_cf = storage::cf(&db, storage::CF_CHUNKS)?;

    let output_path = "training/labels.jsonl";
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(output_path)?;

    let mut prev: Vec<String> = Vec::new();
    let mut labeled = 0usize;

    for item in db.iterator_cf(chunks_cf, IteratorMode::Start) {
        let (_, value) = item?;
        let chunk: Chunk = serde_json::from_slice(&value)?;

        println!("\n========================================");
        println!("Current: {}", chunk.text);
        println!("Prev 1 : {}", prev.last().cloned().unwrap_or_default());
        println!(
            "Prev 2 : {}",
            prev.iter().rev().nth(1).cloned().unwrap_or_default()
        );
        println!(
            "Prev 3 : {}",
            prev.iter().rev().nth(2).cloned().unwrap_or_default()
        );

        let label = match prompt_label() {
            Ok(v) => v,
            Err(_) => break,
        };

        let rec = LabelRecord {
            current_sentence: chunk.text.clone(),
            prev_1: prev.last().cloned(),
            prev_2: prev.iter().rev().nth(1).cloned(),
            prev_3: prev.iter().rev().nth(2).cloned(),
            label,
        };

        let json = serde_json::to_string(&rec)?;
        writeln!(out, "{}", json)?;

        prev.push(chunk.text);
        if prev.len() > 3 {
            prev.remove(0);
        }

        labeled += 1;
        if labeled % 25 == 0 {
            println!("[label] saved {} examples -> {}", labeled, output_path);
        }
    }

    println!("[label] done. labeled={}", labeled);
    Ok(())
}
