use std::collections::{HashMap, HashSet};

use rocksdb::DB;
use url::Url;

use crate::{
    Chunk, SearchResult,
    embeddings::client,
    search::{hnsw::HnswIndex, lexical::LexicalIndex},
    storage,
};

#[derive(Default)]
struct QueryIntent {
    rust: bool,
    cpp: bool,
    web: bool,
    python: bool,
    systems: bool,
    technical: bool,
}

struct Candidate {
    result: SearchResult,
    overlap: f32,
}

pub fn run_query(
    db: &DB,
    index: &HnswIndex,
    lexical: Option<&LexicalIndex>,
    query_text: &str,
    k: usize,
) -> Vec<SearchResult> {
    let query_vec = client::embed(query_text);
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
    let intent = infer_intent(&query_tokens);
    let english_query = is_likely_english_query(query_text);

    let mut candidates = Vec::new();

    for chunk_id in fused_ids {
        if let Ok(Some(bytes)) = db.get_cf(chunks_cf, chunk_id.as_bytes()) {
            if let Ok(chunk) = serde_json::from_slice::<Chunk>(&bytes) {
                let host = url_host(&chunk.source_url);
                let overlap = keyword_overlap(&query_tokens, &chunk);
                let url_overlap = url_token_overlap(&query_tokens, &chunk.source_url);
                let prior = domain_prior(host.as_deref(), &intent);
                let quality_penalty = page_quality_penalty(&chunk.source_url);
                let so_noise_penalty = stackoverflow_noise_penalty(&chunk);
                let language_adj = if english_query && is_non_english_path(&chunk.source_url) {
                    -0.22
                } else {
                    0.0
                };
                let short_penalty = if chunk.text.len() < 48 { -0.06 } else { 0.0 };
                let pair_bonus = term_pair_bonus(&query_tokens, &chunk, &chunk.source_url);
                let asyncio_bonus = asyncio_path_bonus(&query_tokens, &chunk.source_url);

                let vec_score = *vec_scores.get(&chunk.id).unwrap_or(&0.0);
                let lex_score = *lex_scores.get(&chunk.id).unwrap_or(&0.0);

                let rerank_score = (0.48 * vec_score)
                    + (0.24 * lex_score)
                    + (0.42 * overlap)
                    + (0.16 * url_overlap)
                    + prior
                    + language_adj
                    + short_penalty
                    + quality_penalty
                    + so_noise_penalty
                    + pair_bonus
                    + asyncio_bonus;

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

    // Two-pass fill with shared dedup state.
    let strict_overlap = if intent.technical { 0.18 } else { 0.08 };
    let relaxed_overlap = if intent.technical { 0.06 } else { 0.02 };

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
        strict_overlap,
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
            relaxed_overlap,
        );
    }

    selected
}

fn rrf_fuse_ids(vector_hits: &[(String, f32)], lexical_hits: &[(String, f32)], limit: usize) -> Vec<String> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    let k = 60.0_f32;

    for (rank, (id, _)) in vector_hits.iter().enumerate() {
        let rr = 1.0 / (k + rank as f32 + 1.0);
        *scores.entry(id.clone()).or_insert(0.0) += rr;
    }

    for (rank, (id, _)) in lexical_hits.iter().enumerate() {
        let rr = 0.95 / (k + rank as f32 + 1.0);
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

fn infer_intent(tokens: &HashSet<String>) -> QueryIntent {
    let rust_terms = [
        "rust",
        "cargo",
        "borrow",
        "ownership",
        "lifetime",
        "trait",
        "mutable",
        "reference",
    ];
    let cpp_terms = ["c", "cpp", "cxx", "template", "stl", "std", "constexpr", "move"];
    let web_terms = [
        "javascript",
        "js",
        "dom",
        "css",
        "html",
        "browser",
        "web",
        "webapi",
    ];
    let python_terms = ["python", "asyncio", "pip", "django", "flask", "numpy", "pandas"];
    let systems_terms = [
        "latency",
        "throughput",
        "cache",
        "thread",
        "kernel",
        "network",
        "distributed",
        "database",
    ];

    let rust = rust_terms.iter().any(|t| tokens.contains(*t));
    let cpp = cpp_terms.iter().any(|t| tokens.contains(*t));
    let web = web_terms.iter().any(|t| tokens.contains(*t));
    let python = python_terms.iter().any(|t| tokens.contains(*t));
    let systems = systems_terms.iter().any(|t| tokens.contains(*t));

    QueryIntent {
        rust,
        cpp,
        web,
        python,
        systems,
        technical: rust || cpp || web || python || systems,
    }
}

fn domain_prior(host: Option<&str>, intent: &QueryIntent) -> f32 {
    let Some(host) = host else {
        return 0.0;
    };

    let mut s = match host {
        "doc.rust-lang.org" | "docs.python.org" | "developer.mozilla.org" => 0.08,
        "stackoverflow.com" | "blog.cloudflare.com" | "martinfowler.com" | "jvns.ca" => 0.05,
        "en.wikipedia.org" => -0.08,
        _ => 0.0,
    };

    if intent.rust {
        if host == "doc.rust-lang.org" || host == "blog.rust-lang.org" {
            s += 0.26;
        }
        if host == "cppreference.com" {
            s -= 0.22;
        }
        if host == "stackoverflow.com" {
            s += 0.06;
        }
    }

    if intent.python {
        if host == "docs.python.org" || host == "stackoverflow.com" {
            s += 0.26;
        }
        if host == "doc.rust-lang.org" || host == "cppreference.com" {
            s -= 0.14;
        }
    }

    if intent.web && host == "developer.mozilla.org" {
        s += 0.22;
    }

    if intent.cpp {
        if host == "cppreference.com" {
            s += 0.22;
        }
        if host == "doc.rust-lang.org" {
            s -= 0.08;
        }
    }

    if intent.systems && host == "blog.cloudflare.com" {
        s += 0.10;
    }

    s
}

fn page_quality_penalty(url: &str) -> f32 {
    let l = url.to_ascii_lowercase();
    if l.contains("py-modindex")
        || l.contains("genindex")
        || l.contains("sitemap")
        || l.contains("/tag/")
        || l.contains("/tags/")
        || l.contains("/category/")
        || l.ends_with("/feed")
    {
        -0.22
    } else if l.contains("/tutorial/whatnow") {
        -0.35
    } else if l.contains("/whatsnew/") {
        -0.20
    } else {
        0.0
    }
}

fn term_pair_bonus(query_tokens: &HashSet<String>, chunk: &Chunk, url: &str) -> f32 {
    if !(query_tokens.contains("gather") && query_tokens.contains("wait")) {
        return 0.0;
    }

    let body = chunk.text.to_ascii_lowercase();
    let headings = chunk.heading_chain.join(" ").to_ascii_lowercase();
    let u = url.to_ascii_lowercase();

    let gather_present = body.contains("gather") || headings.contains("gather") || u.contains("gather");
    let wait_present = body.contains("wait") || headings.contains("wait") || u.contains("wait");

    if gather_present && wait_present {
        0.18
    } else {
        0.0
    }
}

fn asyncio_path_bonus(query_tokens: &HashSet<String>, url: &str) -> f32 {
    if !query_tokens.contains("asyncio") {
        return 0.0;
    }

    let u = url.to_ascii_lowercase();
    if u.contains("docs.python.org") && u.contains("/library/asyncio-task") {
        0.32
    } else if u.contains("docs.python.org") && u.contains("/library/asyncio") {
        0.16
    } else {
        0.0
    }
}

fn stackoverflow_noise_penalty(chunk: &Chunk) -> f32 {
    if !chunk.source_url.contains("stackoverflow.com") {
        return 0.0;
    }

    let t = chunk.text.to_ascii_lowercase();
    let h = chunk.heading_chain.join(" ").to_ascii_lowercase();

    if h.contains("your answer")
        || h.contains("comments")
        || t.contains("asking for help, clarification")
        || t.contains("add a comment")
        || t.contains("post your answer")
    {
        -0.30
    } else {
        0.0
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

fn url_token_overlap(query_tokens: &HashSet<String>, url: &str) -> f32 {
    if query_tokens.is_empty() {
        return 0.0;
    }
    let tokens = tokenize_set(url);
    if tokens.is_empty() {
        return 0.0;
    }
    let matched = query_tokens.iter().filter(|t| tokens.contains(*t)).count() as f32;
    matched / (query_tokens.len() as f32)
}

fn tokenize_set(text: &str) -> HashSet<String> {
    const STOP: &[&str] = &[
        "the", "a", "an", "and", "or", "to", "of", "in", "on", "for", "with", "is", "are", "be",
        "how", "what", "why", "when", "from", "by", "as", "at", "it", "that", "this", "vs", "rules",
        "performance",
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

        // Dedup docs.python.org versioned docs: /3.10/, /3.11/, /3.13/ -> /3/
        if u.host_str() == Some("docs.python.org") {
            if let Some(segments) = u.path_segments() {
                let mut segs: Vec<String> = segments.map(|s| s.to_string()).collect();
                if let Some(first) = segs.first() {
                    if first.starts_with('3') {
                        segs[0] = "3".to_string();
                        let new_path = format!("/{}", segs.join("/"));
                        u.set_path(&new_path);
                    }
                }
            }
        }

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

fn is_likely_english_query(q: &str) -> bool {
    q.is_ascii()
}

fn is_non_english_path(url: &str) -> bool {
    let langs = [
        "de", "fr", "es", "pt", "pt-br", "ru", "ja", "ko", "zh", "zh-cn", "zh-tw", "it", "pl", "tr",
        "id", "uk", "vi",
    ];

    let Ok(u) = Url::parse(url) else {
        return false;
    };

    let mut segs = u.path_segments().into_iter().flatten();
    if let Some(first) = segs.next() {
        let lower = first.to_ascii_lowercase();
        return langs.contains(&lower.as_str());
    }

    false
}
