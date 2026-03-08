use std::collections::HashSet;
use std::time::Instant;

use rocksdb::IteratorMode;
use search_engine::{
    config,
    embeddings::client,
    search::{bruteforce::BruteForceIndex, hnsw::HnswIndex},
    storage,
};

const SAMPLE_QUERIES: &[&str] = &[
    "what is a B-tree",
    "time complexity of merge sort",
    "how does tcp three-way handshake work",
    "why is rust memory safe without gc",
    "hash table vs binary search tree",
    "difference between mutex and rwlock",
    "how does garbage collection work",
    "python asyncio gather vs wait",
    "what is dynamic programming",
    "dijkstra shortest path complexity",
    "how to avoid sql injection",
    "what is consistent hashing",
    "what is quorum in distributed systems",
    "difference between process and thread",
    "what is eventual consistency",
];

fn decode_vector(value: &[u8], dim: usize) -> Option<Vec<f32>> {
    if value.len() == dim * std::mem::size_of::<f32>() {
        let mut out = vec![0.0_f32; dim];
        for (i, slot) in out.iter_mut().enumerate() {
            let start = i * 4;
            let bytes = [
                value[start],
                value[start + 1],
                value[start + 2],
                value[start + 3],
            ];
            *slot = f32::from_le_bytes(bytes);
        }
        return Some(out);
    }

    if let Ok(vector_vec) = bincode::deserialize::<Vec<f32>>(value) {
        if vector_vec.len() == dim {
            return Some(vector_vec);
        }
    }

    None
}

fn percentile(sorted_ms: &[u128], p: f32) -> u128 {
    if sorted_ms.is_empty() {
        return 0;
    }

    let idx = ((sorted_ms.len() - 1) as f32 * p).round() as usize;
    sorted_ms[idx.min(sorted_ms.len() - 1)]
}

fn recall_at_k(gt: &[(String, f32)], ann: &[(String, f32)], k: usize) -> f32 {
    if k == 0 {
        return 0.0;
    }

    let gt_ids: HashSet<&str> = gt.iter().take(k).map(|(id, _)| id.as_str()).collect();
    if gt_ids.is_empty() {
        return 0.0;
    }

    let overlap = ann
        .iter()
        .take(k)
        .filter(|(id, _)| gt_ids.contains(id.as_str()))
        .count();

    overlap as f32 / gt_ids.len() as f32
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = config::load()?;
    let db = storage::open_db_read_only(&cfg.paths.db_path)?;
    let embeddings_cf = storage::cf(&db, storage::CF_EMBEDDINGS)?;

    let mut vectors: Vec<(String, Vec<f32>)> = Vec::new();
    for item in db.iterator_cf(embeddings_cf, IteratorMode::Start) {
        let (key, value) = item?;
        let id = String::from_utf8(key.to_vec())?;
        let Some(vec) = decode_vector(value.as_ref(), cfg.embedding.dim) else {
            continue;
        };
        vectors.push((id, vec));
    }

    if vectors.is_empty() {
        return Err("no embeddings found; run embed first".into());
    }

    println!("[bench_ann] loaded vectors={}", vectors.len());

    let build_start = Instant::now();
    let mut brute = BruteForceIndex::new(cfg.embedding.dim);
    for (id, vec) in &vectors {
        brute.insert(id.clone(), vec.clone());
    }
    let brute_build_ms = build_start.elapsed().as_millis();

    let build_start = Instant::now();
    let mut hnsw = HnswIndex::with_params(
        cfg.embedding.dim,
        cfg.hnsw.m,
        cfg.hnsw.ef_construction,
        cfg.hnsw.ef_search,
        cfg.hnsw.max_elements.max(vectors.len()),
    );
    for (id, vec) in &vectors {
        hnsw.insert(id.clone(), vec.clone());
    }
    let hnsw_build_ms = build_start.elapsed().as_millis();

    let mut rows = Vec::new();
    for ef in [10usize, 50, 100, 200] {
        hnsw.set_ef_search(ef);

        let mut brute_lat = Vec::new();
        let mut ann_lat = Vec::new();
        let mut recalls = Vec::new();

        for q in SAMPLE_QUERIES {
            let qv = client::embed(q)?;

            let t0 = Instant::now();
            let gt = brute.search(&qv, 10);
            brute_lat.push(t0.elapsed().as_millis());

            let t1 = Instant::now();
            let ann = hnsw.search(&qv, 10);
            ann_lat.push(t1.elapsed().as_millis());

            recalls.push(recall_at_k(&gt, &ann, 10));
        }

        brute_lat.sort_unstable();
        ann_lat.sort_unstable();

        let avg_recall = if recalls.is_empty() {
            0.0
        } else {
            recalls.iter().sum::<f32>() / recalls.len() as f32
        };

        rows.push(serde_json::json!({
            "ef_search": ef,
            "avg_recall_at_10": avg_recall,
            "bruteforce_latency_ms": {
                "p50": percentile(&brute_lat, 0.50),
                "p95": percentile(&brute_lat, 0.95)
            },
            "ann_latency_ms": {
                "p50": percentile(&ann_lat, 0.50),
                "p95": percentile(&ann_lat, 0.95)
            }
        }));

        println!(
            "[bench_ann] ef={} recall@10={:.3} ann_p50={}ms ann_p95={}ms",
            ef,
            avg_recall,
            percentile(&ann_lat, 0.50),
            percentile(&ann_lat, 0.95)
        );
    }

    std::fs::create_dir_all("reports")?;
    let report = serde_json::json!({
        "dataset": {
            "vectors": vectors.len(),
            "dim": cfg.embedding.dim,
            "queries": SAMPLE_QUERIES.len()
        },
        "build_ms": {
            "bruteforce": brute_build_ms,
            "hnsw": hnsw_build_ms
        },
        "results": rows
    });

    std::fs::write(
        "reports/bench_ann.json",
        serde_json::to_string_pretty(&report)?,
    )?;
    println!("[bench_ann] wrote reports/bench_ann.json");

    Ok(())
}
