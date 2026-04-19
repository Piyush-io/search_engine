use std::collections::HashMap;

use serde::Serialize;

use search_engine::eval::{compute_metrics, load_qrels, load_queries};

use search_engine::{
    config,
    embeddings::client,
    search::{bootstrap, query},
};

#[derive(Serialize)]
struct JsonReport {
    mrr: f64,
    ndcg_at: Vec<(usize, f64)>,
    recall_at: Vec<(usize, f64)>,
    num_queries: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let mut qrels_path = String::new();
    let mut queries_path = String::new();
    let mut k_values = vec![1, 3, 5, 10];

    loop {
        let arg = match args.next() {
            Some(a) => a,
            None => break,
        };
        match arg.as_str() {
            "--qrels" => qrels_path = args.next().unwrap(),
            "--queries" => queries_path = args.next().unwrap(),
            "--k" => {
                let k_str = args.next().unwrap();
                k_values = k_str
                    .split(',')
                    .filter_map(|s| s.trim().parse::<usize>().ok())
                    .collect();
            }
            _ => {}
        }
    }

    let qrels = load_qrels(&qrels_path)?;
    let queries = load_queries(&queries_path)?;

    let cfg = config::load()?;
    println!("[eval] {}", client::backend_info()?);

    let stack = bootstrap::load_search_stack()?;

    let top_k = *k_values.iter().max().unwrap_or(&10);
    let mut ranked_lists: HashMap<String, Vec<String>> = HashMap::new();

    for (query_id, query_text) in &queries {
        let results = query::run_query(
            &stack.db,
            stack.index.as_ref(),
            stack.lexical.as_deref(),
            query_text,
            top_k,
            &cfg.ranking,
        );
        let doc_ids: Vec<String> = results.iter().map(|r| r.source_url.clone()).collect();
        ranked_lists.insert(query_id.clone(), doc_ids);
    }

    let result = compute_metrics(&ranked_lists, &qrels, &k_values);

    println!("=== Evaluation Results ===");
    println!("Queries evaluated: {}", result.num_queries);
    println!("MRR: {:.4}", result.mrr);
    for (k, v) in &result.ndcg_at {
        println!("NDCG@{}: {:.4}", k, v);
    }
    for (k, v) in &result.recall_at {
        println!("Recall@{}: {:.4}", k, v);
    }

    let report = JsonReport {
        mrr: result.mrr,
        ndcg_at: result.ndcg_at,
        recall_at: result.recall_at,
        num_queries: result.num_queries,
    };

    std::fs::create_dir_all("reports")?;
    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write("reports/eval_results.json", json)?;
    println!("[eval] report written to reports/eval_results.json");

    Ok(())
}
