use search_engine::{config, storage, search::{vector_index::VectorIndex, lexical::LexicalIndex}, embeddings::client};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;

fn main() {
    let cfg = config::load().unwrap();
    let db = storage::open_db_read_only(&cfg.paths.db_path).unwrap();
    let lexical = LexicalIndex::open(&cfg.paths.lexical_index_path).unwrap();
    
    let searcher = lexical.reader().searcher(); // Need to expose reader
}
