use std::collections::{HashMap, HashSet};

use rocksdb::DB;
use url::Url;

use crate::{
    Chunk, SearchResult,
    embeddings::client,
    search::{lexical::LexicalIndex, vector_index::VectorIndex},
    storage,
};

struct Candidate {
    result: SearchResult,
    overlap: f32,
}

pub fn run_query(
    db: &DB,
    index: &dyn VectorIndex,
    lexical: Option<&LexicalIndex>,
    query_text: &str,
    k: usize,
) -> Vec<SearchResult> {
    let query_vec = match client::embed(query_text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[query] embedding failed: {e}");
            return Vec::new();
        }
    };

    let vec_pool_k = (k.saturating_mul(100)).clamp(k, 2_000);
    let lex_pool_k = (k.saturating_mul(40)).clamp(k, 1_000);

    let vector_hits = index.search(&query_vec, vec_pool_k);
    let lexical_hits = lexical
        .and_then(|lx| lx.search(query_text, lex_pool_k).ok())
        .unwrap_or_default();

    let fused_ids = rrf_fuse_ids(&vector_hits, &lexical_hits, 2_000);
    let vec_scores = normalize_scores(&vector_hits);
    let lex_scores = normalize_scores(&lexical_hits);

    let chunks_cf = match storage::cf(db, storage::CF_CHUNKS) {
        Ok(cf) => cf,
        Err(_) => return Vec::new(),
    };

    let query_tokens = tokenize_set(query_text);
    let mut candidates = Vec::new();

    for chunk_id in fused_ids {
        if let Ok(Some(bytes)) = db.get_cf(chunks_cf, chunk_id.as_bytes()) {
            if let Ok(chunk) = serde_json::from_slice::<Chunk>(&bytes) {
                if !chunk.is_leaf {
                    continue;
                }

                let overlap = keyword_overlap(&query_tokens, &chunk);
                let vec_score = *vec_scores.get(&chunk.id).unwrap_or(&0.0);
                let lex_score = *lex_scores.get(&chunk.id).unwrap_or(&0.0);

                // Keep a compact, explainable formula.
                let rerank_score = (0.60 * vec_score) + (0.30 * lex_score) + (0.10 * overlap);

                candidates.push(Candidate {
                    result: SearchResult {
                        chunk_id: chunk.id,
                        score: rerank_score,
                        text: chunk.text,
                        source_url: chunk.source_url,
                        heading_chain: chunk.heading_chain,
                    },
                    overlap,
                });
            }
        }
    }

    candidates.sort_by(|a, b| b.result.score.total_cmp(&a.result.score));

    let mut selected = Vec::new();
    let mut seen_url_keys = HashSet::new();
    let mut seen_text_keys = HashSet::new();
    let mut per_host: HashMap<String, usize> = HashMap::new();
    let host_cap = (k / 2).max(2);

    fill_results(
        &mut selected,
        &mut seen_url_keys,
        &mut seen_text_keys,
        &mut per_host,
        host_cap,
        candidates.iter(),
        k,
        0.04,
    );

    if selected.len() < k {
        fill_results(
            &mut selected,
            &mut seen_url_keys,
            &mut seen_text_keys,
            &mut per_host,
            host_cap,
            candidates.iter(),
            k,
            0.0,
        );
    }

    selected
}

fn rrf_fuse_ids(
    vector_hits: &[(String, f32)],
    lexical_hits: &[(String, f32)],
    limit: usize,
) -> Vec<String> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    let k = 60.0_f32;

    for (rank, (id, _)) in vector_hits.iter().enumerate() {
        let rr = 1.0 / (k + rank as f32 + 1.0);
        *scores.entry(id.clone()).or_insert(0.0) += rr;
    }

    for (rank, (id, _)) in lexical_hits.iter().enumerate() {
        let rr = 1.0 / (k + rank as f32 + 1.0);
        *scores.entry(id.clone()).or_insert(0.0) += rr;
    }

    let mut ranked: Vec<(String, f32)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    ranked.truncate(limit);
    ranked.into_iter().map(|(id, _)| id).collect()
}

fn normalize_scores(hits: &[(String, f32)]) -> HashMap<String, f32> {
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

fn fill_results<'a>(
    selected: &mut Vec<SearchResult>,
    seen_url_keys: &mut HashSet<String>,
    seen_text_keys: &mut HashSet<String>,
    per_host: &mut HashMap<String, usize>,
    host_cap: usize,
    candidates: impl Iterator<Item = &'a Candidate>,
    k: usize,
    min_overlap: f32,
) {
    for c in candidates {
        if selected.len() >= k {
            break;
        }
        if c.overlap < min_overlap {
            continue;
        }

        let r = &c.result;
        let url_key = canonical_url_key(&r.source_url);
        if seen_url_keys.contains(&url_key) {
            continue;
        }

        let text_key = normalize_text_key(&r.text);
        if seen_text_keys.contains(&text_key) {
            continue;
        }

        if let Some(host) = url_host(&r.source_url) {
            let c = per_host.entry(host).or_insert(0);
            if *c >= host_cap {
                continue;
            }
            *c += 1;
        }

        seen_url_keys.insert(url_key);
        seen_text_keys.insert(text_key);
        selected.push(r.clone());
    }
}

fn keyword_overlap(query_tokens: &HashSet<String>, chunk: &Chunk) -> f32 {
    if query_tokens.is_empty() {
        return 0.0;
    }

    let mut combined = String::with_capacity(chunk.text.len() + 128);
    combined.push_str(&chunk.text);
    if !chunk.heading_chain.is_empty() {
        combined.push(' ');
        combined.push_str(&chunk.heading_chain.join(" "));
    }

    let tokens = tokenize_set(&combined);
    if tokens.is_empty() {
        return 0.0;
    }

    let matched = query_tokens.iter().filter(|t| tokens.contains(*t)).count() as f32;
    matched / (query_tokens.len() as f32)
}

fn tokenize_set(text: &str) -> HashSet<String> {
    const STOP: &[&str] = &[
        "the", "a", "an", "and", "or", "to", "of", "in", "on", "for", "with", "is", "are", "be",
        "how", "what", "why", "when", "from", "by", "as", "at", "it", "that", "this", "vs",
    ];

    let stop: HashSet<&str> = STOP.iter().copied().collect();
    let mut out = HashSet::new();
    let mut cur = String::new();

    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '+' {
            cur.push(ch.to_ascii_lowercase());
        } else if !cur.is_empty() {
            if cur.len() >= 2 && !stop.contains(cur.as_str()) {
                out.insert(cur.clone());
            }
            cur.clear();
        }
    }

    if !cur.is_empty() && cur.len() >= 2 && !stop.contains(cur.as_str()) {
        out.insert(cur);
    }

    out
}

fn canonical_url_key(url: &str) -> String {
    if let Ok(mut u) = Url::parse(url) {
        u.set_query(None);
        u.set_fragment(None);
        return u.to_string();
    }
    url.to_string()
}

fn normalize_text_key(text: &str) -> String {
    text.to_ascii_lowercase()
        .split_whitespace()
        .take(40)
        .collect::<Vec<_>>()
        .join(" ")
}

fn url_host(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
}
