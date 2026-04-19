pub mod chunking;
pub mod config;
pub mod crawler;
pub mod embeddings;
pub mod eval;
pub mod extraction;
pub mod knowledge;
pub mod pipeline;
pub mod search;
pub mod storage;
pub mod web;

use serde::{Deserialize, Serialize};

/// Unique identifier for a chunk: hash of (source_url + position_index)
pub type ChunkId = String;

/// Dense embedding vector produced by the configured embedding model.
pub type EmbeddingVec = Vec<f32>;

/// A single content block extracted from a page, with its heading context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextBlock {
    /// Heading chain from root to this block, e.g. ["Guide", "Connection Settings"]
    pub heading_chain: Vec<String>,
    pub text: String,
}

/// Normalised page record stored in RocksDB `content` column family
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageRecord {
    pub url: String,
    pub title: String,
    pub description: Option<String>,
    pub blocks: Vec<TextBlock>,
}

/// Sentence-level chunk stored in RocksDB `chunks` column family
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: ChunkId,
    pub source_url: String,
    /// Full heading chain prepended before embedding
    pub heading_chain: Vec<String>,
    pub text: String,
    /// Text used for embedding (includes page title + heading context). Falls back to `text`.
    #[serde(default)]
    pub embed_text: Option<String>,
    /// Page title from the source document
    #[serde(default)]
    pub page_title: Option<String>,
    /// False for sentences that are pure antecedents (statement chaining)
    pub is_leaf: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub chunk_id: ChunkId,
    pub score: f32,
    pub text: String,
    pub source_url: String,
    pub heading_chain: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoredHit {
    pub chunk_id: ChunkId,
    pub text: String,
    pub source_url: String,
    pub heading_chain: Vec<String>,
    pub vector_score: f32,
    pub lexical_score: f32,
    pub title_overlap: f32,
    pub heading_overlap: f32,
    pub body_overlap: f32,
    pub authority_bonus: f32,
    pub exact_heading_phrase: bool,
    pub exact_body_phrase: bool,
    pub final_score: f32,
}

impl ScoredHit {
    pub fn to_search_result(&self) -> SearchResult {
        SearchResult {
            chunk_id: self.chunk_id.clone(),
            score: self.final_score,
            text: self.text.clone(),
            source_url: self.source_url.clone(),
            heading_chain: self.heading_chain.clone(),
        }
    }
}
