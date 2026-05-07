//! Stand-alone embedding inference engine.
//! Intended to run on the machine with the NVIDIA GPU (or any accelerator).

use crate::embeddings;
use fastembed::{InitOptions, RerankInitOptions, RerankerModel, TextEmbedding, TextRerank};
use ort::execution_providers::ExecutionProviderDispatch;
use std::sync::Mutex;

/// Server-side embedding state (explicit init — no OnceLock).
pub struct EmbedServer {
    model: Mutex<TextEmbedding>,
    pub dim: usize,
    pub model_name: String,
}

impl EmbedServer {
    pub fn new(model_name: &str, dim: usize, max_length: usize) -> Result<Self, String> {
        let parsed = embeddings::parse_model_name(model_name)?;
        let info = TextEmbedding::get_model_info(&parsed)
            .map_err(|e| format!("failed reading model metadata for {model_name}: {e}"))?;

        if info.dim != dim {
            return Err(format!(
                "embedding dim mismatch: config says {dim}, model '{model_name}' outputs {}",
                info.dim
            ));
        }

        let providers = default_providers();

        let opts = InitOptions::new(parsed)
            .with_show_download_progress(true)
            .with_max_length(max_length)
            .with_execution_providers(providers);

        let model = TextEmbedding::try_new(opts)
            .map_err(|e| format!("failed to initialize embedding model '{model_name}': {e}"))?;

        Ok(Self {
            model: Mutex::new(model),
            dim,
            model_name: model_name.to_string(),
        })
    }

    /// Embed a batch of texts.  Large batches are chunked internally so the
    /// GPU does not OOM.  Serialized via mutex because the underlying ONNX
    /// session is not thread-safe.
    pub fn embed(&self, texts: Vec<String>, is_query: bool) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let texts = if is_query {
            texts
                .into_iter()
                .map(|t| format_query_for_model(&self.model_name, &t))
                .collect()
        } else {
            texts
        };

        const MAX_BATCH: usize = 32;
        let mut out = Vec::with_capacity(texts.len());

        let model = self
            .model
            .lock()
            .map_err(|e| format!("model mutex poisoned: {e}"))?;

        for chunk in texts.chunks(MAX_BATCH) {
            let vecs = model
                .embed(chunk.to_vec(), Some(chunk.len()))
                .map_err(|e| format!("model '{}' embed failed: {e}", self.model_name))?;

            for v in vecs {
                out.push(normalize(v));
            }
        }

        Ok(out)
    }

    /// Embed a single text.
    pub fn embed_single(&self, text: &str, is_query: bool) -> Result<Vec<f32>, String> {
        let mut batch = self.embed(vec![text.to_string()], is_query)?;
        batch.pop().ok_or_else(|| "empty embedding".to_string())
    }
}

/// Pick the best execution provider for the current platform.
fn default_providers() -> Vec<ExecutionProviderDispatch> {
    #[cfg(not(target_os = "macos"))]
    {
        use ort::execution_providers::CUDAExecutionProvider;
        vec![CUDAExecutionProvider::default().build()]
    }
    #[cfg(target_os = "macos")]
    {
        use ort::execution_providers::CoreMLExecutionProvider;
        vec![CoreMLExecutionProvider::default().build()]
    }
}

fn format_query_for_model(model_name: &str, query: &str) -> String {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let lower = model_name.to_ascii_lowercase();
    let needs_bge_instruction = lower.starts_with("bge-") || lower.contains("/bge-");
    if needs_bge_instruction {
        format!("Represent this sentence for searching relevant passages: {trimmed}")
    } else {
        trimmed.to_string()
    }
}

fn normalize(v: Vec<f32>) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.into_iter().map(|x| x / norm).collect()
    } else {
        v
    }
}

/// Server-side cross-encoder reranker state.
pub struct RerankerServer {
    model: Mutex<TextRerank>,
    pub model_name: String,
    pub max_length: usize,
}

impl RerankerServer {
    pub fn new(model_name: &str, max_length: usize) -> Result<Self, String> {
        let parsed = parse_reranker_model_name(model_name)?;

        let providers = default_providers();

        let opts = RerankInitOptions::new(parsed)
            .with_show_download_progress(true)
            .with_max_length(max_length)
            .with_execution_providers(providers);

        let model = TextRerank::try_new(opts)
            .map_err(|e| format!("failed to initialize reranker model '{model_name}': {e}"))?;

        Ok(Self {
            model: Mutex::new(model),
            model_name: model_name.to_string(),
            max_length,
        })
    }

    /// Rerank documents for a single query. Returns scores in the same order as input documents.
    pub fn rerank(&self, query: &str, documents: &[String]) -> Result<Vec<f32>, String> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        const MAX_BATCH: usize = 32;
        let mut out = vec![0.0_f32; documents.len()];

        let model = self
            .model
            .lock()
            .map_err(|e| format!("reranker mutex poisoned: {e}"))?;

        for (chunk_idx, chunk) in documents.chunks(MAX_BATCH).enumerate() {
            let chunk_refs: Vec<&str> = chunk.iter().map(|s| s.as_str()).collect();
            let results = model
                .rerank(query, chunk_refs, false, Some(MAX_BATCH))
                .map_err(|e| format!("rerank failed: {e}"))?;

            for r in results {
                let global_idx = chunk_idx * MAX_BATCH + r.index;
                if global_idx < out.len() {
                    out[global_idx] = r.score;
                }
            }
        }

        Ok(out)
    }
}

fn parse_reranker_model_name(raw: &str) -> Result<RerankerModel, String> {
    let lower = raw.trim().to_ascii_lowercase();
    let mapped = match lower.as_str() {
        "bge-reranker-base" => Some(RerankerModel::BGERerankerBase),
        "bge-reranker-v2-m3" => Some(RerankerModel::BGERerankerV2M3),
        "jina-reranker-v1-turbo-en" => Some(RerankerModel::JINARerankerV1TurboEn),
        "jina-reranker-v2-base-multilingual" => Some(RerankerModel::JINARerankerV2BaseMultiligual),
        _ => None,
    };
    if let Some(m) = mapped {
        return Ok(m);
    }
    use std::str::FromStr;
    RerankerModel::from_str(raw).map_err(|e| format!("unknown reranker model '{raw}': {e}"))
}
