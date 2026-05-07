use std::collections::{BTreeMap, HashMap, HashSet};

use rocksdb::DB;
use serde::Serialize;
use url::Url;

use crate::{
    ScoredHit,
    config::RankingConfig,
    eval::{Qrel, canonical_doc_key, compute_metrics, load_qrels, load_queries},
    search::{self, lexical::LexicalIndex, vector_index::VectorIndex},
};

#[derive(Debug, Serialize)]
pub struct DebugSearchResponse {
    pub query: String,
    pub elapsed_ms: u128,
    pub result_count: usize,
    pub summary: DebugSummary,
    pub results: Vec<DebugHit>,
}

#[derive(Debug, Serialize)]
pub struct DebugSummary {
    pub query_terms: Vec<String>,
    pub short_query: bool,
    pub score_gap_top1_top2: f32,
    pub host_diversity: usize,
    pub avg_vector_score: f32,
    pub avg_lexical_score: f32,
    pub avg_heading_overlap: f32,
    pub avg_body_overlap: f32,
    pub dense_dominant_hits: usize,
    pub lexical_dominant_hits: usize,
    pub authority_boosted_hits: usize,
    pub exact_phrase_hits: usize,
    pub risk_counts: Vec<DebugRiskCount>,
}

#[derive(Debug, Serialize)]
pub struct DebugRiskCount {
    pub code: String,
    pub label: String,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct DebugHit {
    pub rank: usize,
    pub chunk_id: String,
    pub source_url: String,
    pub display_url: String,
    pub host: String,
    pub heading_chain: Vec<String>,
    pub text: String,
    pub preview: String,
    pub matched_terms: Vec<String>,
    pub missing_terms: Vec<String>,
    pub diagnostics: Vec<DebugDiagnostic>,
    pub score_breakdown: DebugScoreBreakdown,
}

#[derive(Debug, Serialize)]
pub struct DebugDiagnostic {
    pub code: String,
    pub label: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct DebugScoreBreakdown {
    pub query_mode: String,
    pub vector_score: f32,
    pub lexical_score: f32,
    pub title_overlap: f32,
    pub heading_overlap: f32,
    pub body_overlap: f32,
    pub vector_contribution: f32,
    pub lexical_contribution: f32,
    pub title_contribution: f32,
    pub heading_contribution: f32,
    pub body_contribution: f32,
    pub base_total: f32,
    pub phrase_bonus: f32,
    pub penalty_multiplier: f32,
    pub pre_authority_score: f32,
    pub authority_bonus: f32,
    pub final_score: f32,
    pub dense_minus_lexical: f32,
    pub exact_heading_phrase: bool,
    pub exact_body_phrase: bool,
    pub reconstruction_gap: f32,
}

#[derive(Debug, Serialize)]
pub struct DebugEvalResponse {
    pub elapsed_ms: u128,
    pub qrels_path: String,
    pub queries_path: String,
    pub top_k: usize,
    pub num_queries: usize,
    pub mrr: f64,
    pub ndcg_at: Vec<MetricPoint>,
    pub recall_at: Vec<MetricPoint>,
    pub worst_queries: Vec<DebugQueryEvalRow>,
}

#[derive(Debug, Serialize)]
pub struct MetricPoint {
    pub k: usize,
    pub value: f64,
}

#[derive(Debug, Serialize)]
pub struct DebugQueryEvalRow {
    pub query_id: String,
    pub query: String,
    pub reciprocal_rank: f64,
    pub first_relevant_rank: Option<usize>,
    pub ndcg_at_top_k: f64,
    pub recall_at_top_k: f64,
    pub returned_relevant: Vec<String>,
    pub missed_relevant: Vec<String>,
}

pub fn default_k_values() -> Vec<usize> {
    vec![1, 3, 5, 10]
}

pub fn parse_k_values(raw: Option<&str>) -> Vec<usize> {
    let mut values = raw
        .map(|input| {
            input
                .split(',')
                .filter_map(|part| part.trim().parse::<usize>().ok())
                .filter(|k| *k > 0)
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(default_k_values);

    values.sort_unstable();
    values.dedup();

    if values.is_empty() {
        default_k_values()
    } else {
        values
    }
}

pub fn build_debug_search_response(
    query: &str,
    hits: &[ScoredHit],
    elapsed_ms: u128,
    ranking: &RankingConfig,
) -> DebugSearchResponse {
    let query_terms = extract_query_terms(query);
    let short_query = query_terms.len() <= 5;

    let results: Vec<DebugHit> = hits
        .iter()
        .enumerate()
        .map(|(index, hit)| build_debug_hit(index + 1, hit, &query_terms, ranking, short_query))
        .collect();

    let mut risk_counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
    let mut hosts = HashSet::new();
    let mut dense_dominant_hits = 0;
    let mut lexical_dominant_hits = 0;
    let mut authority_boosted_hits = 0;
    let mut exact_phrase_hits = 0;

    for result in &results {
        if !result.host.is_empty() {
            hosts.insert(result.host.clone());
        }

        if result.score_breakdown.dense_minus_lexical >= 0.15 {
            dense_dominant_hits += 1;
        } else if result.score_breakdown.dense_minus_lexical <= -0.15 {
            lexical_dominant_hits += 1;
        }

        if result.score_breakdown.authority_bonus > 0.0 {
            authority_boosted_hits += 1;
        }

        if result.score_breakdown.exact_heading_phrase || result.score_breakdown.exact_body_phrase {
            exact_phrase_hits += 1;
        }

        for diagnostic in &result.diagnostics {
            let entry = risk_counts
                .entry(diagnostic.code.clone())
                .or_insert_with(|| (diagnostic.label.clone(), 0));
            entry.1 += 1;
        }
    }

    let mut risk_counts: Vec<DebugRiskCount> = risk_counts
        .into_iter()
        .map(|(code, (label, count))| DebugRiskCount { code, label, count })
        .collect();
    risk_counts.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.code.cmp(&b.code)));

    let summary = DebugSummary {
        query_terms,
        short_query,
        score_gap_top1_top2: score_gap_top1_top2(hits),
        host_diversity: hosts.len(),
        avg_vector_score: average(
            results
                .iter()
                .map(|result| result.score_breakdown.vector_score),
        ),
        avg_lexical_score: average(
            results
                .iter()
                .map(|result| result.score_breakdown.lexical_score),
        ),
        avg_heading_overlap: average(
            results
                .iter()
                .map(|result| result.score_breakdown.heading_overlap),
        ),
        avg_body_overlap: average(
            results
                .iter()
                .map(|result| result.score_breakdown.body_overlap),
        ),
        dense_dominant_hits,
        lexical_dominant_hits,
        authority_boosted_hits,
        exact_phrase_hits,
        risk_counts,
    };

    DebugSearchResponse {
        query: query.to_string(),
        elapsed_ms,
        result_count: hits.len(),
        summary,
        results,
    }
}

pub fn run_debug_evaluation(
    db: &DB,
    index: &dyn VectorIndex,
    lexical: Option<&LexicalIndex>,
    ranking: &RankingConfig,
    qrels_path: &str,
    queries_path: &str,
    k_values: &[usize],
) -> Result<DebugEvalResponse, String> {
    let started = std::time::Instant::now();
    let qrels = load_qrels(qrels_path).map_err(|err| err.to_string())?;
    let queries = load_queries(queries_path).map_err(|err| err.to_string())?;

    let top_k = *k_values.iter().max().unwrap_or(&10);
    let mut ranked_lists: HashMap<String, Vec<String>> = HashMap::new();
    let mut per_query = Vec::new();

    for (query_id, query_text) in &queries {
        let results = search::query::run_query(db, index, lexical, query_text, top_k, ranking);
        let ranked: Vec<String> = results
            .iter()
            .map(|result| canonical_doc_key(&result.source_url))
            .collect();
        ranked_lists.insert(query_id.clone(), ranked.clone());

        let judged = match qrels.get(query_id) {
            Some(judged) => judged,
            None => continue,
        };

        let rel_map = relevance_map(judged);
        let first_relevant_rank = first_relevant_rank(&ranked, &rel_map);
        let reciprocal_rank = first_relevant_rank
            .map(|rank| 1.0 / (rank as f64 + 1.0))
            .unwrap_or(0.0);
        let ndcg_at_top_k = ndcg_at_k(&ranked, &rel_map, top_k);
        let recall_at_top_k = recall_at_k(&ranked, &rel_map, top_k);

        per_query.push(DebugQueryEvalRow {
            query_id: query_id.clone(),
            query: query_text.clone(),
            reciprocal_rank,
            first_relevant_rank,
            ndcg_at_top_k,
            recall_at_top_k,
            returned_relevant: returned_relevant(&ranked, &rel_map, top_k),
            missed_relevant: missed_relevant(&ranked, &rel_map, top_k),
        });
    }

    let metrics = compute_metrics(&ranked_lists, &qrels, k_values);
    per_query.sort_by(|a, b| {
        a.reciprocal_rank
            .total_cmp(&b.reciprocal_rank)
            .then_with(|| a.recall_at_top_k.total_cmp(&b.recall_at_top_k))
            .then_with(|| a.ndcg_at_top_k.total_cmp(&b.ndcg_at_top_k))
            .then_with(|| a.query_id.cmp(&b.query_id))
    });
    per_query.truncate(8);

    Ok(DebugEvalResponse {
        elapsed_ms: started.elapsed().as_millis(),
        qrels_path: qrels_path.to_string(),
        queries_path: queries_path.to_string(),
        top_k,
        num_queries: metrics.num_queries,
        mrr: metrics.mrr,
        ndcg_at: metrics
            .ndcg_at
            .into_iter()
            .map(|(k, value)| MetricPoint { k, value })
            .collect(),
        recall_at: metrics
            .recall_at
            .into_iter()
            .map(|(k, value)| MetricPoint { k, value })
            .collect(),
        worst_queries: per_query,
    })
}

fn build_debug_hit(
    rank: usize,
    hit: &ScoredHit,
    query_terms: &[String],
    ranking: &RankingConfig,
    short_query: bool,
) -> DebugHit {
    let combined = format!("{} {}", hit.heading_chain.join(" "), hit.text);
    let combined_terms = extract_query_terms_set(&combined);
    let matched_terms: Vec<String> = query_terms
        .iter()
        .filter(|term| combined_terms.contains(*term))
        .cloned()
        .collect();
    let missing_terms: Vec<String> = query_terms
        .iter()
        .filter(|term| !combined_terms.contains(*term))
        .cloned()
        .collect();
    let diagnostics = diagnose_hit(
        hit,
        word_count(&hit.text),
        matched_terms.len(),
        missing_terms.len(),
    );
    let host = url_host(&hit.source_url).unwrap_or_default();
    let score_breakdown = build_score_breakdown(hit, ranking, short_query);

    DebugHit {
        rank,
        chunk_id: hit.chunk_id.clone(),
        source_url: hit.source_url.clone(),
        display_url: extract_display_url(&hit.source_url),
        host,
        heading_chain: hit.heading_chain.clone(),
        text: hit.text.clone(),
        preview: build_preview(&hit.text),
        matched_terms,
        missing_terms,
        diagnostics,
        score_breakdown,
    }
}

fn build_score_breakdown(
    hit: &ScoredHit,
    ranking: &RankingConfig,
    short_query: bool,
) -> DebugScoreBreakdown {
    let (vec_weight, lex_weight, title_weight, heading_weight, body_weight, query_mode) =
        if short_query {
            (
                ranking.short_vec_weight,
                ranking.short_lex_weight,
                ranking.short_title_weight,
                ranking.short_heading_weight,
                ranking.short_body_weight,
                "short",
            )
        } else {
            (
                ranking.long_vec_weight,
                ranking.long_lex_weight,
                ranking.long_title_weight,
                ranking.long_heading_weight,
                ranking.long_body_weight,
                "long",
            )
        };

    let vector_contribution = vec_weight * hit.vector_score;
    let lexical_contribution = lex_weight * hit.lexical_score;
    let title_contribution = title_weight * hit.title_overlap;
    let heading_contribution = heading_weight * hit.heading_overlap;
    let body_contribution = body_weight * hit.body_overlap;
    let base_total = vector_contribution
        + lexical_contribution
        + title_contribution
        + heading_contribution
        + body_contribution;

    let phrase_bonus = if hit.exact_heading_phrase {
        ranking.exact_heading_boost
    } else if hit.exact_body_phrase {
        ranking.exact_body_boost
    } else {
        0.0
    };

    let penalty_multiplier =
        if short_query && hit.title_overlap == 0.0 && hit.heading_overlap == 0.0 {
            ranking.no_heading_penalty
        } else if short_query && hit.title_overlap == 0.0 && hit.heading_overlap < 0.34 {
            ranking.weak_heading_penalty
        } else {
            1.0
        };

    let pre_authority_score = (base_total + phrase_bonus) * penalty_multiplier;
    let reconstructed_final = pre_authority_score + hit.authority_bonus;

    DebugScoreBreakdown {
        query_mode: query_mode.to_string(),
        vector_score: hit.vector_score,
        lexical_score: hit.lexical_score,
        title_overlap: hit.title_overlap,
        heading_overlap: hit.heading_overlap,
        body_overlap: hit.body_overlap,
        vector_contribution,
        lexical_contribution,
        title_contribution,
        heading_contribution,
        body_contribution,
        base_total,
        phrase_bonus,
        penalty_multiplier,
        pre_authority_score,
        authority_bonus: hit.authority_bonus,
        final_score: hit.final_score,
        dense_minus_lexical: hit.vector_score - hit.lexical_score,
        exact_heading_phrase: hit.exact_heading_phrase,
        exact_body_phrase: hit.exact_body_phrase,
        reconstruction_gap: hit.final_score - reconstructed_final,
    }
}

fn diagnose_hit(
    hit: &ScoredHit,
    word_count: usize,
    matched_term_count: usize,
    missing_term_count: usize,
) -> Vec<DebugDiagnostic> {
    let mut diagnostics = Vec::new();
    let structural_overlap = hit.title_overlap.max(hit.heading_overlap);

    if hit.vector_score >= 0.65
        && hit.lexical_score <= 0.20
        && hit.body_overlap < 0.25
        && structural_overlap < 0.25
    {
        diagnostics.push(DebugDiagnostic {
            code: "semantic_confusion".to_string(),
            label: "Semantic drift risk".to_string(),
            reason: "Strong dense similarity is not backed by lexical overlap or heading evidence."
                .to_string(),
        });
    }

    if hit.lexical_score >= 0.60 && hit.vector_score <= 0.20 {
        diagnostics.push(DebugDiagnostic {
            code: "lexical_tunnel_vision".to_string(),
            label: "Lexical tunnel vision".to_string(),
            reason: "BM25 dominates while semantic similarity stays weak, so paraphrases may be under-served.".to_string(),
        });
    }

    if hit.body_overlap >= 0.45 && structural_overlap == 0.0 {
        diagnostics.push(DebugDiagnostic {
            code: "heading_mismatch".to_string(),
            label: "Heading mismatch".to_string(),
            reason: "Query terms appear in the body, but the title and section headings do not reinforce them.".to_string(),
        });
    }

    if hit.body_overlap >= 0.35 && word_count <= 40 {
        diagnostics.push(DebugDiagnostic {
            code: "context_fragmentation".to_string(),
            label: "Context fragmentation".to_string(),
            reason: "The chunk is short and term-heavy, which can preserve keywords while losing surrounding context.".to_string(),
        });
    }

    if hit.authority_bonus > 0.0
        && hit.vector_score < 0.35
        && hit.lexical_score < 0.35
        && structural_overlap < 0.35
    {
        diagnostics.push(DebugDiagnostic {
            code: "authority_bias".to_string(),
            label: "Authority bias risk".to_string(),
            reason: "Domain authority contributes to ranking even though the core relevance signals are soft.".to_string(),
        });
    }

    if (hit.vector_score - hit.lexical_score).abs() >= 0.45 {
        diagnostics.push(DebugDiagnostic {
            code: "dense_lexical_disagreement".to_string(),
            label: "Dense/lexical disagreement".to_string(),
            reason: "Dense and lexical signals disagree sharply, so fusion weights are driving the final order.".to_string(),
        });
    }

    if matched_term_count == 0 && missing_term_count > 0 {
        diagnostics.push(DebugDiagnostic {
            code: "low_term_coverage".to_string(),
            label: "Low term coverage".to_string(),
            reason: "None of the core query terms are surfaced directly in the selected chunk text or headings.".to_string(),
        });
    } else if missing_term_count > matched_term_count && structural_overlap < 0.5 {
        diagnostics.push(DebugDiagnostic {
            code: "partial_coverage".to_string(),
            label: "Partial coverage".to_string(),
            reason:
                "Only part of the query is covered, which can leave the answer context incomplete."
                    .to_string(),
        });
    }

    diagnostics
}

fn relevance_map<'a>(judged: &'a [Qrel]) -> HashMap<&'a str, u32> {
    judged
        .iter()
        .map(|qrel| (qrel.doc_id.as_str(), qrel.relevance))
        .collect()
}

