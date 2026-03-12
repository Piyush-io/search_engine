use std::collections::{HashMap, HashSet};

use rocksdb::DB;
use url::Url;

use crate::{
    Chunk, SearchResult,
    embeddings::client,
    search::{lexical::LexicalIndex, vector_index::VectorIndex},
    storage,
};

const SYNONYMS: &[(&[&str], &[&str])] = &[
    (&["js"], &["javascript"]),
    (&["javascript"], &["js"]),
    (&["ts"], &["typescript"]),
    (&["typescript"], &["ts"]),
    (&["cpp", "c++"], &["c++"]),
    (&["auth"], &["authentication"]),
    (&["authentication"], &["auth"]),
    (&["gpu"], &["cuda", "graphics"]),
    (&["cuda"], &["gpu"]),
    (&["db"], &["database"]),
    (&["database"], &["db"]),
    (&["ml"], &["machine learning"]),
    (&["ai"], &["artificial intelligence"]),
    (&["api"], &["interface", "endpoint"]),
    (&["oop"], &["object oriented"]),
    (&["fp"], &["functional programming"]),
    (&["os"], &["operating system"]),
    (&["cli"], &["command line"]),
    (&["regex"], &["regular expression"]),
    (&["async"], &["asynchronous"]),
    (&["sync"], &["synchronous"]),
];

fn expand_query_tokens(tokens: &HashSet<String>) -> HashSet<String> {
    let mut expanded = tokens.clone();
    for (triggers, expansions) in SYNONYMS {
        if triggers.iter().any(|t| tokens.contains(*t)) {
            for exp in *expansions {
                for word in exp.split_whitespace() {
                    if word.len() >= 2 {
                        expanded.insert(word.to_ascii_lowercase());
                    }
                }
            }
        }
    }
    expanded
}

fn build_expanded_query_text(original: &str, tokens: &HashSet<String>) -> String {
    let expanded = expand_query_tokens(tokens);
    let new_terms: Vec<&String> = expanded.iter().filter(|t| !tokens.contains(*t)).collect();
    if new_terms.is_empty() {
        return original.to_string();
    }
    format!(
        "{} {}",
        original,
        new_terms
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    )
}

struct Candidate {
    result: SearchResult,
    body_overlap: f32,
    heading_overlap: f32,
    title_overlap: f32,
    heading_match_count: usize,
    query_token_count: usize,
    exact_heading_phrase: bool,
    exact_body_phrase: bool,
}

