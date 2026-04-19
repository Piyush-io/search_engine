use std::collections::{HashMap, HashSet};

use rocksdb::DB;
use url::Url;

use crate::{
    config::RankingConfig,
    embeddings::client,
    search::{lexical::LexicalIndex, vector_index::VectorIndex},
    storage, Chunk, ChunkId, ScoredHit,
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
    chunk_id: ChunkId,
    text: String,
    source_url: String,
    heading_chain: Vec<String>,
    final_score: f32,
    vector_score: f32,
    lexical_score: f32,
    title_overlap: f32,
    heading_overlap: f32,
    body_overlap: f32,
    authority_bonus_val: f32,
    exact_heading_phrase: bool,
    exact_body_phrase: bool,
    heading_match_count: usize,
    overall_match_count: usize,
    query_token_count: usize,
}

pub fn run_query(
    db: &DB,
    index: &dyn VectorIndex,
    lexical: Option<&LexicalIndex>,
    query_text: &str,
    k: usize,
    ranking: &RankingConfig,
) -> Vec<ScoredHit> {
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
    let expanded_tokens = expand_query_tokens(&query_tokens);
    let expanded_query = build_expanded_query_text(query_text, &query_tokens);

    let lexical_hits = lexical
        .and_then(|lx| lx.search(&expanded_query, lex_pool_k).ok())
        .unwrap_or_default();

    let fused_ids = rrf_fuse_ids(&vector_hits, &lexical_hits, 2_000, short_query, ranking);
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

                let body_overlap = token_overlap(&expanded_tokens, &chunk.text);
                let page_title = chunk
                    .page_title
                    .as_deref()
                    .or_else(|| chunk.heading_chain.first().map(String::as_str))
                    .unwrap_or("");
                let section_title = chunk.heading_chain.last().map(String::as_str).unwrap_or("");
                let title_overlap = token_overlap(&query_tokens, page_title);
                let heading_text = specific_heading_text(&chunk.heading_chain);
                let heading_overlap = token_overlap(&query_tokens, &heading_text);
                let heading_match_count = token_match_count(&query_tokens, &heading_text);
                let overall_match_count = combined_token_match_count(
                    &query_tokens,
                    &[page_title, &heading_text, &chunk.text],
                );
                let vec_score = *vec_scores.get(&chunk.id).unwrap_or(&0.0);
                let lex_score = *lex_scores.get(&chunk.id).unwrap_or(&0.0);
                let exact_heading_phrase = contains_phrase(&query_phrase, page_title)
                    || contains_phrase(&query_phrase, section_title)
                    || contains_phrase(&query_phrase, &heading_text);
                let exact_body_phrase = contains_phrase(&query_phrase, &chunk.text);

                let mut rerank_score = if short_query {
                    (ranking.short_vec_weight * vec_score)
                        + (ranking.short_lex_weight * lex_score)
                        + (ranking.short_title_weight * title_overlap)
                        + (ranking.short_heading_weight * heading_overlap)
                        + (ranking.short_body_weight * body_overlap)
                } else {
                    (ranking.long_vec_weight * vec_score)
                        + (ranking.long_lex_weight * lex_score)
                        + (ranking.long_title_weight * title_overlap)
                        + (ranking.long_heading_weight * heading_overlap)
                        + (ranking.long_body_weight * body_overlap)
                };

                if exact_heading_phrase {
                    rerank_score += ranking.exact_heading_boost;
                } else if exact_body_phrase {
                    rerank_score += ranking.exact_body_boost;
                }

                if short_query && title_overlap == 0.0 && heading_overlap == 0.0 {
                    rerank_score *= ranking.no_heading_penalty;
                } else if short_query && title_overlap == 0.0 && heading_overlap < 0.34 {
                    rerank_score *= ranking.weak_heading_penalty;
                }

                let auth_bonus = if let Some(host) = url_host(&chunk.source_url) {
                    domain_authority_bonus(&query_tokens, &host, ranking.authority_bonus)
                } else {
                    0.0
                };
                rerank_score += auth_bonus;

                candidates.push(Candidate {
                    chunk_id: chunk.id,
                    text: chunk.text,
                    source_url: chunk.source_url,
                    heading_chain: chunk.heading_chain,
                    final_score: rerank_score,
                    vector_score: vec_score,
                    lexical_score: lex_score,
                    title_overlap,
                    heading_overlap,
                    body_overlap,
                    authority_bonus_val: auth_bonus,
                    exact_heading_phrase,
                    exact_body_phrase,
                    heading_match_count,
                    overall_match_count,
                    query_token_count: query_tokens.len(),
                });
            }
        }
    }

    candidates.sort_by(|a, b| b.final_score.total_cmp(&a.final_score));

    let score_floor = if let Some(top) = candidates.first() {
        (top.final_score * 0.15).max(0.12)
    } else {
        0.12
    };
    candidates.retain(|c| c.final_score >= score_floor);

    let mut selected: Vec<ScoredHit> = Vec::new();
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
    ranking: &RankingConfig,
) -> Vec<String> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    let k = ranking.rrf_k;

    let (vec_weight, lex_weight) = if short_query {
        (ranking.short_rrf_vec_weight, ranking.short_rrf_lex_weight)
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
    selected: &mut Vec<ScoredHit>,
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
        let enough_total_matches = c.overall_match_count >= 2;
        if short_query {
            let enough_heading_matches = c.heading_match_count >= 2
                || (c.query_token_count <= 2 && c.heading_match_count >= 1);
            if strict_structural {
                if structural_signal < min_overlap && !c.exact_heading_phrase {
                    continue;
                }
                if !c.exact_heading_phrase && !enough_heading_matches && !enough_total_matches {
                    continue;
                }
            } else {
                let lexical_signal = c.body_overlap.max(structural_signal);
                if lexical_signal < min_overlap && !c.exact_heading_phrase && !c.exact_body_phrase {
                    continue;
                }
                if !c.exact_heading_phrase && !c.exact_body_phrase && !enough_total_matches {
                    continue;
                }
            }
        } else {
            let lexical_signal = c.body_overlap.max(structural_signal);
            if lexical_signal < min_overlap && !c.exact_heading_phrase && !c.exact_body_phrase {
                continue;
            }
        }

        let url_key = canonical_url_key(&c.source_url);
        if seen_url_keys.contains(&url_key) {
            continue;
        }

        let tokens = tokenize_set(&c.text);
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

        if let Some(host) = url_host(&c.source_url) {
            let cnt = per_host.entry(host).or_insert(0);
            if *cnt >= host_cap {
                continue;
            }
            *cnt += 1;
        }

        seen_url_keys.insert(url_key);
        seen_text_tokens.push(tokens);
        selected.push(ScoredHit {
            chunk_id: c.chunk_id.clone(),
            text: c.text.clone(),
            source_url: c.source_url.clone(),
            heading_chain: c.heading_chain.clone(),
            vector_score: c.vector_score,
            lexical_score: c.lexical_score,
            title_overlap: c.title_overlap,
            heading_overlap: c.heading_overlap,
            body_overlap: c.body_overlap,
            authority_bonus: c.authority_bonus_val,
            exact_heading_phrase: c.exact_heading_phrase,
            exact_body_phrase: c.exact_body_phrase,
            final_score: c.final_score,
        });
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

fn combined_token_match_count(query_tokens: &HashSet<String>, texts: &[&str]) -> usize {
    if query_tokens.is_empty() {
        return 0;
    }

    let mut combined_tokens = HashSet::new();
    for text in texts {
        combined_tokens.extend(tokenize_set(text));
    }

    query_tokens
        .iter()
        .filter(|token| combined_tokens.contains(*token))
        .count()
}

fn tokenize_set(text: &str) -> HashSet<String> {
    const STOP: &[&str] = &[
        "the", "a", "an", "and", "or", "to", "of", "in", "on", "for", "with", "is", "are", "be",
        "how", "what", "why", "when", "from", "by", "as", "at", "it", "that", "this", "vs",
    ];

    let stop: HashSet<&str> = STOP.iter().copied().collect();
    let mut out = HashSet::new();
    let mut cur = String::new();

    for raw_piece in text.split_whitespace() {
        let compact = compact_token(raw_piece);
        maybe_insert_token(&mut out, &stop, compact);
    }

    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '+' {
            cur.push(ch.to_ascii_lowercase());
        } else if !cur.is_empty() {
            maybe_insert_token(&mut out, &stop, std::mem::take(&mut cur));
            cur.clear();
        }
    }

    if !cur.is_empty() {
        maybe_insert_token(&mut out, &stop, cur);
    }

    let normalized = normalize_phrase(text);
    if normalized.contains("b tree") || text.to_ascii_lowercase().contains("btree") {
        out.insert("btree".to_string());
    }

    out
}