fn first_relevant_rank(ranked: &[String], rel_map: &HashMap<&str, u32>) -> Option<usize> {
    ranked.iter().enumerate().find_map(|(index, doc_id)| {
        rel_map
            .get(doc_id.as_str())
            .copied()
            .filter(|relevance| *relevance > 0)
            .map(|_| index)
    })
}

fn ndcg_at_k(ranked: &[String], rel_map: &HashMap<&str, u32>, k: usize) -> f64 {
    let dcg = ranked
        .iter()
        .enumerate()
        .take(k)
        .fold(0.0, |sum, (index, doc_id)| {
            let relevance = *rel_map.get(doc_id.as_str()).unwrap_or(&0) as f64;
            sum + ((2.0_f64).powf(relevance) - 1.0) / (index as f64 + 2.0).log2()
        });

    let mut ideal_relevances: Vec<u32> = rel_map.values().copied().collect();
    ideal_relevances.sort_by(|a, b| b.cmp(a));
    let ideal =
        ideal_relevances
            .into_iter()
            .enumerate()
            .take(k)
            .fold(0.0, |sum, (index, relevance)| {
                sum + ((2.0_f64).powf(relevance as f64) - 1.0) / (index as f64 + 2.0).log2()
            });

    if ideal == 0.0 { 0.0 } else { dcg / ideal }
}

