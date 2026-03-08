//! Direct-ORT bulk embedding engine used exclusively by the `embed` binary.
//!
//! Uses the same model files that fastembed cached in `.fastembed_cache`, but
//! builds each ORT session with an explicit intra-op thread count so we can
//! run N independent sessions in parallel without fighting fastembed's
//! `available_parallelism()` hard-coding.
//!
//! `client.rs` (fastembed singleton) remains unchanged for all online/query
//! paths.

use std::path::{Path, PathBuf};

use fastembed::{EmbeddingModel, TextEmbedding, TokenizerFiles, read_file_to_bytes};
use ndarray::{Array, Array2, s};
use ort::{
    session::{Session, builder::GraphOptimizationLevel},
    value::Value,
};
use tokenizers::{AddedToken, PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

use crate::EmbeddingVec;

// ── public types ─────────────────────────────────────────────────────────────

/// One worker: owns a tokenizer instance + one ORT session.
/// Both are `Send`, so the value can be moved to a worker thread.
pub struct BulkWorker {
    tokenizer: Tokenizer,
    session: Session,
    dim: usize,
    needs_token_type_ids: bool,
}

impl BulkWorker {
    /// Embed a batch of texts, returning one `Vec<f32>` per input.
    pub fn embed_batch(
        &self,
        texts: &[String],
    ) -> Result<Vec<EmbeddingVec>, Box<dyn std::error::Error + Send + Sync>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let inputs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let encodings = self
            .tokenizer
            .encode_batch(inputs, true)
            .map_err(|e| format!("tokenizer: {e}"))?;

        let batch = texts.len();
        let seq_len = encodings[0].len();
        let n = batch * seq_len;

        let mut ids_flat = Vec::with_capacity(n);
        let mut mask_flat = Vec::with_capacity(n);
        let mut type_ids_flat = Vec::with_capacity(n);

        for enc in &encodings {
            ids_flat.extend(enc.get_ids().iter().map(|&x| x as i64));
            mask_flat.extend(enc.get_attention_mask().iter().map(|&x| x as i64));
            type_ids_flat.extend(enc.get_type_ids().iter().map(|&x| x as i64));
        }

        let ids_arr = Array::from_shape_vec((batch, seq_len), ids_flat)?;
        let mask_arr: Array2<i64> = Array::from_shape_vec((batch, seq_len), mask_flat.clone())?;
        let type_ids_arr = Array::from_shape_vec((batch, seq_len), type_ids_flat)?;

        let mut session_inputs = ort::inputs![
            "input_ids"      => ids_arr,
            "attention_mask" => mask_arr.view(),
        ]?;

        if self.needs_token_type_ids {
            let type_val = Value::from_array(type_ids_arr)?.into_dyn();
            session_inputs.push(("token_type_ids".into(), type_val.into()));
        }

        let outputs = self.session.run(session_inputs)?;

        // Model output: [batch, seq_len, dim]  →  mean-pool  →  [batch, dim]
        let token_embeddings = outputs[0].try_extract_tensor::<f32>()?;
        let view = token_embeddings.view();

        let mask_arr2: Array2<i64> = Array::from_shape_vec(
            (batch, seq_len),
            mask_flat.iter().map(|&x| x as i64).collect(),
        )?;
        let pooled = mean_pool(&view, mask_arr2)?;

        let mut result = Vec::with_capacity(batch);
        for row in pooled.rows() {
            let v: Vec<f32> = row.to_vec();
            if v.len() != self.dim {
                return Err(format!(
                    "dim mismatch after pooling: expected {}, got {}",
                    self.dim,
                    v.len()
                )
                .into());
            }
            result.push(v);
        }
        Ok(result)
    }
}

// ── factory ───────────────────────────────────────────────────────────────────

/// Build `count` workers. Each gets its own ORT session with `intra_threads`
/// intra-op threads. Model weights are read from the fastembed cache on disk.
pub fn create_workers(
    model_name: &str,
    backend: &str,
    max_length: usize,
    dim: usize,
    count: usize,
    intra_threads: usize,
) -> Result<Vec<BulkWorker>, Box<dyn std::error::Error>> {
    let parsed = parse_model_name(model_name)?;

    let model_info =
        TextEmbedding::get_model_info(&parsed).map_err(|e| format!("model info: {e}"))?;

    // Resolve model and tokenizer files from fastembed's cache directory.
    // We scan the snapshots sub-directory rather than re-invoking hf-hub.
    let snapshot_dir = find_snapshot_dir(&model_info.model_code, &model_info.model_file)
        .ok_or_else(|| {
            format!(
                "model not found in fastembed cache (run a normal embed first to download it). \
                 Expected under: {}/models--{}/snapshots/",
                fastembed::get_cache_dir(),
                model_info.model_code.replace('/', "--"),
            )
        })?;

    let model_path = snapshot_dir.join(&model_info.model_file);

    let tokenizer_bytes = TokenizerFiles {
        tokenizer_file: read_file_bytes(&snapshot_dir, "tokenizer.json")?,
        config_file: read_file_bytes(&snapshot_dir, "config.json")?,
        special_tokens_map_file: read_file_bytes(&snapshot_dir, "special_tokens_map.json")?,
        tokenizer_config_file: read_file_bytes(&snapshot_dir, "tokenizer_config.json")?,
    };

    let mut workers = Vec::with_capacity(count);
    for _ in 0..count {
        let session = build_session(&model_path, intra_threads, backend)?;
        let needs_token_type_ids = session.inputs.iter().any(|i| i.name == "token_type_ids");
        let tokenizer = build_tokenizer(tokenizer_bytes.clone(), max_length)?;
        workers.push(BulkWorker {
            tokenizer,
            session,
            dim,
            needs_token_type_ids,
        });
    }

    Ok(workers)
}

