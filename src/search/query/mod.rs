mod classify;
mod dedupe;
mod fusion;
mod rerank;
mod scoring;
mod tokenize;

use std::collections::{HashMap, HashSet};

use rocksdb::DB;

use crate::{
    Chunk, ScoredHit,
    config::RankingConfig,
    embeddings::client,
    search::{
        lexical::{LexicalBoostConfig, LexicalIndex},
        vector_index::VectorIndex,
    },
    storage,
};

use self::{
    classify::is_identifier_query,
    dedupe::{fill_results, specific_heading_text, url_host},
    fusion::{normalize_scores, rrf_fuse_ids},
    rerank::apply_reranker,
    scoring::{Candidate, domain_authority_bonus},
    tokenize::{
        build_expanded_query_text, combined_token_match_count, contains_phrase,
        expand_query_tokens, normalize_phrase, token_match_count, token_overlap, tokenize_set,
    },
};

#[derive(Default, Debug)]
pub struct SearchDiagnostics {
    pub vector_candidates: usize,
    pub lexical_candidates: usize,
    pub fused_candidates: usize,
    pub final_selected: usize,
}

pub fn run_query_with_diagnostics(
    db: &DB,
    index: &dyn VectorIndex,
    lexical: Option<&LexicalIndex>,
    query_text: &str,
    k: usize,
    ranking: &RankingConfig,
    diagnostics: Option<&mut SearchDiagnostics>,
) -> Vec<ScoredHit> {
    run_query_inner(db, index, lexical, query_text, k, ranking, diagnostics)
}

pub fn run_query(
    db: &DB,
    index: &dyn VectorIndex,
    lexical: Option<&LexicalIndex>,
    query_text: &str,
    k: usize,
    ranking: &RankingConfig,
) -> Vec<ScoredHit> {
    run_query_inner(db, index, lexical, query_text, k, ranking, None)
}