fn recall_at_k(ranked: &[String], rel_map: &HashMap<&str, u32>, k: usize) -> f64 {
    let total_relevant = rel_map.values().filter(|&&relevance| relevance > 0).count() as f64;
    if total_relevant == 0.0 {
        return 0.0;
    }

    let hits = ranked
        .iter()
        .take(k)
        .filter(|doc_id| rel_map.get(doc_id.as_str()).copied().unwrap_or(0) > 0)
        .count() as f64;

    hits / total_relevant
}

fn returned_relevant(ranked: &[String], rel_map: &HashMap<&str, u32>, k: usize) -> Vec<String> {
    ranked
        .iter()
        .take(k)
        .filter(|doc_id| rel_map.get(doc_id.as_str()).copied().unwrap_or(0) > 0)
        .take(5)
        .cloned()
        .collect()
}

fn missed_relevant(ranked: &[String], rel_map: &HashMap<&str, u32>, k: usize) -> Vec<String> {
    let seen: HashSet<&str> = ranked
        .iter()
        .take(k)
        .map(|doc_id| doc_id.as_str())
        .collect();

    rel_map
        .iter()
        .filter(|(doc_id, relevance)| **relevance > 0 && !seen.contains(**doc_id))
        .map(|(doc_id, _)| (*doc_id).to_string())
        .take(5)
        .collect()
}

