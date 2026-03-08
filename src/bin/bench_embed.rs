//! Embedding throughput benchmark.
//!
//! Samples up to `--samples N` (default 1000) leaf chunks from RocksDB,
//! then runs them through several (workers, intra_threads) topologies and
//! prints a throughput table.
//!
//! Usage:
//!   cargo run --release --bin bench_embed
//!   cargo run --release --bin bench_embed -- --samples 2000
//!
//! Nothing is written to the database; this is read-only.

use std::time::Instant;

use rocksdb::IteratorMode;
use search_engine::{Chunk, config, embeddings::bulk, storage};

const DEFAULT_SAMPLES: usize = 1_000;

/// Simple argument parsing — avoids adding a cli dep.
fn parse_args() -> usize {
    let args: Vec<String> = std::env::args().collect();
    let mut samples = DEFAULT_SAMPLES;
    let mut i = 1;
    while i < args.len() {
        if (args[i] == "--samples" || args[i] == "-n") && i + 1 < args.len() {
            samples = args[i + 1].parse().unwrap_or(DEFAULT_SAMPLES);
            i += 2;
        } else {
            i += 1;
        }
    }
    samples
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target_samples = parse_args();

    let cfg = config::load()?;
    let db = storage::open_db_read_only(&cfg.paths.db_path)?;
    let chunks_cf = storage::cf(&db, storage::CF_CHUNKS)?;

    // ── collect sample texts from leaf chunks ─────────────────────────────────
    let mut texts: Vec<String> = Vec::with_capacity(target_samples);
    for item in db.iterator_cf(chunks_cf, IteratorMode::Start) {
        let (_, value) = item?;
        // Fast leaf check (same as embed.rs)
        let is_leaf = value.windows(9).any(|w| w == b"\"is_leaf\"") && {
            let pos = value.windows(9).position(|w| w == b"\"is_leaf\"").unwrap();
            let rest = &value[pos + 9..];
            let t: Vec<u8> = rest
                .iter()
                .skip_while(|&&b| b == b' ' || b == b':')
                .copied()
                .take(4)
                .collect();
            t == b"true"
        };
        if !is_leaf {
            continue;
        }
        if let Ok(chunk) = serde_json::from_slice::<Chunk>(&value) {
            texts.push(chunk.text);
            if texts.len() >= target_samples {
                break;
            }
        }
    }

    if texts.is_empty() {
        eprintln!("No leaf chunks found in DB. Run the ingest pipeline first.");
        return Ok(());
    }

    println!(
        "\nEmbedding throughput benchmark — {} sample texts, batch_size={}\n",
        texts.len(),
        cfg.embedding.batch_size,
    );
    println!(
        "{:<14} {:<14} {:<14} {:<12}",
        "workers", "intra_threads", "total_threads", "emb/sec"
    );
    println!("{}", "-".repeat(56));

    let max_length = cfg.embedding.max_length.unwrap_or(256);

    // Topologies to test: (workers, intra_threads).  Total ORT threads = w * t.
    let topologies: &[(usize, usize)] = &[(1, 8), (2, 4), (4, 2), (1, 4), (2, 2)];

    for &(workers, intra_threads) in topologies {
        // Warmup + avoid cold-start noise by creating workers once.
        let worker_list = match bulk::create_workers(
            &cfg.embedding.model,
            &cfg.embedding.backend,
            max_length,
            cfg.embedding.dim,
            workers,
            intra_threads,
        ) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("  skipping ({workers},{intra_threads}): {e}");
                continue;
            }
        };

        // Distribute texts across workers in a simple round-robin.
        // We use rayon's scoped threads to run them all concurrently.
        let batch_size = cfg.embedding.batch_size;
        let t0 = Instant::now();
        let total_embedded = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let counts: Vec<usize> = {
            use std::sync::{Arc, Mutex};

            let texts_per_worker = texts.len() / workers;
            let mut worker_texts: Vec<Vec<String>> = vec![Vec::new(); workers];
            for (i, t) in texts.iter().enumerate() {
                worker_texts[i % workers].push(t.clone());
            }

            let results = Arc::new(Mutex::new(vec![0usize; workers]));
            let mut handles = Vec::new();

            for (idx, (worker, wtexts)) in worker_list.into_iter().zip(worker_texts).enumerate() {
                let res = Arc::clone(&results);
                let _ = texts_per_worker; // silence unused
                let h = std::thread::spawn(move || {
                    let mut embedded = 0usize;
                    for batch in wtexts.chunks(batch_size) {
                        match worker.embed_batch(&batch.to_vec()) {
                            Ok(vecs) => embedded += vecs.len(),
                            Err(e) => eprintln!("  worker {idx} error: {e}"),
                        }
                    }
                    res.lock().unwrap()[idx] = embedded;
                });
                handles.push(h);
            }

            for h in handles {
                let _ = h.join();
            }

            Arc::try_unwrap(results).unwrap().into_inner().unwrap()
        };

        let _ = total_embedded; // suppress unused warning
        let total: usize = counts.iter().sum();
        let elapsed = t0.elapsed().as_secs_f64();
        let rate = if elapsed > 0.0 {
            total as f64 / elapsed
        } else {
            0.0
        };

        println!(
            "{:<14} {:<14} {:<14} {:<12.0}",
            workers,
            intra_threads,
            workers * intra_threads,
            rate,
        );
    }

    println!();
    println!(
        "Current config: workers={} intra_threads={}",
        cfg.embedding.bulk_workers, cfg.embedding.bulk_intra_threads,
    );

    Ok(())
}
