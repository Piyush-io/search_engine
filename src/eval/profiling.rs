use serde::Serialize;

use super::queries::QueryBucket;

/// Per-query diagnostic information
#[derive(Debug, Clone, Serialize)]
pub struct QueryDiagnostics {
    pub query_id: String,
    pub query_text: String,
    #[serde(rename = "bucket")]
    pub bucket_name: String,
    pub elapsed_ms: f64,
    pub num_results: usize,
    pub candidates: CandidateCounts,
}

/// Candidate counts at different stages of query execution
#[derive(Debug, Clone, Serialize, Default)]
pub struct CandidateCounts {
    pub vector_hits: usize,
    pub lexical_hits: usize,
    pub fused_candidates: usize,
    pub final_selected: usize,
}

/// Complete profiling data for an evaluation run
#[derive(Debug, Clone, Serialize)]
pub struct ProfilingData {
    pub queries: Vec<QueryDiagnostics>,
    pub summary: ProfilingSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfilingSummary {
    pub total_queries: usize,
    pub zero_result_queries: usize,
    pub avg_elapsed_ms: f64,
    pub p50_elapsed_ms: f64,
    pub p95_elapsed_ms: f64,
    pub p99_elapsed_ms: f64,
}

impl ProfilingData {
    pub fn new() -> Self {
        Self {
            queries: Vec::new(),
            summary: ProfilingSummary {
                total_queries: 0,
                zero_result_queries: 0,
                avg_elapsed_ms: 0.0,
                p50_elapsed_ms: 0.0,
                p95_elapsed_ms: 0.0,
                p99_elapsed_ms: 0.0,
            },
        }
    }

    pub fn add_query(
        &mut self,
        query_id: String,
        query_text: String,
        bucket: QueryBucket,
        elapsed_ms: f64,
        num_results: usize,
        candidates: CandidateCounts,
    ) {
        self.queries.push(QueryDiagnostics {
            query_id,
            query_text,
            bucket_name: bucket.as_str().to_string(),
            elapsed_ms,
            num_results,
            candidates,
        });
    }

    pub fn compute_summary(&mut self) {
        let total = self.queries.len();
        if total == 0 {
            return;
        }

        let zero_results = self.queries.iter().filter(|q| q.num_results == 0).count();

        let mut elapsed_times: Vec<f64> = self.queries.iter().map(|q| q.elapsed_ms).collect();
        elapsed_times.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let avg = elapsed_times.iter().sum::<f64>() / total as f64;
        let p50 = percentile(&elapsed_times, 50.0);
        let p95 = percentile(&elapsed_times, 95.0);
        let p99 = percentile(&elapsed_times, 99.0);

        self.summary = ProfilingSummary {
            total_queries: total,
            zero_result_queries: zero_results,
            avg_elapsed_ms: avg,
            p50_elapsed_ms: p50,
            p95_elapsed_ms: p95,
            p99_elapsed_ms: p99,
        };
    }
}

fn percentile(sorted_data: &[f64], p: f64) -> f64 {
    if sorted_data.is_empty() {
        return 0.0;
    }
    if p <= 0.0 {
        return sorted_data[0];
    }
    if p >= 100.0 {
        return sorted_data[sorted_data.len() - 1];
    }

    let index = (p / 100.0) * (sorted_data.len() - 1) as f64;
    let lower = index.floor() as usize;
    let upper = index.ceil() as usize;
    let fraction = index - lower as f64;

    if lower == upper {
        sorted_data[lower]
    } else {
        sorted_data[lower] * (1.0 - fraction) + sorted_data[upper] * fraction
    }
}