pub fn run_query(
    db: &DB,
    index: &dyn VectorIndex,
    lexical: Option<&LexicalIndex>,
    query_text: &str,
    k: usize,
) -> Vec<SearchResult> {
    let query_vec = match client::embed_query(query_text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[query] embedding failed: {e}");
            return Vec::new();
        }
    };

    let vec_pool_k = (k.saturating_mul(100)).clamp(k, 2_000);
    let lex_pool_k = (k.saturating_mul(40)).clamp(k, 1_000);

    let vector_hits = index.search(&query_vec, vec_pool_k);

    let query_tokens = tokenize_set(query_text);
    let short_query = query_tokens.len() <= 5;
    let expanded_query = build_expanded_query_text(query_text, &query_tokens);

    let lexical_hits = lexical
        .and_then(|lx| lx.search(&expanded_query, lex_pool_k).ok())
        .unwrap_or_default();

    let fused_ids = rrf_fuse_ids(&vector_hits, &lexical_hits, 2_000, short_query);
    let vec_scores = normalize_scores(&vector_hits);
    let lex_scores = normalize_scores(&lexical_hits);

    let chunks_cf = match storage::cf(db, storage::CF_CHUNKS) {
        Ok(cf) => cf,
        Err(_) => return Vec::new(),
    };

    let query_phrase = normalize_phrase(query_text);
    let mut candidates = Vec::new();

    for chunk_id in fused_ids {
        if let Ok(Some(bytes)) = db.get_cf(chunks_cf, chunk_id.as_bytes()) {
            if let Ok(chunk) = serde_json::from_slice::<Chunk>(&bytes) {
                if !chunk.is_leaf {
                    continue;
                }

                let expanded_tokens = expand_query_tokens(&query_tokens);
                let body_overlap = token_overlap(&expanded_tokens, &chunk.text);
                let section_title = chunk.heading_chain.last().map(String::as_str).unwrap_or("");
                let title_overlap = token_overlap(&query_tokens, section_title);
                let heading_text = specific_heading_text(&chunk.heading_chain);
                let heading_overlap = token_overlap(&query_tokens, &heading_text);
                let heading_match_count = token_match_count(&query_tokens, &heading_text);
                let vec_score = *vec_scores.get(&chunk.id).unwrap_or(&0.0);
                let lex_score = *lex_scores.get(&chunk.id).unwrap_or(&0.0);
                let exact_heading_phrase = contains_phrase(&query_phrase, section_title)
                    || contains_phrase(&query_phrase, &heading_text);
                let exact_body_phrase = contains_phrase(&query_phrase, &chunk.text);

                // Short queries (≤5 tokens) emphasize lexical and title signals over vector.
                // Long queries balance vector and lexical more equally.
                let mut rerank_score = if short_query {
                    (0.25 * vec_score)
                        + (0.35 * lex_score)
                        + (0.22 * title_overlap)
                        + (0.12 * heading_overlap)
                        + (0.06 * body_overlap)
                } else {
                    (0.35 * vec_score)
                        + (0.20 * lex_score)
                        + (0.20 * title_overlap)
                        + (0.15 * heading_overlap)
                        + (0.10 * body_overlap)
                };

                if exact_heading_phrase {
                    rerank_score += 0.25;
                } else if exact_body_phrase {
                    rerank_score += 0.10;
                }

                if short_query && title_overlap == 0.0 && heading_overlap == 0.0 {
                    rerank_score *= 0.55;
                } else if short_query && title_overlap == 0.0 && heading_overlap < 0.34 {
                    rerank_score *= 0.78;
                }

                // Boost results from canonical authority domains for the queried tech
                if let Some(host) = url_host(&chunk.source_url) {
                    rerank_score += domain_authority_bonus(&query_tokens, &host);
                }

                candidates.push(Candidate {
                    result: SearchResult {
                        chunk_id: chunk.id,
                        score: rerank_score,
                        text: chunk.text,
                        source_url: chunk.source_url,
                        heading_chain: chunk.heading_chain,
                    },
                    body_overlap,
                    heading_overlap,
                    title_overlap,
                    heading_match_count,
                    query_token_count: query_tokens.len(),
                    exact_heading_phrase,
                    exact_body_phrase,
                });
            }
        }
    }

    candidates.sort_by(|a, b| b.result.score.total_cmp(&a.result.score));

    // Score floor: drop candidates below a minimum quality threshold.
    let score_floor = if let Some(top) = candidates.first() {
        (top.result.score * 0.15).max(0.12)
    } else {
        0.12
    };
    candidates.retain(|c| c.result.score >= score_floor);

    let mut selected = Vec::new();
    let mut seen_url_keys = HashSet::new();
    let mut seen_text_tokens = Vec::new();
    let mut per_host: HashMap<String, usize> = HashMap::new();
    let host_cap = (k / 4).clamp(2, 3);

    fill_results(
        &mut selected,
        &mut seen_url_keys,
        &mut seen_text_tokens,
        &mut per_host,
        host_cap,
        candidates.iter(),
        k,
        0.08,
        short_query,
        true,
    );

    if selected.len() < k {
        fill_results(
            &mut selected,
            &mut seen_url_keys,
            &mut seen_text_tokens,
            &mut per_host,
            host_cap,
            candidates.iter(),
            k,
            0.0,
            short_query,
            false,
        );
    }

    selected
}

