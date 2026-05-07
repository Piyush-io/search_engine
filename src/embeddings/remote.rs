use std::sync::OnceLock;

use crate::embeddings::{EmbedRequest, EmbedResponse, RerankRequest, RerankResponse};

/// Shared blocking HTTP client — built once, cloned cheaply.
fn shared_client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build shared reqwest client")
    })
}

/// Call a remote embed-server over HTTP (blocking).
pub fn request_embed(
    base_url: &str,
    texts: &[String],
    is_query: bool,
    expected_model: &str,
    expected_dim: usize,
) -> Result<Vec<Vec<f32>>, Box<dyn std::error::Error>> {
    let client = shared_client();

    let req = EmbedRequest {
        texts: texts.to_vec(),
        is_query,
    };

    let resp: EmbedResponse = client
        .post(format!("{base_url}/embed"))
        .json(&req)
        .send()
        .map_err(|e| std::io::Error::other(format!("embed request to {base_url} failed: {e}")))?
        .json()
        .map_err(|e| {
            std::io::Error::other(format!("embed response parse from {base_url} failed: {e}"))
        })?;

    if resp.dim != expected_dim {
        return Err(std::io::Error::other(format!(
            "remote embed dim mismatch at {base_url}: config expects {}, server '{}' returned {}",
            expected_dim, resp.model, resp.dim
        ))
        .into());
    }

    if resp.model != expected_model {
        return Err(std::io::Error::other(format!(
            "remote embed model mismatch at {base_url}: config expects '{}', server returned '{}'",
            expected_model, resp.model
        ))
        .into());
    }

    if resp.vectors.len() != texts.len() {
        return Err(std::io::Error::other(format!(
            "remote embed batch size mismatch at {base_url}: sent {}, got {} vectors",
            texts.len(),
            resp.vectors.len()
        ))
        .into());
    }

    for (idx, vector) in resp.vectors.iter().enumerate() {
        if vector.len() != resp.dim {
            return Err(std::io::Error::other(format!(
                "remote embed vector {} from {base_url} has dim {}, expected {}",
                idx,
                vector.len(),
                resp.dim
            ))
            .into());
        }
    }

    Ok(resp.vectors)
}

/// Call a remote embed-server rerank endpoint over HTTP (blocking).
pub fn request_rerank(
    base_url: &str,
    query: &str,
    documents: &[String],
    top_k: usize,
) -> Result<(Vec<f32>, Vec<usize>), Box<dyn std::error::Error>> {
    let client = shared_client();

    let req = RerankRequest {
        query: query.to_string(),
        documents: documents.to_vec(),
        top_k,
    };

    let resp: RerankResponse = client
        .post(format!("{base_url}/rerank"))
        .json(&req)
        .send()
        .map_err(|e| std::io::Error::other(format!("rerank request to {base_url} failed: {e}")))?
        .json()
        .map_err(|e| {
            std::io::Error::other(format!("rerank response parse from {base_url} failed: {e}"))
        })?;

    Ok((resp.scores, resp.indices))
}
