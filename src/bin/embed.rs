use std::{sync::mpsc, time::Instant};

use bytemuck::cast_slice;
use rocksdb::{DBRawIteratorWithThreadMode, IteratorMode, ReadOptions, WriteBatch, WriteOptions};
use search_engine::{
    config,
    embeddings::{bulk, client},
    pipeline::IndexOperation,
    storage, Chunk,
};
use tracing::{debug, info};

const FLUSH_EVERY: usize = 20_000;
const STREAM_READAHEAD_BYTES: usize = 8 * 1024 * 1024;
const CHANNEL_DEPTH: usize = 8;

fn has_flag(flag: &str) -> bool {
    std::env::args().any(|arg| arg == flag)
}

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

enum EmbedMode {
    Fresh,
    Incremental,
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

fn run_embed_queue(
    cfg: &config::Config,
    db: &rocksdb::DB,
) -> Result<(), Box<dyn std::error::Error>> {
    let embed_queue_cf = storage::cf(db, storage::CF_EMBED_QUEUE)?;
    let chunks_cf = storage::cf(db, storage::CF_CHUNKS)?;
    let embeddings_cf = storage::cf(db, storage::CF_EMBEDDINGS)?;
    let vector_queue_cf = storage::cf(db, storage::CF_VECTOR_QUEUE)?;

    if db
        .iterator_cf(embed_queue_cf, IteratorMode::Start)
        .next()
        .is_none()
    {
        info!("embed queue empty; nothing to do");
        return Ok(());
    }

    let mut write_opts = WriteOptions::default();
    write_opts.disable_wal(true);

    let batch_size = cfg.embedding.batch_size.max(1);
    let mut embedded = 0usize;
    let mut skipped_existing = 0usize;
    let mut skipped_missing = 0usize;
    let mut skipped_non_leaf = 0usize;
    let mut skipped_malformed = 0usize;
    let started = Instant::now();

    loop {
        let mut queue_batch = Vec::new();
        for item in db
            .iterator_cf(embed_queue_cf, IteratorMode::Start)
            .take(batch_size)
        {
            let (key, _) = item?;
            queue_batch.push(key.to_vec());
        }

        if queue_batch.is_empty() {
            break;
        }

        let mut wb = WriteBatch::default();
        let mut ids = Vec::new();
        let mut texts = Vec::new();

        for key in &queue_batch {
            if db.get_cf(embeddings_cf, key)?.is_some() {
                wb.put_cf(vector_queue_cf, key, IndexOperation::Upsert.as_bytes());
                wb.delete_cf(embed_queue_cf, key);
                skipped_existing += 1;
                continue;
            }

            let Some(bytes) = db.get_cf(chunks_cf, key)? else {
                wb.delete_cf(embed_queue_cf, key);
                skipped_missing += 1;
                continue;
            };

            let chunk: Chunk = match serde_json::from_slice(&bytes) {
                Ok(chunk) => chunk,
                Err(_) => {
                    wb.delete_cf(embed_queue_cf, key);
                    skipped_malformed += 1;
                    continue;
                }
            };

            if !chunk.is_leaf {
                wb.delete_cf(embed_queue_cf, key);
                skipped_non_leaf += 1;
                continue;
            }

            ids.push(key.clone());
            texts.push(chunk.embed_text.unwrap_or(chunk.text));
        }

        if !texts.is_empty() {
            let vectors = client::embed_batch(&texts)?;
            for (i, vector) in vectors.iter().enumerate() {
                wb.put_cf(embeddings_cf, &ids[i], cast_slice(vector.as_slice()));
                wb.put_cf(vector_queue_cf, &ids[i], IndexOperation::Upsert.as_bytes());
                wb.delete_cf(embed_queue_cf, &ids[i]);
            }
            embedded += ids.len();
        }

        db.write_opt(wb, &write_opts)?;
        if embedded > 0 && embedded % FLUSH_EVERY == 0 {
            let _ = db.flush_cf(embeddings_cf);
        }
    }

    let _ = db.flush_cf(embeddings_cf);
    let elapsed = started.elapsed().as_secs_f64();
    let rate = if elapsed > 0.0 {
        embedded as f64 / elapsed
    } else {
        0.0
    };
    info!(
        embedded,
        skipped_existing,
        skipped_missing,
        skipped_non_leaf,
        skipped_malformed,
        rate_per_sec = format_args!("{rate:.0}"),
        "incremental embedding complete"
    );
    Ok(())
}

fn run_full_scan(
    cfg: &config::Config,
    db: std::sync::Arc<rocksdb::DB>,
) -> Result<(), Box<dyn std::error::Error>> {
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

    let t_warm = Instant::now();
    info!("ensuring model cache is warm…");
    let _ = search_engine::embeddings::client::configured_dim()?;
    info!(ms = t_warm.elapsed().as_millis() as u64, "cache warm");

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

    let (work_tx, work_rx) = mpsc::sync_channel::<WorkItem>(CHANNEL_DEPTH);
    let (done_tx, done_rx) = mpsc::sync_channel::<DoneItem>(CHANNEL_DEPTH);

    use std::sync::{Arc, Mutex};
    let shared_rx = Arc::new(Mutex::new(work_rx));

    let mut handles = Vec::new();
    for worker in workers {
        let rx = Arc::clone(&shared_rx);
        let tx = done_tx.clone();
        let handle = std::thread::spawn(move || loop {
            let item = {
                let guard = rx.lock().expect("work queue mutex poisoned");
                guard.recv()
            };
            match item {
                Err(_) => break,
                Ok(WorkItem { ids, texts }) => {
                    let t_embed = Instant::now();
                    match worker.embed_batch(&texts) {
                        Err(err) => {
                            tracing::error!(error = %err, batch = texts.len(), "embed_batch failed");
                        }
                        Ok(vectors) => {
                            let _ = tx.send(DoneItem {
                                ids,
                                vectors,
                                embed_dur: t_embed.elapsed(),
                            });
                        }
                    }
                }
            }
        });
        handles.push(handle);
    }
    drop(done_tx);

    let batch_size = cfg.embedding.batch_size;
    let db_path = cfg.paths.db_path.clone();
    let is_incremental = matches!(embed_mode, EmbedMode::Incremental);
    let reader_handle =
        std::thread::spawn(move || -> Result<(usize, usize, usize, usize), String> {
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

                if is_incremental {
                    let may_exist = reader_db.key_may_exist_cf(embeddings_cf_r, key);
                    if may_exist {
                        let exists = reader_db
                            .get_cf(embeddings_cf_r, key)
                            .map(|value| value.is_some())
                            .unwrap_or(false);
                        if exists {
                            skipped_existing += 1;
                            iter.next();
                            continue;
                        }
                    }
                }

                if !is_leaf_fast(value) {
                    skipped_non_leaf += 1;
                    iter.next();
                    continue;
                }

                let chunk: Chunk = match serde_json::from_slice(value) {
                    Ok(chunk) => chunk,
                    Err(_) => {
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
                        break;
                    }
                }

                iter.next();
            }

            if !texts.is_empty() {
                let _ = work_tx.send(WorkItem { ids, texts });
            }

            Ok((seen, skipped_existing, skipped_non_leaf, skipped_malformed))
        });

