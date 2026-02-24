pub mod chunking;
pub mod config;
pub mod crawler;
pub mod embeddings;
pub mod extraction;
pub mod knowledge;
pub mod search;
pub mod storage;
pub mod web;

use serde::{Deserialize, Serialize};

/// Unique identifier for a chunk: hash of (source_url + position_index)
pub type ChunkId = String;

/// A 768-dimensional sentence embedding from multi-qa-mpnet-base-dot-v1
pub type EmbeddingVec = [f32; 768];

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
