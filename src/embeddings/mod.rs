pub mod bulk;
pub mod client;
pub mod remote;
pub mod server;

use fastembed::EmbeddingModel;
use serde::{Deserialize, Serialize};

/// Shared model-name parsing — used by client, server, and bulk embedding.
pub fn parse_model_name(raw: &str) -> Result<EmbeddingModel, String> {
    let lower = raw.trim().to_ascii_lowercase();

    let mapped = match lower.as_str() {
        "all-minilm-l6-v2" => Some(EmbeddingModel::AllMiniLML6V2),
        "all-minilm-l6-v2-q" => Some(EmbeddingModel::AllMiniLML6V2Q),
        "all-minilm-l12-v2" => Some(EmbeddingModel::AllMiniLML12V2),
        "all-minilm-l12-v2-q" => Some(EmbeddingModel::AllMiniLML12V2Q),
        "paraphrase-mpnet-base-v2" => Some(EmbeddingModel::ParaphraseMLMpnetBaseV2),
        "bge-small-en-v1.5" => Some(EmbeddingModel::BGESmallENV15),
        "bge-small-en-v1.5-q" => Some(EmbeddingModel::BGESmallENV15Q),
        "bge-base-en-v1.5" => Some(EmbeddingModel::BGEBaseENV15),
        "bge-base-en-v1.5-q" => Some(EmbeddingModel::BGEBaseENV15Q),
        "bge-large-en-v1.5" => Some(EmbeddingModel::BGELargeENV15),
        "bge-large-en-v1.5-q" => Some(EmbeddingModel::BGELargeENV15Q),
        _ => None,
    };

    if let Some(m) = mapped {
        return Ok(m);
    }

    use std::str::FromStr;
    EmbeddingModel::from_str(raw).map_err(|e| format!("unknown model '{raw}': {e}"))
}

/// Request body for the remote embedding server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedRequest {
    pub texts: Vec<String>,
    #[serde(default)]
    pub is_query: bool,
}

/// Response body from the remote embedding server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedResponse {
    pub vectors: Vec<Vec<f32>>,
    pub dim: usize,
    pub model: String,
}

/// Request body for the remote reranking server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankRequest {
    pub query: String,
    pub documents: Vec<String>,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

fn default_top_k() -> usize {
    10
}

/// Response body from the remote reranking server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RerankResponse {
    pub scores: Vec<f32>,
    pub indices: Vec<usize>,
}
