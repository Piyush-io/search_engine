use crate::{config::RankingConfig, embeddings::client};

use super::scoring::Candidate;

pub(super) fn apply_reranker(query: &str, candidates: &mut [Candidate], ranking: &RankingConfig) {
    let pool_size = ranking.rerank_pool_size.max(5).min(candidates.len());
    if pool_size == 0 {
        return;
    }

    // Sort temporarily to pick top heuristic candidates
    candidates.sort_by(|a, b| b.final_score.total_cmp(&a.final_score));

    let top_candidates: Vec<usize> = (0..pool_size).collect();
    let documents: Vec<String> = top_candidates
        .iter()
        .map(|&i| {
            let c = &candidates[i];
            if c.heading_chain.is_empty() {
                c.text.clone()
            } else {
                format!("{}\n{}", c.heading_chain.join(" > "), c.text)
            }
        })
        .collect();

    let (scores, indices) = match client::rerank(query, &documents, pool_size) {
        Ok(res) => res,
        Err(e) => {
            eprintln!("[query] rerank failed: {e}");
            return;
        }
    };

    if scores.len() != pool_size || indices.len() != pool_size {
        eprintln!(
            "[query] rerank returned {} scores / {} indices for {pool_size} docs",
            scores.len(),
            indices.len()
        );
        return;
    }

    // Min-max normalize reranker scores to [0, 1]
    let min_score = scores.iter().copied().fold(f32::INFINITY, f32::min);
    let max_score = scores.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let range = max_score - min_score;

    let weight = ranking.rerank_blend_weight.clamp(0.0, 1.0);

    // The server returns scores sorted by relevance with parallel indices.
    // scores[i] is the score for documents[indices[i]], which maps to
    // top_candidates[indices[i]].
    for (idx_in_pool, &doc_idx) in indices.iter().enumerate() {
        let rerank_raw = scores[idx_in_pool];
        let rerank_norm = if range > 1e-6 {
            (rerank_raw - min_score) / range
        } else {
            0.5
        };

        let candidate_idx = top_candidates[doc_idx];
        let c = &mut candidates[candidate_idx];
        c.final_score = c.final_score * (1.0 - weight) + rerank_norm * weight;
    }
}
