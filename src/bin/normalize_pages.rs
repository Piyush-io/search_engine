use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use rocksdb::{IteratorMode, WriteBatch};
use search_engine::{
    Chunk, PageRecord,
    chunking::{chaining, context, sentencizer},
    config,
    crawler::policy,
    pipeline::{IndexOperation, PageState},
    storage,
};
use sha2::{Digest, Sha256};
use url::Url;

const MAX_CHUNKS_PER_PAGE: usize = 220;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

fn page_content_hash(page_data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(page_data);
    format!("{:x}", h.finalize())
}

fn make_chunk_id(url: &str, content_hash: &str, pos: usize) -> String {
    let mut h = Sha256::new();
    h.update(url.as_bytes());
    h.update(b"#");
    h.update(content_hash.as_bytes());
    h.update(b"#");
    h.update(pos.to_string().as_bytes());
    format!("{:x}", h.finalize())
}

fn chunk_limit_for_page(page: &PageRecord) -> usize {
    Url::parse(&page.url)
        .ok()
        .and_then(|url| url.host_str().map(policy::chunk_limit_for_host))
        .unwrap_or(MAX_CHUNKS_PER_PAGE)
}

fn build_chunks(cfg: &config::Config, page: &PageRecord, content_hash: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut preceding_sentences: Vec<String> = Vec::new();
    let chunk_limit = chunk_limit_for_page(page);

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
            let chunk_id = make_chunk_id(&page.url, content_hash, chunks.len());

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
    let embeddings_cf = storage::cf(&db, storage::CF_EMBEDDINGS)?;
    let page_state_cf = storage::cf(&db, storage::CF_PAGE_STATE)?;
    let embed_queue_cf = storage::cf(&db, storage::CF_EMBED_QUEUE)?;
    let vector_queue_cf = storage::cf(&db, storage::CF_VECTOR_QUEUE)?;
    let lexical_queue_cf = storage::cf(&db, storage::CF_LEXICAL_QUEUE)?;
    let tombstones_cf = storage::cf(&db, storage::CF_VECTOR_TOMBSTONES)?;

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
        let mut processed_in_batch = 0usize;

        for url_bytes in &batch {
            let Some(page_data) = db.get_cf(content_cf, url_bytes)? else {
                wb.delete_cf(norm_queue_cf, url_bytes);
                continue;
            };

            let Ok(page) = serde_json::from_slice::<PageRecord>(&page_data) else {
                wb.delete_cf(norm_queue_cf, url_bytes);
                continue;
            };

            let content_hash = page_content_hash(&page_data);
            let old_state = db
                .get_cf(page_state_cf, url_bytes)?
                .and_then(|bytes| serde_json::from_slice::<PageState>(&bytes).ok());

            if old_state
                .as_ref()
                .is_some_and(|state| state.content_hash == content_hash)
            {
                wb.delete_cf(norm_queue_cf, url_bytes);
                processed_in_batch += 1;
                continue;
            }

            let chunks = build_chunks(&cfg, &page, &content_hash);
            let new_chunk_ids: HashSet<String> =
                chunks.iter().map(|chunk| chunk.id.clone()).collect();

            if let Some(state) = old_state {
                for old_chunk_id in state.chunk_ids {
                    if new_chunk_ids.contains(&old_chunk_id) {
                        continue;
                    }

                    wb.delete_cf(chunks_cf, old_chunk_id.as_bytes());
                    wb.delete_cf(embeddings_cf, old_chunk_id.as_bytes());
                    wb.delete_cf(embed_queue_cf, old_chunk_id.as_bytes());
                    wb.put_cf(
                        vector_queue_cf,
                        old_chunk_id.as_bytes(),
                        IndexOperation::Delete.as_bytes(),
                    );
                    wb.put_cf(
                        lexical_queue_cf,
                        old_chunk_id.as_bytes(),
                        IndexOperation::Delete.as_bytes(),
                    );
                    wb.put_cf(tombstones_cf, old_chunk_id.as_bytes(), []);
                }
            }

            for chunk in &chunks {
                wb.put_cf(chunks_cf, chunk.id.as_bytes(), serde_json::to_vec(chunk)?);
                if chunk.is_leaf {
                    wb.put_cf(embed_queue_cf, chunk.id.as_bytes(), []);
                    wb.put_cf(
                        lexical_queue_cf,
                        chunk.id.as_bytes(),
                        IndexOperation::Upsert.as_bytes(),
                    );
                }
            }

            let page_state = PageState {
                content_hash,
                chunk_ids: chunks.iter().map(|chunk| chunk.id.clone()).collect(),
                last_crawled_ms: now_ms(),
            };
            wb.put_cf(page_state_cf, url_bytes, serde_json::to_vec(&page_state)?);
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
