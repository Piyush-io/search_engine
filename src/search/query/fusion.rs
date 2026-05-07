use std::collections::HashMap;

use crate::config::RankingConfig;

/// RRF (Reciprocal Rank Fusion) across vector and lexical result sets.
pub(super) fn rrf_fuse_ids(
    vector_hits: &[(String, f32)],
    lexical_hits: &[(String, f32)],
    limit: usize,
    short_query: bool,
    ranking: &RankingConfig,
) -> Vec<String> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    let k = ranking.rrf_k;

    let (vec_weight, lex_weight) = if short_query {
        (ranking.short_rrf_vec_weight, ranking.short_rrf_lex_weight)
    } else {
        (ranking.long_rrf_vec_weight, ranking.long_rrf_lex_weight)
    };

    for (rank, (id, _)) in vector_hits.iter().enumerate() {
        let rr = vec_weight / (k + rank as f32 + 1.0);
        *scores.entry(id.clone()).or_insert(0.0) += rr;
    }

    for (rank, (id, _)) in lexical_hits.iter().enumerate() {
        let rr = lex_weight / (k + rank as f32 + 1.0);
        *scores.entry(id.clone()).or_insert(0.0) += rr;
    }

    let mut ranked: Vec<(String, f32)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    ranked.truncate(limit);
    ranked.into_iter().map(|(id, _)| id).collect()
}

/// Min-max normalize raw scores from a search backend.
pub(super) fn normalize_scores(hits: &[(String, f32)]) -> HashMap<String, f32> {
    let mut out = HashMap::new();
    if hits.is_empty() {
        return out;
    }

    let max_score = hits
        .iter()
        .map(|(_, s)| *s)
        .fold(f32::MIN, |acc, s| acc.max(s));

    if max_score <= 0.0 {
        for (id, _) in hits {
            out.insert(id.clone(), 0.0);
        }
        return out;
    }

    for (id, s) in hits {
        out.insert(id.clone(), (*s / max_score).clamp(0.0, 1.0));
    }

    out
}
