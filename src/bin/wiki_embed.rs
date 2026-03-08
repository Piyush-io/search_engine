use bytemuck::cast_slice;
use rocksdb::{IteratorMode, WriteBatch, WriteOptions};
use search_engine::{config, embeddings::client, knowledge::wikipedia::WikiRecord, storage};

fn flush_batch(
    db: &rocksdb::DB,
    wiki_embeddings_cf: rocksdb::ColumnFamilyRef<'_>,
    write_opts: &WriteOptions,
    ids: &mut Vec<String>,
    texts: &mut Vec<String>,
) -> Result<usize, Box<dyn std::error::Error>> {
    if texts.is_empty() {
        return Ok(0);
    }

    let vectors = client::embed_batch(texts)?;
    let mut wb = WriteBatch::default();

    for (i, vec) in vectors.iter().enumerate() {
        let id = &ids[i];
        wb.put_cf(
            wiki_embeddings_cf,
            id.as_bytes(),
            cast_slice(vec.as_slice()),
        );
    }

    db.write_opt(wb, write_opts)?;

    let n = texts.len();
    ids.clear();
    texts.clear();
    Ok(n)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let runtime_dim = client::configured_dim()?;
    if runtime_dim != cfg.embedding.dim {
        return Err(format!(
            "embedding dim mismatch: config={} runtime={}",
            cfg.embedding.dim, runtime_dim
        )
        .into());
    }

    let db = storage::open_db_for_bulk_write(&cfg.paths.db_path)?;
    let wiki_cf = storage::cf(&db, storage::CF_WIKI)?;
    let wiki_embeddings_cf = storage::cf(&db, storage::CF_WIKI_EMBEDDINGS)?;

    let mut write_opts = WriteOptions::default();
    write_opts.disable_wal(true);

    let mut ids = Vec::with_capacity(cfg.embedding.batch_size);
    let mut texts = Vec::with_capacity(cfg.embedding.batch_size);

    let mut scanned = 0usize;
    let mut embedded = 0usize;

    for item in db.iterator_cf(wiki_cf, IteratorMode::Start) {
        let (_, value) = item?;
        let rec: WikiRecord = serde_json::from_slice(&value)?;
        scanned += 1;

        let key = rec.title.to_ascii_lowercase();
        let mut text = format!("{}\n{}", rec.title, rec.summary);
        if let Some(desc) = rec.description {
            if !desc.trim().is_empty() {
                text.push('\n');
                text.push_str(desc.trim());
            }
        }

        ids.push(key);
        texts.push(text);

        if texts.len() >= cfg.embedding.batch_size {
            embedded += flush_batch(&db, wiki_embeddings_cf, &write_opts, &mut ids, &mut texts)?;
            if embedded % 5_000 == 0 {
                println!("[wiki_embed] scanned={} embedded={}", scanned, embedded);
            }
        }
    }

    embedded += flush_batch(&db, wiki_embeddings_cf, &write_opts, &mut ids, &mut texts)?;

    println!(
        "[wiki_embed] done. scanned={} embedded={} dim={}",
        scanned, embedded, cfg.embedding.dim
    );

    Ok(())
}
