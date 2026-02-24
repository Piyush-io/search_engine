use std::cmp::Ordering;

use bytemuck::cast_slice;
use rocksdb::{Error as RocksError, IteratorMode, WriteBatch, WriteOptions};
use search_engine::{Chunk, config, embeddings::client, storage};

fn is_cf_empty(
    db: &rocksdb::DB,
    cf: rocksdb::ColumnFamilyRef<'_>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut iter = db.iterator_cf(cf, IteratorMode::Start);
    match iter.next() {
        None => Ok(true),
        Some(item) => {
            let _ = item?;
            Ok(false)
        }
    }
}

fn next_existing_key<I>(iter: &mut I) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>>
where
    I: Iterator<Item = Result<(Box<[u8]>, Box<[u8]>), RocksError>>,
{
    match iter.next() {
        None => Ok(None),
        Some(item) => {
            let (k, _) = item?;
            Ok(Some(k.to_vec()))
        }
    }
}

fn flush_batch(
    db: &rocksdb::DB,
    embeddings_cf: rocksdb::ColumnFamilyRef<'_>,
    write_opts: &WriteOptions,
    ids: &mut Vec<String>,
    texts: &mut Vec<String>,
) -> Result<usize, Box<dyn std::error::Error>> {
    if texts.is_empty() {
        return Ok(0);
    }

    let vectors = client::embed_batch(texts);
    let mut wb = WriteBatch::default();

    for (i, vec) in vectors.iter().enumerate() {
        let id = &ids[i];
        wb.put_cf(embeddings_cf, id.as_bytes(), cast_slice(vec));
    }

    db.write_opt(wb, write_opts)?;

    let n = texts.len();
    ids.clear();
    texts.clear();
    Ok(n)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = storage::open_db(&cfg.paths.db_path)?;

    let chunks_cf = storage::cf(&db, storage::CF_CHUNKS)?;
    let embeddings_cf = storage::cf(&db, storage::CF_EMBEDDINGS)?;

    let embeddings_empty = is_cf_empty(&db, embeddings_cf)?;
    if embeddings_empty {
        println!("[embed] fast mode: embeddings CF empty, skipping existence checks");
    }

    let mut write_opts = WriteOptions::default();
    // Embeddings can be rebuilt; disabling WAL gives a major speedup for bulk ingest.
    write_opts.disable_wal(true);

    let mut ids = Vec::with_capacity(cfg.embedding.batch_size);
    let mut texts = Vec::with_capacity(cfg.embedding.batch_size);

    let mut seen_chunks = 0usize;
    let mut embedded_chunks = 0usize;

    let mut emb_iter = db.iterator_cf(embeddings_cf, IteratorMode::Start);
    let mut next_emb_key = if embeddings_empty {
        None
    } else {
        next_existing_key(&mut emb_iter)?
    };

    for item in db.iterator_cf(chunks_cf, IteratorMode::Start) {
        let (key, value) = item?;
        seen_chunks += 1;

        let mut already_embedded = false;
        if !embeddings_empty {
            while let Some(ref emb_key) = next_emb_key {
                match emb_key.as_slice().cmp(key.as_ref()) {
                    Ordering::Less => {
                        next_emb_key = next_existing_key(&mut emb_iter)?;
                    }
                    Ordering::Equal => {
                        already_embedded = true;
                        next_emb_key = next_existing_key(&mut emb_iter)?;
                        break;
                    }
                    Ordering::Greater => break,
                }
            }
        }

        if already_embedded {
            continue;
        }

        let chunk: Chunk = serde_json::from_slice(&value)?;
        ids.push(String::from_utf8(key.to_vec())?);
        texts.push(chunk.text);

        if texts.len() >= cfg.embedding.batch_size {
            embedded_chunks += flush_batch(&db, embeddings_cf, &write_opts, &mut ids, &mut texts)?;
            if embedded_chunks % 1_000 == 0 {
                println!(
                    "[embed] embedded={} scanned={} batch_size={}",
                    embedded_chunks, seen_chunks, cfg.embedding.batch_size
                );
            }
        }
    }

    embedded_chunks += flush_batch(&db, embeddings_cf, &write_opts, &mut ids, &mut texts)?;

    println!(
        "[embed] done. scanned_chunks={} newly_embedded={}",
        seen_chunks, embedded_chunks
    );

    Ok(())
}