fn extract_display_url(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed.host_str().map(|host| {
                let path = parsed.path();
                if path == "/" || path.is_empty() {
                    host.to_string()
                } else {
                    format!("{}{}", host, path)
                }
            })
        })
        .unwrap_or_else(|| url.to_string())
}

fn url_host(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(ToString::to_string))
}

fn build_preview(text: &str) -> String {
    let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.len() > 220 {
        format!("{}...", &cleaned[..cleaned.floor_char_boundary(217)])
    } else {
        cleaned
    }
}

fn average(values: impl Iterator<Item = f32>) -> f32 {
    let mut count = 0usize;
    let mut sum = 0.0;

    for value in values {
        sum += value;
        count += 1;
    }

    if count == 0 { 0.0 } else { sum / count as f32 }
}

fn score_gap_top1_top2(hits: &[ScoredHit]) -> f32 {
    match hits {
        [] => 0.0,
        [only] => only.final_score,
        [first, second, ..] => first.final_score - second.final_score,
    }
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn extract_query_terms(query: &str) -> Vec<String> {
    let mut terms: Vec<String> = extract_query_terms_set(query).into_iter().collect();
    terms.sort();
    terms
}

fn extract_query_terms_set(query: &str) -> HashSet<String> {
    const STOP: &[&str] = &[
        "the", "a", "an", "and", "or", "to", "of", "in", "on", "for", "with", "is", "are", "be",
        "how", "what", "why", "when", "from", "by", "as", "at", "it", "that", "this", "vs",
    ];

    let stop: HashSet<&str> = STOP.iter().copied().collect();
    let mut terms = HashSet::new();

    for word in query.split_whitespace() {
        let clean: String = word
            .chars()
            .filter(|ch| ch.is_alphanumeric() || *ch == '+')
            .collect::<String>()
            .to_ascii_lowercase();

        if clean.len() >= 2 && !stop.contains(clean.as_str()) {
            terms.insert(clean);
        }
    }

    terms
}

fn format_snippet_with_highlights(text: &str, query_terms: &HashSet<String>) -> String {
    let truncated = build_preview(text);

    if query_terms.is_empty() {
        return escape_html(&truncated);
    }

    let mut result = String::with_capacity(truncated.len() * 2);
    let mut chars = truncated.char_indices().peekable();

    while let Some(&(index, ch)) = chars.peek() {
        if ch.is_alphanumeric() || ch == '+' {
            let word_start = index;
            let mut word_end = index;
            while let Some(&(next_index, next_ch)) = chars.peek() {
                if next_ch.is_alphanumeric() || next_ch == '+' {
                    word_end = next_index + next_ch.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let word = &truncated[word_start..word_end];
            let lower = word.to_ascii_lowercase();
            if query_terms.contains(&lower) {
                result.push_str("<b>");
                result.push_str(&escape_html(word));
                result.push_str("</b>");
            } else {
                result.push_str(&escape_html(word));
            }
        } else {
            chars.next();
            let mut buf = [0u8; 4];
            result.push_str(&escape_html(ch.encode_utf8(&mut buf)));
        }
    }

    result
}

fn score_bar_color(value: f32) -> &'static str {
    if value > 0.5 {
        "#1e8e3e"
    } else if value > 0.2 {
        "#f9ab00"
    } else {
        "#d93025"
    }
}

fn score_bar(value: f32) -> String {
    let width = (value.clamp(0.0, 1.0) * 100.0) as u8;
    let color = score_bar_color(value);
    format!(
        r#"<div style="display:flex;align-items:center;gap:8px"><div style="background:#e8eaed;border-radius:4px;height:14px;width:120px;position:relative;overflow:hidden"><div style="background:{};height:100%;width:{}%;border-radius:4px"></div></div><span style="font-size:12px">{:.4}</span></div>"#,
        color, width, value
    )
}

fn yes_no_badge(value: bool) -> &'static str {
    if value {
        r#"<span style="background:#e6f4ea;color:#1e8e3e;padding:2px 8px;border-radius:12px;font-size:11px;font-weight:600">yes</span>"#
    } else {
        r#"<span style="background:#fce8e6;color:#d93025;padding:2px 8px;border-radius:12px;font-size:11px;font-weight:600">no</span>"#
    }
}

pub fn render_debug_page(query: &str, hits: &[ScoredHit], elapsed_ms: u128) -> String {
    let q = escape_html(query);
    let elapsed_sec = elapsed_ms as f64 / 1000.0;

    let mut html = String::new();
    html.push_str(&format!(
        r##"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{} - Debug View</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #fff; color: #202124; }}
        header {{
            background: #202124;
            padding: 12px 24px;
            display: flex;
            align-items: center;
            gap: 24px;
        }}
        .debug-label {{
            background: #d93025;
            color: #fff;
            padding: 3px 10px;
            border-radius: 4px;
            font-size: 12px;
            font-weight: 600;
            letter-spacing: 0.5px;
            text-transform: uppercase;
        }}
        .logo-sm {{ font-size: 22px; font-weight: 300; text-decoration: none; color: #fff; }}
        .logo-sm b {{ font-weight: 600; color: #8ab4f8; }}
        .search-box {{
            display: flex;
            align-items: center;
            border: 1px solid #5f6368;
            border-radius: 24px;
            padding: 4px 8px 4px 16px;
            width: 520px;
            max-width: 60vw;
            background: transparent;
        }}
        .search-box:focus-within {{ border-color: #8ab4f8; }}
        .search-box input {{
            flex: 1;
            border: none;
            outline: none;
            font-size: 15px;
            padding: 8px 0;
            background: transparent;
            color: #fff;
        }}
        .search-box input::placeholder {{ color: #9aa0a6; }}
        .search-box button {{
            background: #1a73e8;
            color: #fff;
            border: none;
            border-radius: 20px;
            padding: 8px 18px;
            font-size: 13px;
            font-weight: 500;
            cursor: pointer;
        }}
        .search-box button:hover {{ background: #1557b0; }}
        .stats {{
            padding: 8px 24px;
            color: #70757a;
            font-size: 13px;
            border-bottom: 1px solid #e8eaed;
        }}
        .stats a {{ color: #1a73e8; text-decoration: none; }}
        .stats a:hover {{ text-decoration: underline; }}
        .content {{ max-width: 800px; padding: 16px 24px; }}
        .card {{
            background: #f8f9fa;
            border: 1px solid #e8eaed;
            border-radius: 8px;
            padding: 16px;
            margin-bottom: 16px;
        }}
        .card-header {{ margin-bottom: 12px; }}
        .breadcrumbs {{
            color: #5f6368;
            font-size: 12px;
            margin-bottom: 4px;
        }}
        .breadcrumbs .sep {{ color: #bdc1c6; margin: 0 4px; }}
        .card-url {{
            font-size: 12px;
            color: #006621;
            font-style: normal;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
            margin-bottom: 4px;
        }}
        .score-table {{ width: 100%; border-collapse: collapse; font-size: 13px; margin-bottom: 12px; }}
        .score-table td {{ padding: 4px 0; }}
        .score-table td:first-child {{ color: #5f6368; width: 180px; }}
        .score-table tr.final td {{ border-top: 2px solid #dadce0; font-weight: 700; color: #202124; }}
        .snippet {{
            color: #4d5156;
            font-size: 14px;
            line-height: 1.58;
            padding-top: 8px;
            border-top: 1px solid #e8eaed;
        }}
        .snippet b {{ color: #202124; font-weight: 600; }}
    </style>
</head>
<body>
    <header>
        <a href="/" class="logo-sm">Neural<b>CS</b></a>
        <span class="debug-label">Debug</span>
        <form action="/search" method="get">
            <div class="search-box">
                <input type="text" name="q" value="{}" autocomplete="off" />
                <button type="submit">Search</button>
            </div>
        </form>
    </header>
"##,
        q, q,
    ));

    html.push_str(&format!(
        r#"<div class="stats">{} results ({:.2}s) — debug view · <a href="/search?q={}">normal view</a></div>"#,
        hits.len(),
        elapsed_sec,
        urlencoding::encode(query),
    ));

    html.push_str(r#"<div class="content">"#);

    if hits.is_empty() {
        html.push_str(
            r#"<p style="color:#5f6368;font-style:italic;padding:40px 0">No results found.</p>"#,
        );
    } else {
        let query_terms = extract_query_terms_set(query);

        for hit in hits {
            let breadcrumbs = if hit.heading_chain.is_empty() {
                String::new()
            } else {
                let chain: Vec<_> = hit
                    .heading_chain
                    .iter()
                    .map(|heading| escape_html(heading))
                    .collect();
                format!(
                    r#"<div class="breadcrumbs">{}</div>"#,
                    chain.join(r#"<span class="sep">›</span>"#)
                )
            };

            let display_url = extract_display_url(&hit.source_url);
            let snippet = format_snippet_with_highlights(&hit.text, &query_terms);

            html.push_str(r#"<div class="card"><div class="card-header">"#);
            html.push_str(&breadcrumbs);
            html.push_str(&format!(
                r#"<div class="card-url">{}</div>"#,
                escape_html(&display_url),
            ));
            html.push_str(r#"</div>"#);

            html.push_str(r#"<table class="score-table">"#);
            html.push_str(&format!(
                r#"<tr><td>Vector Score</td><td>{}</td></tr>"#,
                score_bar(hit.vector_score),
            ));
            html.push_str(&format!(
                r#"<tr><td>Lexical Score</td><td>{}</td></tr>"#,
                score_bar(hit.lexical_score),
            ));
            html.push_str(&format!(
                r#"<tr><td>Title Overlap</td><td>{}</td></tr>"#,
                score_bar(hit.title_overlap),
            ));
            html.push_str(&format!(
                r#"<tr><td>Heading Overlap</td><td>{}</td></tr>"#,
                score_bar(hit.heading_overlap),
            ));
            html.push_str(&format!(
                r#"<tr><td>Body Overlap</td><td>{}</td></tr>"#,
                score_bar(hit.body_overlap),
            ));
            html.push_str(&format!(
                r#"<tr><td>Authority Bonus</td><td>{}</td></tr>"#,
                score_bar(hit.authority_bonus),
            ));
            html.push_str(&format!(
                r#"<tr><td>Exact Heading Phrase</td><td>{}</td></tr>"#,
                yes_no_badge(hit.exact_heading_phrase),
            ));
            html.push_str(&format!(
                r#"<tr><td>Exact Body Phrase</td><td>{}</td></tr>"#,
                yes_no_badge(hit.exact_body_phrase),
            ));
            html.push_str(&format!(
                r#"<tr class="final"><td>Final Score</td><td>{}</td></tr>"#,
                score_bar(hit.final_score),
            ));
            html.push_str(r#"</table>"#);
            html.push_str(&format!(r#"<div class="snippet">{}</div>"#, snippet));
            html.push_str(r#"</div>"#);
        }
    }

    html.push_str(r#"</div></body></html>"#);
    html
}
