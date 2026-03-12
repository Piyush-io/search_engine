use std::sync::mpsc;
use std::time::Instant;

use bytemuck::cast_slice;
use rocksdb::{DBRawIteratorWithThreadMode, IteratorMode, ReadOptions, WriteBatch, WriteOptions};
use search_engine::{Chunk, config, embeddings::bulk, storage};
use tracing::{debug, info};

const FLUSH_EVERY: usize = 20_000;
const STREAM_READAHEAD_BYTES: usize = 8 * 1024 * 1024;
/// Channel capacity in batches; keeps memory bounded while the pipeline stays full.
const CHANNEL_DEPTH: usize = 8;

// ── leaf detection ────────────────────────────────────────────────────────────

/// Check `is_leaf` by scanning raw JSON bytes — avoids full deserialization.
fn is_leaf_fast(value: &[u8]) -> bool {
    if let Some(pos) = value.windows(9).position(|w| w == b"\"is_leaf\"") {
        let rest = &value[pos + 9..];
        let trimmed = rest
            .iter()
            .skip_while(|&&b| b == b' ' || b == b':')
            .copied()
            .take(4)
            .collect::<Vec<_>>();
        trimmed == b"true"
    } else {
        false
    }
}

fn is_cf_empty(
    db: &rocksdb::DB,
    cf: rocksdb::ColumnFamilyRef<'_>,
) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(db.iterator_cf(cf, IteratorMode::Start).next().is_none())
}

fn raw_iterator_for_scan<'a>(
    db: &'a rocksdb::DB,
    cf: rocksdb::ColumnFamilyRef<'a>,
) -> DBRawIteratorWithThreadMode<'a, rocksdb::DB> {
    let mut read_opts = ReadOptions::default();
    read_opts.fill_cache(false);
    read_opts.set_readahead_size(STREAM_READAHEAD_BYTES);
    read_opts.set_auto_readahead_size(true);
    db.raw_iterator_cf_opt(&cf, read_opts)
}

// ── pipeline types ────────────────────────────────────────────────────────────

/// Whether we are in incremental mode (skip already-embedded keys).
/// Instead of loading all keys into a HashSet (uses hundreds of MB),
/// we rely on RocksDB's built-in Bloom filter via `key_may_exist_cf` +
/// a cheap point-read only when the Bloom filter says "maybe".
enum EmbedMode {
    Fresh,       // no existing embeddings — skip all checks
    Incremental, // use Bloom filter to skip already-embedded keys
}

struct WorkItem {
    ids: Vec<Vec<u8>>,
    texts: Vec<String>,
}

