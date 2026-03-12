use rayon::prelude::*;
use rocksdb::ReadOptions;
use search_engine::{Chunk, config, search::lexical::LexicalIndex, storage};

const READAHEAD_BYTES: usize = 8 * 1024 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = storage::open_db_read_only(&cfg.paths.db_path)?;
    let chunks_cf = storage::cf(&db, storage::CF_CHUNKS)?;

    let lexical = LexicalIndex::create_or_open(&cfg.paths.lexical_index_path)?;
    let (chunk_id_field, title_field, section_field, text_field, heading_field, source_url_field) =
        lexical.fields();
    let mut writer = lexical.writer(512 * 1024 * 1024)?;

    writer.delete_all_documents()?;

    // Scan all raw values into memory, then deserialize + build docs in parallel with Rayon.
    println!("[lexical_index] scanning chunks into memory…");
    let mut raw_values: Vec<Box<[u8]>> = Vec::new();

    let mut read_opts = ReadOptions::default();
    read_opts.fill_cache(false);
    read_opts.set_readahead_size(READAHEAD_BYTES);
    read_opts.set_auto_readahead_size(true);
    let mut iter = db.raw_iterator_cf_opt(&chunks_cf, read_opts);
    iter.seek_to_first();
    while iter.valid() {
        if let Some(value) = iter.value() {
            raw_values.push(value.into());
        }
        iter.next();
    }

    let total = raw_values.len();
    println!(
        "[lexical_index] deserializing and building {} documents in parallel…",
        total
    );

    // Parallel deserialization with Rayon — each core handles a partition.
    let docs: Vec<tantivy::TantivyDocument> = raw_values
        .par_iter()
        .filter_map(|value| {
            let chunk: Chunk = serde_json::from_slice(value).ok()?;
            let mut doc = tantivy::doc!();
            doc.add_text(chunk_id_field, &chunk.id);
            if let Some(tf) = title_field {
                if let Some(title) = chunk.heading_chain.first() {
                    doc.add_text(tf, title);
                }
            }
            if let Some(sf) = section_field {
                if let Some(section) = chunk.heading_chain.last() {
                    doc.add_text(sf, section);
                }
            }
            doc.add_text(text_field, &chunk.text);
            doc.add_text(heading_field, &chunk.heading_chain.join(" "));
            doc.add_text(source_url_field, &chunk.source_url);
            Some(doc)
        })
        .collect();

    println!("[lexical_index] adding {} documents to index…", docs.len());
    let mut indexed = 0usize;
    let mut countdown = 10_000usize;
    for doc in docs {
        writer.add_document(doc)?;
        indexed += 1;
        if countdown == 0 {
            println!("[lexical_index] indexed={}", indexed);
            countdown = 10_000;
        } else {
            countdown -= 1;
        }
    }

    writer.commit()?;
    println!(
        "[lexical_index] done. indexed={} path={}",
        indexed, cfg.paths.lexical_index_path
    );

    Ok(())
}
