use serde::{Deserialize, Serialize};

use crate::ChunkId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageState {
    pub content_hash: String,
    pub chunk_ids: Vec<ChunkId>,
    #[serde(default)]
    pub last_crawled_ms: i64,
    #[serde(default)]
    pub last_fetch_ms: i64,
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexOperation {
    Upsert,
    Delete,
}

impl IndexOperation {
    pub fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::Upsert => b"u",
            Self::Delete => b"d",
        }
    }

    pub fn from_bytes(value: &[u8]) -> Option<Self> {
        match value {
            b"u" => Some(Self::Upsert),
            b"d" => Some(Self::Delete),
            _ => None,
        }
    }
}
