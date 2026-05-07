use std::path::Path;

use tantivy::{
    Index, IndexReader, Term,
    collector::TopDocs,
    query::QueryParser,
    schema::{Field, STORED, STRING, Schema, TEXT, Value},
};

use crate::{Chunk, ChunkId};

/// Lexical field boost configuration — wired from `config.toml`.
#[derive(Debug, Clone)]
pub struct LexicalBoostConfig {
    pub field_boost_title: f32,
    pub field_boost_section: f32,
    pub field_boost_heading: f32,
    pub field_boost_text: f32,
    /// Multiplier on field boosts for short-query phrase clauses.
    /// Default 2.5 preserves legacy hardcoded ^10/^8/^6/^3 pattern.
    pub short_query_phrase_boost: f32,
}

impl Default for LexicalBoostConfig {
    fn default() -> Self {
        Self {
            field_boost_title: 4.0,
            field_boost_section: 3.0,
            field_boost_heading: 2.5,
            field_boost_text: 1.0,
            short_query_phrase_boost: 2.5,
        }
    }
}

fn query_boost(value: f32) -> String {
    let value = value.max(0.0);
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

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

    pub fn chunk_term(&self, chunk_id: &str) -> Term {
        Term::from_field_text(self.chunk_id_field, chunk_id)
    }

    pub fn document_for_chunk(&self, chunk: &Chunk) -> Option<tantivy::TantivyDocument> {
        if !chunk.is_leaf {
            return None;
        }

        let mut doc = tantivy::doc!();
        doc.add_text(self.chunk_id_field, &chunk.id);
        if let Some(tf) = self.title_field {
            if let Some(title) = chunk
                .page_title
                .as_deref()
                .or_else(|| chunk.heading_chain.first().map(String::as_str))
            {
                doc.add_text(tf, title);
            }
        }
        if let Some(sf) = self.section_field {
            if let Some(section) = chunk.heading_chain.last() {
                doc.add_text(sf, section);
            }
        }
        doc.add_text(self.text_field, &chunk.text);
        doc.add_text(self.heading_field, &chunk.heading_chain.join(" "));
        doc.add_text(self.source_url_field, &chunk.source_url);
        Some(doc)
    }

    fn escape_query_text(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for ch in text.chars() {
            if matches!(
                ch,
                '+' | '-'
                    | '&'
                    | '|'
                    | '!'
                    | '('
                    | ')'
                    | '{'
                    | '}'
                    | '['
                    | ']'
                    | '^'
                    | '"'
                    | '~'
                    | '*'
                    | '?'
                    | ':'
                    | '\\'
                    | '/'
            ) {
                out.push('\\');
            }
            out.push(ch);
        }
        out
    }

    fn build_query(&self, query_text: &str, boosts: &LexicalBoostConfig) -> String {
        let trimmed = query_text.trim();
        if trimmed.is_empty() {
            return trimmed.to_string();
        }

        let escaped = Self::escape_query_text(trimmed);

        let mut clauses = vec![format!("({escaped})")];
        let word_count = trimmed.split_whitespace().count();

        if word_count <= 5 {
            let phrase_mult = boosts.short_query_phrase_boost.max(0.0);
            if self.title_field.is_some() {
                let boost = query_boost(boosts.field_boost_title * phrase_mult);
                clauses.push(format!("title:\"{escaped}\"^{boost}"));
            }
            if self.section_field.is_some() {
                let boost = query_boost(boosts.field_boost_section * phrase_mult);
                clauses.push(format!("section:\"{escaped}\"^{boost}"));
            }
            {
                let boost = query_boost(boosts.field_boost_heading * phrase_mult);
                clauses.push(format!("heading:\"{escaped}\"^{boost}"));
            }
            {
                let boost = query_boost(boosts.field_boost_text * phrase_mult);
                clauses.push(format!("text:\"{escaped}\"^{boost}"));
            }
        }

        clauses.join(" OR ")
    }

    pub fn search(
        &self,
        query_text: &str,
        k: usize,
        relaxed_fallback_enabled: bool,
        relaxed_min_hits: usize,
        relaxed_extra_k: usize,
        boosts: &LexicalBoostConfig,
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

        let mut parser = QueryParser::for_index(&self.index, default_fields.clone());
        parser.set_conjunction_by_default();
        if let Some(title_field) = self.title_field {
            parser.set_field_boost(title_field, boosts.field_boost_title);
        }
        if let Some(section_field) = self.section_field {
            parser.set_field_boost(section_field, boosts.field_boost_section);
        }
        parser.set_field_boost(self.heading_field, boosts.field_boost_heading);
        parser.set_field_boost(self.text_field, boosts.field_boost_text);
        parser.set_field_boost(self.source_url_field, 0.2);
        let query = self.build_query(query_text, boosts);
        let q = parser.parse_query(&query)?;

        let top = searcher.search(&q, &TopDocs::with_limit(k))?;
        let mut out = Vec::with_capacity(top.len());
        let mut seen = std::collections::HashSet::new();

        for (score, addr) in top {
            let doc: tantivy::TantivyDocument = searcher.doc(addr)?;
            let Some(val) = doc.get_first(self.chunk_id_field) else {
                continue;
            };
            let Some(id) = val.as_str() else {
                continue;
            };
            let id = id.to_string();
            if seen.insert(id.clone()) {
                out.push((id, score));
            }
        }

        if relaxed_fallback_enabled && out.len() < relaxed_min_hits && out.len() < k {
            let relaxed_limit = (k + relaxed_extra_k).max(k);
            let mut parser = QueryParser::for_index(&self.index, default_fields);
            if let Some(title_field) = self.title_field {
                parser.set_field_boost(title_field, boosts.field_boost_title);
            }
            if let Some(section_field) = self.section_field {
                parser.set_field_boost(section_field, boosts.field_boost_section);
            }
            parser.set_field_boost(self.heading_field, boosts.field_boost_heading);
            parser.set_field_boost(self.text_field, boosts.field_boost_text);
            parser.set_field_boost(self.source_url_field, 0.2);
            let q = parser.parse_query(&query)?;

            let relaxed_top = searcher.search(&q, &TopDocs::with_limit(relaxed_limit))?;
            for (score, addr) in relaxed_top {
                if out.len() >= k {
                    break;
                }
                let doc: tantivy::TantivyDocument = searcher.doc(addr)?;
                let Some(val) = doc.get_first(self.chunk_id_field) else {
                    continue;
                };
                let Some(id) = val.as_str() else {
                    continue;
                };
                let id = id.to_string();
                if seen.insert(id.clone()) {
                    out.push((id, score));
                }
            }
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::LexicalIndex;

    #[test]
    fn escapes_reserved_query_parser_characters() {
        let escaped = LexicalIndex::escape_query_text("rust async? tokio::spawn");
        assert_eq!(escaped, "rust async\\? tokio\\:\\:spawn");
    }
}
