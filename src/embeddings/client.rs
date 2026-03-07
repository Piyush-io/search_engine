use std::{io, str::FromStr, sync::OnceLock};

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use ort::execution_providers::{CoreMLExecutionProvider, ExecutionProviderDispatch};

use crate::{EmbeddingVec, config};

const FASTEMBED_MAX_INNER_BATCH: usize = 32;

struct ModelState {
    model: TextEmbedding,
    dim: usize,
    model_name: String,
    backend: String,
}

static MODEL: OnceLock<Result<ModelState, String>> = OnceLock::new();

fn parse_model_name(raw: &str) -> Result<EmbeddingModel, String> {
    let lower = raw.trim().to_ascii_lowercase();

    let mapped = match lower.as_str() {
        "all-minilm-l6-v2" => Some(EmbeddingModel::AllMiniLML6V2),
        "all-minilm-l6-v2-q" => Some(EmbeddingModel::AllMiniLML6V2Q),
        "all-minilm-l12-v2" => Some(EmbeddingModel::AllMiniLML12V2),
        "all-minilm-l12-v2-q" => Some(EmbeddingModel::AllMiniLML12V2Q),
        "paraphrase-mpnet-base-v2" => Some(EmbeddingModel::ParaphraseMLMpnetBaseV2),
        _ => None,
    };

    if let Some(m) = mapped {
        return Ok(m);
    }

    EmbeddingModel::from_str(raw)
}

fn coreml_providers() -> Vec<ExecutionProviderDispatch> {
    vec![
        // Prefer ANE (Apple Neural Engine) when available; falls back to GPU then CPU.
        CoreMLExecutionProvider::default().build(),
    ]
}

fn init_model() -> Result<ModelState, String> {
    let cfg = config::load().map_err(|e| format!("failed loading config.toml: {e}"))?;

    let backend = cfg.embedding.backend.trim().to_ascii_lowercase();

    let parsed = parse_model_name(&cfg.embedding.model)?;

    let info = TextEmbedding::get_model_info(&parsed).map_err(|e| {
        format!(
            "failed reading model metadata for {}: {e}",
            cfg.embedding.model
        )
    })?;

    if info.dim != cfg.embedding.dim {
        return Err(format!(
            "embedding dim mismatch: config says {}, model '{}' outputs {}",
            cfg.embedding.dim, cfg.embedding.model, info.dim
        ));
    }

    let max_length = cfg.embedding.max_length.unwrap_or(256);
    let mut opts = InitOptions::new(parsed)
        .with_show_download_progress(true)
        .with_max_length(max_length);

    // Wire CoreML execution provider when backend = "coreml"
    if backend == "coreml" {
        opts = opts.with_execution_providers(coreml_providers());
    }

    let model = TextEmbedding::try_new(opts).map_err(|e| {
        format!(
            "failed to initialize embedding model '{}' (backend={}): {e}",
            cfg.embedding.model, backend
        )
    })?;

    Ok(ModelState {
        model,
        dim: cfg.embedding.dim,
        model_name: cfg.embedding.model,
        backend,
    })
}

fn state() -> Result<&'static ModelState, Box<dyn std::error::Error>> {
    let res = MODEL.get_or_init(init_model);
    match res {
        Ok(s) => Ok(s),
        Err(msg) => Err(io::Error::other(msg.clone()).into()),
    }
}

pub fn configured_dim() -> Result<usize, Box<dyn std::error::Error>> {
    Ok(state()?.dim)
}

pub fn backend_info() -> Result<String, Box<dyn std::error::Error>> {
    let s = state()?;
    Ok(format!(
        "backend={} model={} dim={}",
        s.backend, s.model_name, s.dim
    ))
}

/// Embed a single text. Fails fast if model is unavailable.
pub fn embed(text: &str) -> Result<EmbeddingVec, Box<dyn std::error::Error>> {
    let s = state()?;
    let out = s
        .model
        .embed(vec![text.to_string()], Some(1))
        .map_err(|e| io::Error::other(format!("model '{}' embed failed: {e}", s.model_name)))?;

    let Some(v) = out.into_iter().next() else {
        return Err(io::Error::other("embedding model returned empty batch").into());
    };

    validate_and_normalize(v, s.dim)
}

pub fn embed_batch(texts: &[String]) -> Result<Vec<EmbeddingVec>, Box<dyn std::error::Error>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let s = state()?;
    let mut vecs = Vec::with_capacity(texts.len());

    for batch in texts.chunks(FASTEMBED_MAX_INNER_BATCH) {
        let out = s
            .model
            .embed(batch.to_vec(), Some(batch.len()))
            .map_err(|e| {
                io::Error::other(format!("model '{}' batch embed failed: {e}", s.model_name))
            })?;

        for v in out {
            vecs.push(validate_and_normalize(v, s.dim)?);
        }
    }

    Ok(vecs)
}

pub fn cosine_similarity(a: &EmbeddingVec, b: &EmbeddingVec) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }

    let mut dot = 0.0_f32;
    let mut an = 0.0_f32;
    let mut bn = 0.0_f32;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        an += a[i] * a[i];
        bn += b[i] * b[i];
    }

    if an == 0.0 || bn == 0.0 {
        return 0.0;
    }

    dot / (an.sqrt() * bn.sqrt())
}

fn validate_and_normalize(
    v: Vec<f32>,
    expected_dim: usize,
) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    if v.len() != expected_dim {
        return Err(io::Error::other(format!(
            "embedding vector dim mismatch: expected {}, got {}",
            expected_dim,
            v.len()
        ))
        .into());
    }

    Ok(v)
}
