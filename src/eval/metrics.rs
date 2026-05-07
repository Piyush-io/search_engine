use std::collections::HashMap;

use super::qrels::Qrel;
use super::queries::QueryBucket;

/// Metrics for a specific query bucket
#[derive(Debug, Clone)]
pub struct BucketMetrics {
    pub bucket: QueryBucket,
    pub num_queries: usize,
    pub mrr: f64,
    pub ndcg_at: Vec<(usize, f64)>,
    pub recall_at: Vec<(usize, f64)>,
}

/// Simple bucket metrics for string-based bucket names
#[derive(Debug, Clone)]
pub struct SimpleBucketMetrics {
    pub bucket_name: String,
    pub num_queries: usize,
    pub mrr: f64,
    pub avg_ndcg: f64,
}

pub struct EvalResult {
    pub mrr: f64,
    pub ndcg_at: Vec<(usize, f64)>,
    pub recall_at: Vec<(usize, f64)>,
    pub num_queries: usize,
    pub zero_result_count: usize,
    pub bucket_metrics: Vec<BucketMetrics>,
}

/// Compute metrics with per-bucket breakdown
pub fn compute_metrics_with_buckets(
    ranked_lists: &HashMap<String, Vec<String>>,
    qrels: &HashMap<String, Vec<Qrel>>,
    query_buckets: &HashMap<String, QueryBucket>,
    k_values: &[usize],
) -> EvalResult {
    // First compute overall metrics
    let overall = compute_metrics(ranked_lists, qrels, k_values);

    // Compute per-bucket metrics
    let mut bucket_results: Vec<BucketMetrics> = Vec::new();

    // Group queries by bucket
    let mut bucket_queries: HashMap<QueryBucket, Vec<String>> = HashMap::new();
    for (query_id, bucket) in query_buckets {
        bucket_queries
            .entry(*bucket)
            .or_default()
            .push(query_id.clone());
    }

    // Compute metrics for each bucket
    for (bucket, query_ids) in bucket_queries {
        let bucket_ranked: HashMap<String, Vec<String>> = ranked_lists
            .iter()
            .filter(|(k, _)| query_ids.contains(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let bucket_qrels: HashMap<String, Vec<Qrel>> = qrels
            .iter()
            .filter(|(k, _)| query_ids.contains(*k))
            .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
            .collect();

        if !bucket_ranked.is_empty() {
            let metrics = compute_metrics(&bucket_ranked, &bucket_qrels, k_values);
            bucket_results.push(BucketMetrics {
                bucket,
                num_queries: metrics.num_queries,
                mrr: metrics.mrr,
                ndcg_at: metrics.ndcg_at,
                recall_at: metrics.recall_at,
            });
        }
    }

    EvalResult {
        mrr: overall.mrr,
        ndcg_at: overall.ndcg_at,
        recall_at: overall.recall_at,
        num_queries: overall.num_queries,
        zero_result_count: overall.zero_result_count,
        bucket_metrics: bucket_results,
    }
}

/// Compute metrics grouped by query buckets (simple string-based buckets)
pub fn compute_bucket_metrics(
    ranked_lists: &HashMap<String, Vec<String>>,
    qrels: &HashMap<String, Vec<Qrel>>,
    query_buckets: &HashMap<String, String>, // query_id -> bucket_name
    k_values: &[usize],
) -> Vec<SimpleBucketMetrics> {
    use std::collections::HashSet;

    // Group queries by bucket
    let mut bucket_queries: HashMap<String, HashSet<String>> = HashMap::new();
    for (query_id, bucket) in query_buckets {
        bucket_queries
            .entry(bucket.clone())
            .or_default()
            .insert(query_id.clone());
    }

    let mut results = Vec::new();

    for (bucket_name, query_ids) in bucket_queries {
        // Filter ranked_lists and qrels to only include this bucket's queries
        let bucket_ranked: HashMap<String, Vec<String>> = ranked_lists
            .iter()
            .filter(|(k, _)| query_ids.contains(*k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let bucket_qrels: HashMap<String, Vec<Qrel>> = qrels
            .iter()
            .filter(|(k, _)| query_ids.contains(*k))
            .map(|(k, v): (&String, &Vec<Qrel>)| {
                (k.clone(), v.iter().cloned().collect::<Vec<Qrel>>())
            })
            .collect();

        if bucket_ranked.is_empty() {
            continue;
        }

        let metrics = compute_metrics(&bucket_ranked, &bucket_qrels, k_values);

        let avg_ndcg = if !metrics.ndcg_at.is_empty() {
            metrics.ndcg_at.iter().map(|(_, v)| v).sum::<f64>() / metrics.ndcg_at.len() as f64
        } else {
            0.0
        };

        results.push(SimpleBucketMetrics {
            bucket_name,
            num_queries: metrics.num_queries,
            mrr: metrics.mrr,
            avg_ndcg,
        });
    }

    results
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
    let mut zero_result_count = 0;

    for query_id in ranked_lists.keys() {
        let ranked = &ranked_lists[query_id];
        let judged = match qrels.get(query_id) {
            Some(v) => v,
            None => continue,
        };

        // Count zero-result queries
        if ranked.is_empty() {
            zero_result_count += 1;
        }

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
        zero_result_count,
        bucket_metrics: Vec::new(),
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