// ── internals ────────────────────────────────────────────────────────────────

fn parse_model_name(raw: &str) -> Result<EmbeddingModel, Box<dyn std::error::Error>> {
    use std::str::FromStr;
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
        _ => None,
    };
    if let Some(m) = mapped {
        return Ok(m);
    }
    EmbeddingModel::from_str(raw).map_err(|_| format!("unknown model: {raw}").into())
}

/// Find the snapshot directory containing `model_file` under the fastembed cache.
fn find_snapshot_dir(model_code: &str, model_file: &str) -> Option<PathBuf> {
    let cache_root = fastembed::get_cache_dir();
    let repo_folder = format!("models--{}", model_code.replace('/', "--"));
    let snapshots_dir = Path::new(&cache_root).join(repo_folder).join("snapshots");

    // Alternatively the model may live in a flat layout directly under cache root
    // (some older fastembed versions): check there first.
    let flat_dir =
        Path::new(&cache_root).join(format!("models--{}", model_code.replace('/', "--")));
    if flat_dir.join(model_file).exists() {
        return Some(flat_dir);
    }

    // Normal HF-hub layout: models--org--name/snapshots/<hash>/...
    let Ok(entries) = std::fs::read_dir(&snapshots_dir) else {
        return None;
    };
    for entry in entries.flatten() {
        let candidate = entry.path().join(model_file);
        if candidate.exists() {
            return Some(entry.path());
        }
    }
    None
}

fn read_file_bytes(dir: &Path, name: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let path = dir.join(name);
    read_file_to_bytes(&path).map_err(|e| format!("read {name}: {e}").into())
}

fn build_session(
    model_path: &Path,
    intra_threads: usize,
    backend: &str,
) -> Result<Session, Box<dyn std::error::Error>> {
    let mut builder = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_intra_threads(intra_threads)?
        .with_inter_threads(1)?;

    #[cfg(not(target_os = "macos"))]
    if backend == "cuda" {
        use ort::execution_providers::CUDAExecutionProvider;
        builder = builder.with_execution_providers([CUDAExecutionProvider::default().build()])?;
    }

    Ok(builder.commit_from_file(model_path)?)
}

fn build_tokenizer(
    files: TokenizerFiles,
    max_length: usize,
) -> Result<Tokenizer, Box<dyn std::error::Error>> {
    use serde_json::Value as JsonValue;

    let config: JsonValue = serde_json::from_slice(&files.config_file)?;
    let special_tokens_map: JsonValue = serde_json::from_slice(&files.special_tokens_map_file)?;
    let tokenizer_config: JsonValue = serde_json::from_slice(&files.tokenizer_config_file)?;

    let model_max = tokenizer_config["model_max_length"]
        .as_f64()
        .unwrap_or(512.0) as usize;
    let capped = max_length.min(model_max);

    let pad_id = config["pad_token_id"].as_u64().unwrap_or(0) as u32;
    let pad_token = tokenizer_config["pad_token"]
        .as_str()
        .unwrap_or("[PAD]")
        .to_string();

    let mut tokenizer = Tokenizer::from_bytes(&files.tokenizer_file)
        .map_err(|e| format!("tokenizer parse: {e}"))?;

    let tokenizer = tokenizer
        .with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::BatchLongest,
            pad_token,
            pad_id,
            ..Default::default()
        }))
        .with_truncation(Some(TruncationParams {
            max_length: capped,
            ..Default::default()
        }))
        .map_err(|e| format!("tokenizer config: {e}"))?
        .clone();

    let mut tokenizer = tokenizer;
    if let JsonValue::Object(map) = special_tokens_map {
        for (_, v) in &map {
            if let Some(content) = v.as_str() {
                tokenizer.add_special_tokens(&[AddedToken {
                    content: content.into(),
                    special: true,
                    ..Default::default()
                }]);
            } else if let Some(content) = v.get("content").and_then(|c| c.as_str()) {
                tokenizer.add_special_tokens(&[AddedToken {
                    content: content.into(),
                    special: true,
                    single_word: v
                        .get("single_word")
                        .and_then(|x| x.as_bool())
                        .unwrap_or(false),
                    lstrip: v.get("lstrip").and_then(|x| x.as_bool()).unwrap_or(false),
                    rstrip: v.get("rstrip").and_then(|x| x.as_bool()).unwrap_or(false),
                    normalized: v
                        .get("normalized")
                        .and_then(|x| x.as_bool())
                        .unwrap_or(true),
                }]);
            }
        }
    }

    Ok(tokenizer.into())
}

fn mean_pool(
    token_embeddings: &ndarray::ArrayView<f32, ndarray::Dim<ndarray::IxDynImpl>>,
    attention_mask: Array2<i64>,
) -> Result<Array2<f32>, Box<dyn std::error::Error + Send + Sync>> {
    if token_embeddings.ndim() == 2 {
        let d = token_embeddings.dim();
        return Ok(token_embeddings
            .slice(s![.., ..])
            .to_owned()
            .into_shape_with_order((d[0], d[1]))?);
    }
    if token_embeddings.ndim() != 3 {
        return Err(format!("unexpected ORT output ndim: {}", token_embeddings.ndim()).into());
    }

    let te = token_embeddings.slice(s![.., .., ..]);
    let mask_f = attention_mask
        .insert_axis(ndarray::Axis(2))
        .broadcast(te.dim())
        .ok_or("mask broadcast failed")?
        .mapv(|x| x as f32);

    let sum = (&mask_f * &te).sum_axis(ndarray::Axis(1));
    let mask_sum = mask_f
        .sum_axis(ndarray::Axis(1))
        .mapv(|x| if x == 0.0 { 1.0 } else { x });
    Ok(sum / mask_sum)
}