    let mut write_opts = WriteOptions::default();
    write_opts.disable_wal(true);

    let mut embedded = 0usize;
    let mut last_flush_at = 0usize;
    let mut total_embed_dur = std::time::Duration::ZERO;
    let mut total_write_dur = std::time::Duration::ZERO;
    let started = Instant::now();

    while let Ok(DoneItem {
        ids,
        vectors,
        embed_dur,
    }) = done_rx.recv()
    {
        total_embed_dur += embed_dur;

        let t_write = Instant::now();
        let mut wb = WriteBatch::default();
        for (i, vector) in vectors.iter().enumerate() {
            wb.put_cf(embeddings_cf, &ids[i], cast_slice(vector.as_slice()));
        }
        db.write_opt(wb, &write_opts)?;
        total_write_dur += t_write.elapsed();
        embedded += ids.len();

        debug!(batch_size = ids.len(), "full-scan embed batch complete");

        if embedded - last_flush_at >= FLUSH_EVERY {
            let _ = db.flush_cf(embeddings_cf);
            last_flush_at = embedded;
            let elapsed = started.elapsed().as_secs_f64();
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

    for handle in handles {
        let _ = handle.join();
    }

    let _ = db.flush_cf(embeddings_cf);
    let _ = db.flush_wal(true);

    let (seen, skipped_existing, skipped_non_leaf, skipped_malformed) = reader_handle
        .join()
        .map_err(|_| "reader thread panicked")??;

    let loop_dur = started.elapsed();
    info!("─── full-scan embedding complete ───");
    info!(
        scanned = seen,
        embedded, skipped_existing, skipped_non_leaf, skipped_malformed
    );
    info!(
        embed_secs = format_args!("{:.1}", total_embed_dur.as_secs_f64()),
        write_secs = format_args!("{:.1}", total_write_dur.as_secs_f64()),
        wall_secs = format_args!("{:.1}", loop_dur.as_secs_f64()),
        "time breakdown"
    );

    let embed_queue_cf = storage::cf(&db, storage::CF_EMBED_QUEUE)?;
    let vector_queue_cf = storage::cf(&db, storage::CF_VECTOR_QUEUE)?;
    let lexical_queue_cf = storage::cf(&db, storage::CF_LEXICAL_QUEUE)?;

    let mut queued = 0usize;
    let mut skipped_non_leaf = 0usize;

    loop {
        let mut batch_keys = Vec::new();
        for item in db
            .iterator_cf(&embed_queue_cf, IteratorMode::Start)
            .take(50_000)
        {
            let (key, _) = item?;
            batch_keys.push(key.to_vec());
        }

        if batch_keys.is_empty() {
            break;
        }

        let mut wb = WriteBatch::default();
        for key in &batch_keys {
            if db.get_cf(&embeddings_cf, key)?.is_some() {
                wb.put_cf(&vector_queue_cf, key, IndexOperation::Upsert.as_bytes());
                wb.delete_cf(&embed_queue_cf, key);
                queued += 1;
            } else {
                wb.delete_cf(&embed_queue_cf, key);
                wb.delete_cf(&lexical_queue_cf, key);
                skipped_non_leaf += 1;
            }
        }

        db.write_opt(wb, &write_opts)?;
    }

    info!(
        "[embed] drained embed_queue: queued={} skipped_non_leaf={}",
        queued, skipped_non_leaf
    );

    Ok(())
}

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

    let cfg = config::load()?;
    info!(
        backend = %cfg.embedding.backend,
        model = %cfg.embedding.model,
        dim = cfg.embedding.dim,
        batch_size = cfg.embedding.batch_size,
        max_length = cfg.embedding.max_length.unwrap_or(256),
        bulk_workers = cfg.embedding.bulk_workers,
        bulk_intra_threads = cfg.embedding.bulk_intra_threads,
        "embedding config"
    );

    if has_flag("--full-scan") {
        let db = std::sync::Arc::new(storage::open_db_for_bulk_write(&cfg.paths.db_path)?);
        return run_full_scan(&cfg, db);
    }

    let db = storage::open_db_for_bulk_write(&cfg.paths.db_path)?;
    run_embed_queue(&cfg, &db)
}
