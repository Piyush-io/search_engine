use std::sync::OnceLock;

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use rayon::prelude::*;
use sha2::{Digest, Sha256};

use crate::EmbeddingVec;

static MODEL: OnceLock<Option<TextEmbedding>> = OnceLock::new();

fn model() -> &'static Option<TextEmbedding> {
    MODEL.get_or_init(|| {
        // Fast path on CPU: quantized model + lower max token length.
        // Fallback hash embedder is kept for resilience.
        let opts = InitOptions::new(EmbeddingModel::AllMiniLML6V2)
            .with_show_download_progress(false);
        TextEmbedding::try_new(opts).ok()
    })
}

/// Embed a single text. Uses fastembed model when available.
pub fn embed(text: &str) -> EmbeddingVec {
    if let Some(m) = model() {
        if let Ok(vs) = m.embed(vec![text.to_string()], Some(1)) {
            if let Some(v) = vs.first() {
                return to_fixed_dim(v);
            }
        }
    }

    fallback_embed(text)
}

pub fn embed_batch(texts: &[String]) -> Vec<EmbeddingVec> {
    if texts.is_empty() {
        return Vec::new();
    }

    if let Some(m) = model() {
        // For dynamic quantized models, batch_size must cover the whole input slice.
        if let Ok(vs) = m.embed(texts.to_vec(), Some(texts.len())) {
            return vs.iter().map(|v| to_fixed_dim(v)).collect();
        }
    }

    texts.par_iter().map(|t| fallback_embed(t)).collect()
}

pub fn cosine_similarity(a: &EmbeddingVec, b: &EmbeddingVec) -> f32 {
    let mut dot = 0.0_f32;
    let mut an = 0.0_f32;
    let mut bn = 0.0_f32;

    for i in 0..768 {
        dot += a[i] * b[i];
        an += a[i] * a[i];
        bn += b[i] * b[i];
    }

    if an == 0.0 || bn == 0.0 {
        return 0.0;
    }

    dot / (an.sqrt() * bn.sqrt())
}

fn to_fixed_dim(v: &[f32]) -> EmbeddingVec {
    let mut out = [0.0_f32; 768];
    let n = v.len().min(768);
    out[..n].copy_from_slice(&v[..n]);
    l2_normalize(&mut out);
    out
}

fn fallback_embed(text: &str) -> EmbeddingVec {
    let mut out = [0.0_f32; 768];

    for token in text.split_whitespace() {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        let digest = hasher.finalize();

        let idx = ((digest[0] as usize) << 8 | digest[1] as usize) % 768;
        let sign = if digest[2] % 2 == 0 { 1.0 } else { -1.0 };
        out[idx] += sign;
    }

    l2_normalize(&mut out);
    out
}

fn l2_normalize(v: &mut EmbeddingVec) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}