fn rrf_fuse_ids(
    vector_hits: &[(String, f32)],
    lexical_hits: &[(String, f32)],
    limit: usize,
    short_query: bool,
) -> Vec<String> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    let k = 60.0_f32;

    // Short queries: trust lexical signal more (navigational intent).
    // Long queries: trust vector signal more (semantic intent).
    let (vec_weight, lex_weight) = if short_query {
        (0.6_f32, 1.8_f32)
    } else {
        (1.0_f32, 1.0_f32)
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
    seen_text_tokens: &mut Vec<HashSet<String>>,
    per_host: &mut HashMap<String, usize>,
    host_cap: usize,
    candidates: impl Iterator<Item = &'a Candidate>,
    k: usize,
    min_overlap: f32,
    short_query: bool,
    strict_structural: bool,
) {
    for c in candidates {
        if selected.len() >= k {
            break;
        }
        let structural_signal = c.heading_overlap.max(c.title_overlap);
        if short_query {
            let enough_heading_matches = c.heading_match_count >= 2
                || (c.query_token_count <= 2 && c.heading_match_count >= 1);
            if strict_structural {
                if structural_signal < min_overlap && !c.exact_heading_phrase {
                    continue;
                }
                if !c.exact_heading_phrase && !enough_heading_matches {
                    continue;
                }
            } else {
                let lexical_signal = c.body_overlap.max(structural_signal);
                if lexical_signal < min_overlap && !c.exact_heading_phrase && !c.exact_body_phrase {
                    continue;
                }
            }
        } else {
            let lexical_signal = c.body_overlap.max(structural_signal);
            if lexical_signal < min_overlap && !c.exact_heading_phrase && !c.exact_body_phrase {
                continue;
            }
        }

        let r = &c.result;
        let url_key = canonical_url_key(&r.source_url);
        if seen_url_keys.contains(&url_key) {
            continue;
        }

        let tokens = tokenize_set(&r.text);
        if tokens.is_empty() {
            continue;
        }

        let is_dup = seen_text_tokens.iter().any(|seen| {
            let intersection = seen.intersection(&tokens).count() as f32;
            let union = seen.union(&tokens).count() as f32;
            intersection / union > 0.8
        });

        if is_dup {
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
        seen_text_tokens.push(tokens);
        selected.push(r.clone());
    }
}

fn token_overlap(query_tokens: &HashSet<String>, text: &str) -> f32 {
    if query_tokens.is_empty() {
        return 0.0;
    }

    let tokens = tokenize_set(text);
    if tokens.is_empty() {
        return 0.0;
    }

    let matched = query_tokens.iter().filter(|t| tokens.contains(*t)).count() as f32;
    matched / (query_tokens.len() as f32)
}

fn token_match_count(query_tokens: &HashSet<String>, text: &str) -> usize {
    if query_tokens.is_empty() {
        return 0;
    }

    let tokens = tokenize_set(text);
    if tokens.is_empty() {
        return 0;
    }

    query_tokens.iter().filter(|t| tokens.contains(*t)).count()
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

fn normalize_phrase(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch.is_ascii_whitespace() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_phrase(query_phrase: &str, text: &str) -> bool {
    if query_phrase.is_empty() {
        return false;
    }
    normalize_phrase(text).contains(query_phrase)
}

fn canonical_url_key(url: &str) -> String {
    if let Ok(mut u) = Url::parse(url) {
        let host = u.host_str().map(|h| h.to_ascii_lowercase());
        if matches!(
            host.as_deref(),
            Some("doc.rust-lang.org") | Some("docs.rust-lang.org")
        ) {
            let path = u.path().trim_start_matches('/');
            let stripped = path
                .strip_prefix("beta/")
                .or_else(|| path.strip_prefix("stable/"))
                .or_else(|| path.strip_prefix("nightly/"))
                .unwrap_or(path);
            return format!("doc.rust-lang.org/{}", stripped);
        }
        u.set_query(None);
        u.set_fragment(None);
        return u.to_string();
    }
    url.to_string()
}

fn specific_heading_text(heading_chain: &[String]) -> String {
    match heading_chain {
        [] => String::new(),
        [only] => only.clone(),
        [_, rest @ ..] => rest.join(" "),
    }
}

fn url_host(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
}

fn domain_authority_bonus(query_tokens: &HashSet<String>, host: &str) -> f32 {
    const AUTHORITY: &[(&str, &[&str])] = &[
        ("rust", &["doc.rust-lang.org", "docs.rust-lang.org"]),
        ("python", &["docs.python.org"]),
        ("go", &["go.dev", "pkg.go.dev"]),
        ("java", &["docs.oracle.com"]),
        ("javascript", &["developer.mozilla.org"]),
        ("typescript", &["www.typescriptlang.org"]),
        ("linux", &["kernel.org", "man7.org"]),
        ("git", &["git-scm.com"]),
        ("sql", &["www.postgresql.org", "dev.mysql.com"]),
        ("http", &["developer.mozilla.org", "httpwg.org"]),
        ("css", &["developer.mozilla.org"]),
        ("html", &["developer.mozilla.org"]),
        ("haskell", &["www.haskell.org", "hackage.haskell.org"]),
        ("c++", &["en.cppreference.com"]),
        ("cpp", &["en.cppreference.com"]),
    ];

    for (keyword, canonical_hosts) in AUTHORITY {
        if query_tokens.contains(*keyword) {
            if canonical_hosts
                .iter()
                .any(|h| host == *h || host.ends_with(&format!(".{h}")))
            {
                return 0.08;
            }
        }
    }
    0.0
}
