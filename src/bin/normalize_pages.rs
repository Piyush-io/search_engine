use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use rocksdb::{IteratorMode, WriteBatch};
use search_engine::{
    chunking::{chaining, context, sentencizer},
    config, storage, Chunk, PageRecord,
};
use sha2::{Digest, Sha256};
use url::Url;

const MAX_CHUNKS_PER_PAGE: usize = 220;

fn make_chunk_id(url: &str, pos: usize) -> String {
    let mut h = Sha256::new();
    h.update(url.as_bytes());
    h.update(b"#");
    h.update(pos.to_string().as_bytes());
    format!("{:x}", h.finalize())
}

fn build_chunks(cfg: &config::Config, page: &PageRecord) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut preceding_sentences: Vec<String> = Vec::new();

    // In a real V2 we might want to use the per-host chunk limit from policy.rs
    // For now we use the default.
    let chunk_limit = MAX_CHUNKS_PER_PAGE;

    let window_size = cfg.chunking.window_size;
    let window_overlap = cfg.chunking.window_overlap;

    for block in &page.blocks {
        if chunks.len() >= chunk_limit {
            break;
        }

        let sentences = sentencizer::split_sentences(&block.text);
        let windows = sentencizer::merge_windows(&sentences, window_size, window_overlap);

        for window_text in windows {
            if chunks.len() >= chunk_limit {
                break;
            }

            let (chained_text, is_leaf) =
                chaining::apply_statement_chaining(&window_text, &preceding_sentences);
            let display_text = context::with_context_depth(
                &block.heading_chain,
                &chained_text,
                cfg.chunking.context_depth,
            );
            let embed_text = context::with_embed_context(
                Some(&page.title),
                &block.heading_chain,
                &chained_text,
                cfg.chunking.context_depth,
            );
            let chunk_id = make_chunk_id(&page.url, chunks.len());

            chunks.push(Chunk {
                id: chunk_id,
                source_url: page.url.clone(),
                heading_chain: block.heading_chain.clone(),
                text: display_text,
                embed_text: Some(embed_text),
                page_title: Some(page.title.clone()),
                is_leaf,
            });

            preceding_sentences.push(window_text);
            if preceding_sentences.len() > 4 {
                preceding_sentences.remove(0);
            }
        }
    }

    chunks
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Arc::new(config::load()?);
    let db = Arc::new(storage::open_db_with_cache(
        &cfg.paths.db_path,
        cfg.rocksdb.block_cache_mb,
    )?);

    let norm_queue_cf = storage::cf(&db, storage::CF_NORMALIZE_QUEUE)?;
    let content_cf = storage::cf(&db, storage::CF_CONTENT)?;
    let chunks_cf = storage::cf(&db, storage::CF_CHUNKS)?;

    let count = Arc::new(AtomicUsize::new(0));
    let mut last_report = Instant::now();

    println!("[normalize] starting page normalization...");

    loop {
        let mut batch = Vec::new();
        for item in db.iterator_cf(norm_queue_cf, IteratorMode::Start).take(100) {
            let (key, _) = item?;
            batch.push(key.to_vec());
        }

        if batch.is_empty() {
            println!("[normalize] queue empty, finished.");
            return Ok(());
        }

        let mut wb = WriteBatch::default();
        let mut processed_in_batch = 0;

        for url_bytes in &batch {
            let Some(page_data) = db.get_cf(content_cf, url_bytes)? else {
                wb.delete_cf(norm_queue_cf, url_bytes);
                continue;
            };

            let Ok(page) = serde_json::from_slice::<PageRecord>(&page_data) else {
                wb.delete_cf(norm_queue_cf, url_bytes);
                continue;
            };

            let chunks = build_chunks(&cfg, &page);
            for chunk in chunks {
                wb.put_cf(chunks_cf, chunk.id.as_bytes(), serde_json::to_vec(&chunk)?);
            }

            wb.delete_cf(norm_queue_cf, url_bytes);
            processed_in_batch += 1;
        }

        db.write(wb)?;
        let total = count.fetch_add(processed_in_batch, Ordering::SeqCst) + processed_in_batch;

        if last_report.elapsed() >= Duration::from_secs(5) {
            println!("[normalize] processed {} pages total", total);
            last_report = Instant::now();
        }
    }
}
