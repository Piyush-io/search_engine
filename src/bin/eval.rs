use std::collections::HashMap;

use clap::Parser;
use serde::Serialize;

use search_engine::eval::{
    CandidateCounts, ProfilingData, QueryBucket, canonical_doc_key, compute_metrics_with_buckets,
    load_qrels, load_queries_with_buckets,
};

use search_engine::{
    config,
    embeddings::client,
    search::{SearchDiagnostics, bootstrap, run_query_with_diagnostics},
};

/// Offline retrieval evaluation for the search engine.
#[derive(Parser)]
#[command(name = "eval")]
struct Cli {
    /// Path to qrels TSV file
    #[arg(long, required = true)]
    qrels: String,

    /// Path to queries TSV file
    #[arg(long, required = true)]
    queries: String,

    /// Comma-separated k values (e.g. "1,3,5,10")
    #[arg(long, default_value = "1,3,5,10", value_delimiter = ',')]
    k: Vec<usize>,
}

#[derive(Serialize)]
struct JsonReport {
    mrr: f64,
    ndcg_at: Vec<(usize, f64)>,
    recall_at: Vec<(usize, f64)>,
    num_queries: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    zero_result_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    zero_result_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bucket_metrics: Option<Vec<BucketMetricEntry>>,
}

#[derive(Serialize)]
struct BucketMetricEntry {
    bucket: String,
    num_queries: usize,
    mrr: f64,
    ndcg_at: Vec<(usize, f64)>,
    recall_at: Vec<(usize, f64)>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let k_values = if cli.k.is_empty() {
        vec![1, 3, 5, 10]
    } else {
        cli.k
    };

    let qrels = load_qrels(&cli.qrels)?;
    let queries = load_queries_with_buckets(&cli.queries)?;

    let cfg = config::load()?;
    println!("[eval] config={}", config::config_path());
    println!("[eval] db_path={}", cfg.paths.db_path);
    println!("[eval] {}", client::backend_info()?);

    let stack = bootstrap::load_search_stack()?;

    let top_k = *k_values.iter().max().unwrap_or(&10);
    let mut ranked_lists: HashMap<String, Vec<String>> = HashMap::new();
    let mut query_bucket_map: HashMap<String, QueryBucket> = HashMap::new();
    let mut profiling_data = ProfilingData::new();

    for query in &queries {
        let start_time = std::time::Instant::now();

        let mut diagnostics = SearchDiagnostics::default();
        let results = run_query_with_diagnostics(
            &stack.db,
            stack.index.as_ref(),
            stack.lexical.as_deref(),
            &query.text,
            top_k,
            &cfg.ranking,
            Some(&mut diagnostics),
        );

        let elapsed_ms = start_time.elapsed().as_secs_f64() * 1000.0;

        let doc_ids: Vec<String> = results
            .iter()
            .map(|r| canonical_doc_key(&r.source_url))
            .collect();

        ranked_lists.insert(query.id.clone(), doc_ids.clone());
        query_bucket_map.insert(query.id.clone(), query.bucket);

        // Record profiling data
        profiling_data.add_query(
            query.id.clone(),
            query.text.clone(),
            query.bucket,
            elapsed_ms,
            doc_ids.len(),
            CandidateCounts {
                vector_hits: diagnostics.vector_candidates,
                lexical_hits: diagnostics.lexical_candidates,
                fused_candidates: diagnostics.fused_candidates,
                final_selected: diagnostics.final_selected,
            },
        );
    }

    // Compute summary statistics
    profiling_data.compute_summary();

    // Compute metrics with bucket breakdown
    let result = compute_metrics_with_buckets(&ranked_lists, &qrels, &query_bucket_map, &k_values);

    // Display results
    println!("=== Evaluation Results ===");
    println!("Queries evaluated: {}", result.num_queries);
    println!(
        "Zero-result queries: {} ({:.1}%)",
        result.zero_result_count,
        (result.zero_result_count as f64 / result.num_queries.max(1) as f64) * 100.0
    );
    println!("MRR: {:.4}", result.mrr);
    for (k, v) in &result.ndcg_at {
        println!("NDCG@{}: {:.4}", k, v);
    }
    for (k, v) in &result.recall_at {
        println!("Recall@{}: {:.4}", k, v);
    }

    // Display per-bucket metrics
    if !result.bucket_metrics.is_empty() {
        println!("\n=== Per-Bucket Metrics ===");

        // Sort buckets for consistent output
        let mut buckets = result.bucket_metrics.clone();
        buckets.sort_by_key(|m| format!("{:?}", m.bucket));

        for metrics in buckets {
            let bucket_name = format!("{:?}", metrics.bucket);
            println!("\n{}:", bucket_name);
            println!(
                "  Queries: {} | MRR: {:.4}",
                metrics.num_queries, metrics.mrr
            );
            if let Some((_, ndcg_3)) = metrics.ndcg_at.iter().find(|(k, _)| *k == 3) {
                println!("  NDCG@3: {:.4}", ndcg_3);
            }
        }
    }

    // Build bucket metrics for JSON report
    let bucket_metrics_for_json: Vec<BucketMetricEntry> = result
        .bucket_metrics
        .iter()
        .map(|metrics| BucketMetricEntry {
            bucket: format!("{:?}", metrics.bucket).to_lowercase(),
            num_queries: metrics.num_queries,
            mrr: metrics.mrr,
            ndcg_at: metrics.ndcg_at.clone(),
            recall_at: metrics.recall_at.clone(),
        })
        .collect();

    let zero_result_rate = if result.num_queries > 0 {
        Some(result.zero_result_count as f64 / result.num_queries as f64)
    } else {
        Some(0.0)
    };

    let report = JsonReport {
        mrr: result.mrr,
        ndcg_at: result.ndcg_at,
        recall_at: result.recall_at,
        num_queries: result.num_queries,
        zero_result_count: Some(result.zero_result_count),
        zero_result_rate,
        bucket_metrics: Some(bucket_metrics_for_json),
    };

    // Write main report
    std::fs::create_dir_all("reports")?;
    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write("reports/eval_results.json", json)?;
    println!("\n[eval] report written to reports/eval_results.json");

    // Write profiling/diagnostics report
    let diagnostics_json = serde_json::to_string_pretty(&profiling_data)?;
    std::fs::write("reports/eval_query_diagnostics.json", diagnostics_json)?;
    println!("[eval] diagnostics written to reports/eval_query_diagnostics.json");

    Ok(())
}