fn compact_token(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn maybe_insert_token(tokens: &mut HashSet<String>, stop: &HashSet<&str>, token: String) {
    if token.len() >= 2 && !stop.contains(token.as_str()) {
        tokens.insert(token);
    }
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

fn canonical_host(host: &str) -> String {
    let host = host.to_ascii_lowercase();
    if matches!(host.as_str(), "doc.rust-lang.org" | "docs.rust-lang.org") {
        return "doc.rust-lang.org".to_string();
    }
    host.strip_prefix("www.").unwrap_or(host.as_str()).to_string()
}

fn canonical_url_key(url: &str) -> String {
    if let Ok(mut u) = Url::parse(url) {
        let host = u.host_str().map(canonical_host);
        if matches!(host.as_deref(), Some("doc.rust-lang.org")) {
            let path = u.path().trim_start_matches('/');
            let stripped = path
                .strip_prefix("beta/")
                .or_else(|| path.strip_prefix("stable/"))
                .or_else(|| path.strip_prefix("nightly/"))
                .unwrap_or(path);
            return format!("doc.rust-lang.org/{}", stripped);
        }

        if let Some(host) = host.as_deref() {
            let _ = u.set_host(Some(host));
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

fn domain_authority_bonus(query_tokens: &HashSet<String>, host: &str, bonus: f32) -> f32 {
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
                return bonus;
            }
        }
    }
    0.0
}

#[cfg(test)]
mod tests {
    use super::{canonical_url_key, tokenize_set};

    #[test]
    fn tokenize_set_preserves_compound_technical_terms() {
        let tokens = tokenize_set("what is a B-tree and a three-way handshake?");

        assert!(tokens.contains("btree"));
        assert!(tokens.contains("threeway"));
        assert!(tokens.contains("handshake"));
        assert!(!tokens.contains("what"));
    }

    #[test]
    fn tokenize_set_canonicalizes_btree_variants() {
        assert!(tokenize_set("B-tree").contains("btree"));
        assert!(tokenize_set("B+tree").contains("btree"));
        assert!(tokenize_set("B tree").contains("btree"));
        assert!(tokenize_set("btree").contains("btree"));
    }

    #[test]
    fn canonical_url_key_dedupes_www_aliases() {
        assert_eq!(
            canonical_url_key("https://sqlite.org/c3ref/wal_checkpoint.html"),
            canonical_url_key("https://www.sqlite.org/c3ref/wal_checkpoint.html?view=all#top"),
        );
    }

    #[test]
    fn canonical_url_key_dedupes_rust_doc_channels() {
        assert_eq!(
            canonical_url_key("https://doc.rust-lang.org/stable/reference/lifetime-elision.html"),
            canonical_url_key("https://docs.rust-lang.org/nightly/reference/lifetime-elision.html"),
        );
    }
}