struct DoneItem {
    ids: Vec<Vec<u8>>,
    vectors: Vec<Vec<f32>>,
    embed_dur: std::time::Duration,
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = rlimit::increase_nofile_limit(10240);
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info,ort=warn,ort_sys=warn")
            }),
        )
        .with_target(false)
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .init();

    let t_start = Instant::now();

    let cfg = config::load()?;
    info!(
        backend     = %cfg.embedding.backend,
        model       = %cfg.embedding.model,
        dim         = cfg.embedding.dim,
        batch_size  = cfg.embedding.batch_size,
        max_length  = cfg.embedding.max_length.unwrap_or(256),
        bulk_workers = cfg.embedding.bulk_workers,
        bulk_intra_threads = cfg.embedding.bulk_intra_threads,
        "embedding config"
    );

    let t_db = Instant::now();
    let db = std::sync::Arc::new(storage::open_db_for_bulk_write(&cfg.paths.db_path)?);
    info!(elapsed_ms = t_db.elapsed().as_millis() as u64, "opened db");

    let embeddings_cf = storage::cf(&db, storage::CF_EMBEDDINGS)?;

    let embed_mode = {
        let empty = is_cf_empty(&db, embeddings_cf)?;
        if empty {
            info!("fast mode: no existing embeddings — will embed everything");
            EmbedMode::Fresh
        } else {
            info!("incremental mode: using Bloom filter to skip already-embedded keys");
            EmbedMode::Incremental
        }
    };

    // Warm fastembed cache (downloads model if missing) before workers start.
    // The OnceLock singleton in client.rs does this, but we also need it for
    // create_workers which reads files directly from the cache dir.
    {
        let t_warm = Instant::now();
        info!("ensuring model cache is warm…");
        let _ = search_engine::embeddings::client::configured_dim()?;
        info!(ms = t_warm.elapsed().as_millis() as u64, "cache warm");
    }

    // Build workers — each gets its own ORT session with intra_threads threads.
    let max_length = cfg.embedding.max_length.unwrap_or(256);
    let workers = bulk::create_workers(
        &cfg.embedding.model,
        &cfg.embedding.backend,
        max_length,
        cfg.embedding.dim,
        cfg.embedding.bulk_workers,
        cfg.embedding.bulk_intra_threads,
    )?;
    info!(workers = workers.len(), "workers created");

    // ── pipeline channels ────────────────────────────────────────────────────
    // reader  →  work_tx  →  worker threads
    // worker threads  →  done_tx  →  writer (main thread continues after spawn)
    let (work_tx, work_rx) = mpsc::sync_channel::<WorkItem>(CHANNEL_DEPTH);
    let (done_tx, done_rx) = mpsc::sync_channel::<DoneItem>(CHANNEL_DEPTH);

    // Spawn worker threads — one per BulkWorker.
    // Workers pull WorkItems from a shared queue, embed, and push DoneItems.
    // We use std::sync::Arc<Mutex<Receiver>> so all workers share the single
    // work_rx without crossbeam (which is not in the direct dep list).
    use std::sync::{Arc, Mutex};
    let shared_rx = Arc::new(Mutex::new(work_rx));

    let mut handles = Vec::new();
    for worker in workers {
        let rx = Arc::clone(&shared_rx);
        let tx = done_tx.clone();
        let handle = std::thread::spawn(move || {
            loop {
                let item = {
                    let guard = rx.lock().expect("work queue mutex poisoned");
                    guard.recv()
                };
                match item {
                    Err(_) => break, // channel closed — all work done
                    Ok(WorkItem { ids, texts }) => {
                        let t_embed = Instant::now();
                        match worker.embed_batch(&texts) {
                            Err(e) => {
                                // Log and skip bad batches rather than aborting.
                                tracing::error!(error = %e, batch = texts.len(), "embed_batch failed");
                            }
                            Ok(vectors) => {
                                let embed_dur = t_embed.elapsed();
                                let _ = tx.send(DoneItem {
                                    ids,
                                    vectors,
                                    embed_dur,
                                });
                            }
                        }
                    }
                }
            }
        });
        handles.push(handle);
    }
    // Drop the extra done_tx clone so done_rx closes when all workers exit.
    drop(done_tx);

    // ── reader + writer (main thread) ────────────────────────────────────────
    // Spawn the reader as a separate thread so the main thread can act as writer.
    let batch_size = cfg.embedding.batch_size;
    let reader_handle = {
        let db_path = cfg.paths.db_path.clone();
        let is_incremental = matches!(embed_mode, EmbedMode::Incremental);

        std::thread::spawn(move || -> Result<(usize, usize, usize, usize), String> {
            // Scan through a separate read-only handle so the long-lived chunk
            // iterator does not share writer-side file/version state.
            let reader_db =
                storage::open_db_read_only(&db_path).map_err(|e| format!("open reader db: {e}"))?;
            let chunks_cf = storage::cf(&reader_db, storage::CF_CHUNKS)
                .map_err(|e| format!("chunks cf: {e}"))?;
            let embeddings_cf_r = storage::cf(&reader_db, storage::CF_EMBEDDINGS)
                .map_err(|e| format!("embeddings cf: {e}"))?;

            let mut ids: Vec<Vec<u8>> = Vec::with_capacity(batch_size);
            let mut texts: Vec<String> = Vec::with_capacity(batch_size);
            let mut seen = 0usize;
            let mut skipped_existing = 0usize;
            let mut skipped_non_leaf = 0usize;
            let mut skipped_malformed = 0usize;

            let mut iter = raw_iterator_for_scan(&reader_db, chunks_cf);
            iter.seek_to_first();
            while iter.valid() {
                let Some(key) = iter.key() else { break };
                let Some(value) = iter.value() else { break };
                seen += 1;

                // Incremental mode: use RocksDB Bloom filter for a near-free existence check.
                // key_may_exist_cf is O(1) and avoids loading all keys into RAM.
                if is_incremental {
                    let may_exist = reader_db.key_may_exist_cf(embeddings_cf_r, key);
                    if may_exist {
                        // Bloom filter says "maybe" — confirm with a point read.
                        let exists = reader_db
                            .get_cf(embeddings_cf_r, key)
                            .map(|v| v.is_some())
                            .unwrap_or(false);
                        if exists {
                            skipped_existing += 1;
                            if skipped_existing % 50_000 == 0 {
                                tracing::info!(scanned = seen, skipped_existing, "scanning…");
                            }
                            iter.next();
                            continue;
                        }
                    }
                }

                if !is_leaf_fast(value) {
                    skipped_non_leaf += 1;
                    if seen % 100_000 == 0 {
                        tracing::info!(
                            scanned = seen,
                            skipped_non_leaf,
                            skipped_existing,
                            "scanning…"
                        );
                    }
                    iter.next();
                    continue;
                }

                let chunk: Chunk = match serde_json::from_slice(value) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(
                            key = %String::from_utf8_lossy(key),
                            error = %e,
                            "skipping malformed chunk"
                        );
                        skipped_malformed += 1;
                        iter.next();
                        continue;
                    }
                };

                ids.push(key.to_vec());
                texts.push(chunk.embed_text.unwrap_or(chunk.text));

                if texts.len() >= batch_size {
                    if work_tx
                        .send(WorkItem {
                            ids: std::mem::take(&mut ids),
                            texts: std::mem::take(&mut texts),
                        })
                        .is_err()
                    {
                        break; // writer died, abort
                    }
                }

                iter.next();
            }

            // Final partial batch.
            if !texts.is_empty() {
                let _ = work_tx.send(WorkItem { ids, texts });
            }

            Ok((seen, skipped_existing, skipped_non_leaf, skipped_malformed))
        })
    };

    // ── writer loop (main thread) ────────────────────────────────────────────
    let mut write_opts = WriteOptions::default();
    write_opts.disable_wal(true);

    let mut embedded = 0usize;
    let mut last_flush_at = 0usize;
    let mut total_embed_dur = std::time::Duration::ZERO;
    let mut total_write_dur = std::time::Duration::ZERO;
    let t_loop = Instant::now();

    while let Ok(DoneItem {
        ids,
        vectors,
        embed_dur,
    }) = done_rx.recv()
    {
        let n = ids.len();
        total_embed_dur += embed_dur;

        let t_write = Instant::now();
        let mut wb = WriteBatch::default();
        for (i, vec) in vectors.iter().enumerate() {
            wb.put_cf(embeddings_cf, &ids[i], cast_slice(vec.as_slice()));
        }
        db.write_opt(wb, &write_opts)?;
        let write_dur = t_write.elapsed();
        total_write_dur += write_dur;

        embedded += n;

        debug!(
            batch_size = n,
            embed_ms = embed_dur.as_millis() as u64,
            write_ms = write_dur.as_millis() as u64,
            "batch complete"
        );

        if embedded - last_flush_at >= FLUSH_EVERY {
            let t_flush = Instant::now();
            let _ = db.flush_cf(embeddings_cf);
            let flush_ms = t_flush.elapsed().as_millis() as u64;
            last_flush_at = embedded;

            let elapsed = t_loop.elapsed().as_secs_f64();
            let rate = if elapsed > 0.0 {
                embedded as f64 / elapsed
            } else {
                0.0
            };
            info!(
                embedded,
                rate_per_sec = format_args!("{rate:.0}"),
                flush_ms,
                "progress (flushed)"
            );
        } else if embedded % 2_000 == 0 {
            let elapsed = t_loop.elapsed().as_secs_f64();
            let rate = if elapsed > 0.0 {
                embedded as f64 / elapsed
            } else {
                0.0
            };
            info!(
                embedded,
                rate_per_sec = format_args!("{rate:.0}"),
                "progress"
            );
        }
    }

    // Wait for worker threads to finish.
    for handle in handles {
        let _ = handle.join();
    }

    let _ = db.flush_cf(embeddings_cf);
    let _ = db.flush_wal(true);

    // Collect reader stats.
    let (seen, skipped_existing, skipped_non_leaf, skipped_malformed) = reader_handle
        .join()
        .map_err(|_| "reader thread panicked")??;

    let wall = t_start.elapsed();
    let loop_dur = t_loop.elapsed();

    info!("─── embedding complete ───");
    info!(scanned = seen, "total chunks scanned");
    info!(embedded, "newly embedded");
    info!(skipped_existing, "skipped (already embedded)");
    info!(skipped_non_leaf, "skipped (non-leaf)");
    info!(skipped_malformed, "skipped (malformed)");
    info!(
        embed_secs = format_args!("{:.1}", total_embed_dur.as_secs_f64()),
        write_secs = format_args!("{:.1}", total_write_dur.as_secs_f64()),
        "time breakdown"
    );
    if loop_dur.as_secs_f64() > 0.0 {
        let rate = embedded as f64 / loop_dur.as_secs_f64();
        info!(
            rate_per_sec = format_args!("{rate:.0}"),
            "average throughput"
        );
    }
    info!(
        wall_secs = format_args!("{:.1}", wall.as_secs_f64()),
        "total wall time"
    );

    Ok(())
}
