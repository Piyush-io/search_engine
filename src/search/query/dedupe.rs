use std::collections::{HashMap, HashSet};

use url::Url;

use crate::ScoredHit;

use super::scoring::Candidate;
use super::tokenize;

pub(super) fn fill_results<'a>(
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

        let tokens = tokenize::tokenize_set(&c.text);
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

pub(super) fn canonical_host(host: &str) -> String {
    let host = host.to_ascii_lowercase();
    if matches!(host.as_str(), "doc.rust-lang.org" | "docs.rust-lang.org") {
        return "doc.rust-lang.org".to_string();
    }
    host.strip_prefix("www.")
        .unwrap_or(host.as_str())
        .to_string()
}

pub(super) fn canonical_url_key(url: &str) -> String {
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

pub(super) fn specific_heading_text(heading_chain: &[String]) -> String {
    match heading_chain {
        [] => String::new(),
        [only] => only.clone(),
        [_, rest @ ..] => rest.join(" "),
    }
}

pub(super) fn url_host(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
}
