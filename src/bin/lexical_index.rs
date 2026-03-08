use rocksdb::IteratorMode;
use search_engine::{config, search::lexical::LexicalIndex, storage, Chunk};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = storage::open_db_read_only(&cfg.paths.db_path)?;
    let chunks_cf = storage::cf(&db, storage::CF_CHUNKS)?;

    let lexical = LexicalIndex::create_or_open(&cfg.paths.lexical_index_path)?;
    let (chunk_id_field, title_field, section_field, text_field, heading_field, source_url_field) =
        lexical.fields();
    let mut writer = lexical.writer(512 * 1024 * 1024)?;

    writer.delete_all_documents()?;

    let mut indexed = 0usize;
    for item in db.iterator_cf(chunks_cf, IteratorMode::Start) {
        let (_, value) = item?;
        let chunk: Chunk = serde_json::from_slice(&value)?;

        let mut doc = tantivy::doc!();
        doc.add_text(chunk_id_field, &chunk.id);
        if let Some(title_field) = title_field {
            if let Some(title) = chunk.heading_chain.first() {
                doc.add_text(title_field, title);
            }
        }
        if let Some(section_field) = section_field {
            if let Some(section) = chunk.heading_chain.last() {
                doc.add_text(section_field, section);
            }
        }
        doc.add_text(text_field, &chunk.text);
        doc.add_text(heading_field, &chunk.heading_chain.join(" "));
        doc.add_text(source_url_field, &chunk.source_url);
        writer.add_document(doc)?;

        indexed += 1;
        if indexed % 10_000 == 0 {
            println!("[lexical_index] indexed={}", indexed);
        }
    }

    writer.commit()?;
    println!(
        "[lexical_index] done. indexed={} path={}",
        indexed, cfg.paths.lexical_index_path
    );

    Ok(())
}
