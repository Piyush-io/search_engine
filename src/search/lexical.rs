use std::path::Path;

use tantivy::{
    collector::TopDocs,
    query::QueryParser,
    schema::{Field, Schema, Value, STORED, STRING, TEXT},
    Index, IndexReader,
};

use crate::ChunkId;

#[derive(Clone)]
pub struct LexicalIndex {
    index: Index,
    reader: IndexReader,
    chunk_id_field: Field,
    title_field: Option<Field>,
    section_field: Option<Field>,
    text_field: Field,
    heading_field: Field,
    source_url_field: Field,
}

impl LexicalIndex {
    pub fn create_or_open(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        std::fs::create_dir_all(path)?;

        let index = if Path::new(path).join("meta.json").exists() {
            let existing = Index::open_in_dir(path)?;
            let schema = existing.schema();
            let has_title = schema.get_field("title").is_ok();
            let has_section = schema.get_field("section").is_ok();

            if has_title && has_section {
                existing
            } else {
                drop(existing);
                std::fs::remove_dir_all(path)?;
                std::fs::create_dir_all(path)?;

                let mut builder = Schema::builder();
                builder.add_text_field("chunk_id", STRING | STORED);
                builder.add_text_field("title", TEXT | STORED);
                builder.add_text_field("section", TEXT | STORED);
                builder.add_text_field("text", TEXT | STORED);
                builder.add_text_field("heading", TEXT | STORED);
                builder.add_text_field("source_url", TEXT | STORED);
                let schema = builder.build();
                Index::create_in_dir(path, schema)?
            }
        } else {
            let mut builder = Schema::builder();
            builder.add_text_field("chunk_id", STRING | STORED);
            builder.add_text_field("title", TEXT | STORED);
            builder.add_text_field("section", TEXT | STORED);
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
        let title_field = schema.get_field("title").ok();
        let section_field = schema.get_field("section").ok();
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
            title_field,
            section_field,
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

    pub fn fields(&self) -> (Field, Option<Field>, Option<Field>, Field, Field, Field) {
        (
            self.chunk_id_field,
            self.title_field,
            self.section_field,
            self.text_field,
            self.heading_field,
            self.source_url_field,
        )
    }

    fn build_query(&self, query_text: &str) -> String {
        let trimmed = query_text.trim();
        if trimmed.is_empty() {
            return trimmed.to_string();
        }

        let mut clauses = vec![format!("({trimmed})")];
        let word_count = trimmed.split_whitespace().count();

        if word_count <= 5 {
            if self.title_field.is_some() {
                clauses.push(format!("title:\"{trimmed}\"^10"));
            }
            if self.section_field.is_some() {
                clauses.push(format!("section:\"{trimmed}\"^8"));
            }
            clauses.push(format!("heading:\"{trimmed}\"^6"));
            clauses.push(format!("text:\"{trimmed}\"^3"));
        }

        clauses.join(" OR ")
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
        let mut default_fields = vec![self.text_field, self.heading_field, self.source_url_field];
        if let Some(title_field) = self.title_field {
            default_fields.push(title_field);
        }
        if let Some(section_field) = self.section_field {
            default_fields.push(section_field);
        }

        let mut parser = QueryParser::for_index(&self.index, default_fields);
        parser.set_conjunction_by_default();
        if let Some(title_field) = self.title_field {
            parser.set_field_boost(title_field, 4.0);
        }
        if let Some(section_field) = self.section_field {
            parser.set_field_boost(section_field, 3.0);
        }
        parser.set_field_boost(self.heading_field, 2.5);
        parser.set_field_boost(self.text_field, 1.0);
        parser.set_field_boost(self.source_url_field, 0.2);
        let q = parser.parse_query(&self.build_query(query_text))?;

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