fn run_query_inner(
    db: &DB,
    index: &dyn VectorIndex,
    lexical: Option<&LexicalIndex>,
    query_text: &str,
    k: usize,
    ranking: &RankingConfig,
    mut diagnostics: Option<&mut SearchDiagnostics>,
) -> Vec<ScoredHit> {
    let query_vec = match client::embed_query(query_text) {
        Ok(v) => Some(v),
        Err(e) => {
            eprintln!("[query] embedding failed: {e}");
            None
        }
    };

    if query_vec.is_none() && (!ranking.lexical_only_fallback_enabled || lexical.is_none()) {
        if let Some(d) = diagnostics {
            d.final_selected = 0;
        }
        return Vec::new();
    }

    // Classify query before vector search so ef_search can adapt.
    let query_tokens = tokenize_set(query_text);
    let short_query = query_tokens.len() <= 5;
    let identifier_query = is_identifier_query(query_text);

    // Per-query-class ef_search for HNSW backends. Keep this request-scoped;
    // the web server shares one index across concurrent searches.
    let ef_search = if identifier_query && ranking.ef_search_identifier > 0 {
        Some(ranking.ef_search_identifier)
    } else if short_query && ranking.ef_search_short > 0 {
        Some(ranking.ef_search_short)
    } else if !short_query && ranking.ef_search_long > 0 {
        Some(ranking.ef_search_long)
    } else {
        None
    };

    let vec_pool_k = (k.saturating_mul(40)).clamp(k, 1_600);
    let lex_pool_k = if query_vec.is_some() {
        (k.saturating_mul(20)).clamp(k, 800)
    } else {
        let cap = ranking.lexical_only_pool_cap.max(k);
        (k.saturating_mul(ranking.lexical_only_pool_mult.max(1))).clamp(k, cap)
    };

    let vector_hits = query_vec
        .as_ref()
        .map(|query_vec| {
            if let Some(ef_search) = ef_search {
                index.search_with_ef(query_vec, vec_pool_k, ef_search)
            } else {
                index.search(query_vec, vec_pool_k)
            }
        })
        .unwrap_or_default();

    if let Some(d) = &mut diagnostics {
        d.vector_candidates = vector_hits.len();
    }

    let expanded_tokens = expand_query_tokens(&query_tokens);
    let expanded_query = build_expanded_query_text(query_text, &query_tokens);

    let lexical_boosts = LexicalBoostConfig {
        field_boost_title: ranking.lexical_field_boost_title,
        field_boost_section: ranking.lexical_field_boost_section,
        field_boost_heading: ranking.lexical_field_boost_heading,
        field_boost_text: ranking.lexical_field_boost_text,
        short_query_phrase_boost: ranking.lexical_short_query_phrase_boost,
    };

    let lexical_hits = lexical
        .and_then(|lx| {
            lx.search(
                &expanded_query,
                lex_pool_k,
                ranking.lexical_relaxed_fallback_enabled && query_vec.is_none(),
                ranking.lexical_relaxed_min_hits,
                ranking.lexical_relaxed_extra_k,
                &lexical_boosts,
            )
            .ok()
        })
        .unwrap_or_default();

    if let Some(d) = &mut diagnostics {
        d.lexical_candidates = lexical_hits.len();
    }

    if query_vec.is_none() && lexical_hits.is_empty() {
        if let Some(d) = diagnostics {
            d.final_selected = 0;
        }
        return Vec::new();
    }

    let fused_ids = rrf_fuse_ids(&vector_hits, &lexical_hits, 2_000, short_query, ranking);

    if let Some(d) = &mut diagnostics {
        d.fused_candidates = fused_ids.len();
    }
    let vec_scores = normalize_scores(&vector_hits);
    let lex_scores = normalize_scores(&lexical_hits);

    let chunks_cf = match storage::cf(db, storage::CF_CHUNKS) {
        Ok(cf) => cf,
        Err(_) => {
            if let Some(d) = diagnostics {
                d.final_selected = 0;
            }
            return Vec::new();
        }
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

                // Skip heading-match penalties for identifier queries — the
                // identifier token itself may not appear in any heading.
                if !identifier_query {
                    if short_query && title_overlap == 0.0 && heading_overlap == 0.0 {
                        rerank_score *= ranking.no_heading_penalty;
                    } else if short_query && title_overlap == 0.0 && heading_overlap < 0.34 {
                        rerank_score *= ranking.weak_heading_penalty;
                    }
                }

                let structural_signal = title_overlap.max(heading_overlap);
                let auth_bonus = if lex_score >= ranking.authority_min_lexical_score
                    || structural_signal >= ranking.authority_min_structural_overlap
                {
                    if let Some(host) = url_host(&chunk.source_url) {
                        domain_authority_bonus(&query_tokens, &host, ranking.authority_bonus)
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };
                rerank_score += auth_bonus;

                // Host allowlist / penalty multipliers
                if let Some(host) = url_host(&chunk.source_url) {
                    let host_lower = host.to_ascii_lowercase();
                    if ranking.host_allowlist.iter().any(|h| {
                        let h = h.to_ascii_lowercase();
                        host_lower == h || host_lower.ends_with(&format!(".{h}"))
                    }) {
                        rerank_score *= ranking.host_allowlist_boost;
                    }
                    if ranking.host_penalty_list.iter().any(|h| {
                        let h = h.to_ascii_lowercase();
                        host_lower == h || host_lower.ends_with(&format!(".{h}"))
                    }) {
                        rerank_score *= ranking.host_soft_penalty;
                    }
                }

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

    // Optional cross-encoder reranking on top heuristic candidates.
    // Skip reranking for identifier queries where token-level exactness matters
    // more than semantic similarity.
    if ranking.rerank_enabled && !identifier_query {
        apply_reranker(query_text, &mut candidates, ranking);
    }

    candidates.sort_by(|a, b| b.final_score.total_cmp(&a.final_score));

    let score_floor = if let Some(top) = candidates.first() {
        (top.final_score * ranking.score_floor_fraction).max(ranking.score_floor_min)
    } else {
        ranking.score_floor_min
    };
    candidates.retain(|c| c.final_score >= score_floor);

    let mut selected: Vec<ScoredHit> = Vec::new();
    let mut seen_url_keys = HashSet::new();
    let mut seen_text_tokens = Vec::new();
    let mut per_host: HashMap<String, usize> = HashMap::new();
    let host_cap = if let Some(hard_cap) = ranking.host_hard_cap {
        hard_cap.max(1)
    } else {
        (k / ranking.host_cap_divisor.max(1)).clamp(
            ranking.host_cap_min.max(1),
            ranking.host_cap_max.max(ranking.host_cap_min.max(1)),
        )
    };

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

    if let Some(d) = diagnostics {
        d.final_selected = selected.len();
    }

    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use dedupe::canonical_url_key;
    use scoring::domain_authority_bonus;
    use tokenize::tokenize_set;

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

    #[test]
    fn authority_bonus_matches_normalized_hosts() {
        let mut tokens = std::collections::HashSet::new();
        tokens.insert("typescript".to_string());
        assert!(domain_authority_bonus(&tokens, "www.typescriptlang.org", 0.5) > 0.0);
    }

    #[test]
    fn identifier_query_detects_acronyms() {
        assert!(is_identifier_query("WAL checkpoint"));
        assert!(is_identifier_query("LSN logging"));
        assert!(is_identifier_query("MVCC postgres"));
        assert!(is_identifier_query("XID wraparound"));
        assert!(is_identifier_query("CID in postgres"));
        assert!(is_identifier_query("TLS handshake"));
        assert!(is_identifier_query("SSD vs HDD"));
    }

    #[test]
    fn identifier_query_detects_underscores_and_namespaces() {
        assert!(is_identifier_query("wal_level"));
        assert!(is_identifier_query("full_page_writes"));
        assert!(is_identifier_query("std::vector"));
        assert!(is_identifier_query("pg_catalog::pg_class"));
    }

    #[test]
    fn identifier_query_rejects_normal_queries() {
        assert!(!is_identifier_query("what is a wal"));
        assert!(!is_identifier_query("how does postgres work"));
        assert!(!is_identifier_query("explain checkpoint"));
    }
}
