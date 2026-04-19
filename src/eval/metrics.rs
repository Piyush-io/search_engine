use std::collections::HashMap;

use super::qrels::Qrel;

pub struct EvalResult {
    pub mrr: f64,
    pub ndcg_at: Vec<(usize, f64)>,
    pub recall_at: Vec<(usize, f64)>,
    pub num_queries: usize,
}

pub fn compute_metrics(
    ranked_lists: &HashMap<String, Vec<String>>,
    qrels: &HashMap<String, Vec<Qrel>>,
    k_values: &[usize],
) -> EvalResult {
    let mut mrr_sum = 0.0;
    let mut ndcg_sums: Vec<f64> = vec![0.0; k_values.len()];
    let mut recall_sums: Vec<f64> = vec![0.0; k_values.len()];
    let mut num_queries = 0;

    for query_id in ranked_lists.keys() {
        let ranked = &ranked_lists[query_id];
        let judged = match qrels.get(query_id) {
            Some(v) => v,
            None => continue,
        };

        let rel_map: HashMap<&str, u32> = judged
            .iter()
            .map(|q| (q.doc_id.as_str(), q.relevance))
            .collect();

        let total_relevant = rel_map.values().filter(|&&r| r > 0).count() as f64;

        let mut first_relevant_rank: Option<usize> = None;
        for (i, doc_id) in ranked.iter().enumerate() {
            if let Some(&rel) = rel_map.get(doc_id.as_str()) {
                if rel > 0 && first_relevant_rank.is_none() {
                    first_relevant_rank = Some(i);
                }
            }
        }

        if let Some(rank) = first_relevant_rank {
            mrr_sum += 1.0 / (rank as f64 + 1.0);
        }

        for (ki, &k) in k_values.iter().enumerate() {
            let dcg = dcg_at_k(ranked, &rel_map, k);
            let idcg = ideal_dcg_at_k(&rel_map, k);
            let ndcg = if idcg == 0.0 { 0.0 } else { dcg / idcg };
            ndcg_sums[ki] += ndcg;

            let mut hits = 0.0;
            for doc_id in ranked.iter().take(k) {
                if let Some(&rel) = rel_map.get(doc_id.as_str()) {
                    if rel > 0 {
                        hits += 1.0;
                    }
                }
            }
            let recall = if total_relevant > 0.0 {
                hits / total_relevant
            } else {
                0.0
            };
            recall_sums[ki] += recall;
        }

        num_queries += 1;
    }

    let mrr = if num_queries > 0 {
        mrr_sum / num_queries as f64
    } else {
        0.0
    };

    let ndcg_at: Vec<(usize, f64)> = k_values
        .iter()
        .enumerate()
        .map(|(i, &k)| {
            let v = if num_queries > 0 {
                ndcg_sums[i] / num_queries as f64
            } else {
                0.0
            };
            (k, v)
        })
        .collect();

    let recall_at: Vec<(usize, f64)> = k_values
        .iter()
        .enumerate()
        .map(|(i, &k)| {
            let v = if num_queries > 0 {
                recall_sums[i] / num_queries as f64
            } else {
                0.0
            };
            (k, v)
        })
        .collect();

    EvalResult {
        mrr,
        ndcg_at,
        recall_at,
        num_queries,
    }
}

fn dcg_at_k(ranked: &[String], rel_map: &HashMap<&str, u32>, k: usize) -> f64 {
    let mut sum = 0.0;
    for (i, doc_id) in ranked.iter().enumerate().take(k) {
        let rel = *rel_map.get(doc_id.as_str()).unwrap_or(&0) as f64;
        sum += ((2.0_f64).powf(rel) - 1.0) / (i as f64 + 2.0).log2();
    }
    sum
}

fn ideal_dcg_at_k(rel_map: &HashMap<&str, u32>, k: usize) -> f64 {
    let mut relevances: Vec<u32> = rel_map.values().copied().collect();
    relevances.sort_by(|a, b| b.cmp(a));
    let mut sum = 0.0;
    for (i, &rel) in relevances.iter().enumerate().take(k) {
        let rel = rel as f64;
        sum += ((2.0_f64).powf(rel) - 1.0) / (i as f64 + 2.0).log2();
    }
    sum
}
