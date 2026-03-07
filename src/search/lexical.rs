use std::path::Path;

use tantivy::{
    Index, IndexReader,
    collector::TopDocs,
    query::QueryParser,
    schema::{Field, STORED, STRING, Schema, TEXT, Value},
};

use crate::ChunkId;

#[derive(Clone)]
pub struct LexicalIndex {
    index: Index,
    reader: IndexReader,
    chunk_id_field: Field,
    text_field: Field,
    heading_field: Field,
    source_url_field: Field,
}

impl LexicalIndex {
    pub fn create_or_open(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        std::fs::create_dir_all(path)?;

        let index = if Path::new(path).join("meta.json").exists() {
            Index::open_in_dir(path)?
        } else {
            let mut builder = Schema::builder();
            builder.add_text_field("chunk_id", STRING | STORED);
            builder.add_text_field("text", TEXT | STORED);
            builder.add_text_field("heading", TEXT | STORED);
            builder.add_text_field("source_url", TEXT | STORED);
            let schema = builder.build();
            Index::create_in_dir(path, schema)?
        };

        Self::from_index(index)
    }

    pub fn open(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let index = Index::open_in_dir(path)?;
        Self::from_index(index)
    }

    fn from_index(index: Index) -> Result<Self, Box<dyn std::error::Error>> {
        let schema = index.schema();
        let chunk_id_field = schema
            .get_field("chunk_id")
            .map_err(|_| "missing field chunk_id")?;
        let text_field = schema.get_field("text").map_err(|_| "missing field text")?;
        let heading_field = schema
            .get_field("heading")
            .map_err(|_| "missing field heading")?;
        let source_url_field = schema
            .get_field("source_url")
            .map_err(|_| "missing field source_url")?;

        let reader = index
            .reader_builder()
            .reload_policy(tantivy::ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        Ok(Self {
            index,
            reader,
            chunk_id_field,
            text_field,
            heading_field,
            source_url_field,
        })
    }

    pub fn writer(
        &self,
        heap_size_bytes: usize,
    ) -> Result<tantivy::IndexWriter, Box<dyn std::error::Error>> {
        Ok(self.index.writer(heap_size_bytes)?)
    }

    pub fn fields(&self) -> (Field, Field, Field, Field) {
        (
            self.chunk_id_field,
            self.text_field,
            self.heading_field,
            self.source_url_field,
        )
    }

    pub fn search(
        &self,
        query_text: &str,
        k: usize,
    ) -> Result<Vec<(ChunkId, f32)>, Box<dyn std::error::Error>> {
        if k == 0 {
            return Ok(Vec::new());
        }

        let searcher = self.reader.searcher();
        let mut parser = QueryParser::for_index(
            &self.index,
            vec![self.text_field, self.heading_field, self.source_url_field],
        );
        parser.set_conjunction_by_default();
        let q = parser.parse_query(query_text)?;

        let top = searcher.search(&q, &TopDocs::with_limit(k))?;
        let mut out = Vec::with_capacity(top.len());

        for (score, addr) in top {
            let doc: tantivy::TantivyDocument = searcher.doc(addr)?;
            let Some(val) = doc.get_first(self.chunk_id_field) else {
                continue;
            };
            let Some(id) = val.as_str() else {
                continue;
            };
            out.push((id.to_string(), score));
        }

        Ok(out)
    }
}
