use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};

use serde::Serialize;

use search_engine::eval::canonical_doc_key;
use search_engine::pipeline::PageState;
use search_engine::storage;

#[derive(Serialize)]
struct MissingEntry {
    url: String,
    query_ids: Vec<String>,
}

#[derive(Serialize)]
struct ZeroChunkEntry {
    url: String,
    query_ids: Vec<String>,
}

#[derive(Serialize)]
struct AuditReport {
    total_unique_urls: usize,
    urls_found_in_db: usize,
    urls_missing_from_db: Vec<MissingEntry>,
    urls_with_zero_chunks: Vec<ZeroChunkEntry>,
    urls_with_chunks: HashMap<String, usize>,
    coverage_percentage: f64,
    canonicalization: CanonicalizationSummary,
    recommendations: Vec<String>,
}

#[derive(Serialize)]
struct CanonicalizationSummary {
    exact_match_count: usize,
    canonical_match_count: usize,
    mismatch_count: usize,
    mismatches: Vec<MismatchDetail>,
}

#[derive(Serialize)]
struct MismatchDetail {
    qrel_url: String,
    canonical_key: String,
    db_key_used: String,
    note: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = "crawl_data.niche";
    let qrels_path = "benchmarks/niche_db/qrels_100.tsv";
    let report_path = "reports/qrels_audit.json";

    let db = storage::open_db_read_only(db_path)?;
    let content_cf = storage::cf(&db, storage::CF_CONTENT)?;
    let page_state_cf = storage::cf(&db, storage::CF_PAGE_STATE)?;

    // Read qrels
    let file = File::open(qrels_path)?;
    let reader = BufReader::new(file);

    let mut url_to_queries: HashMap<String, Vec<String>> = HashMap::new();
    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 4 {
            continue;
        }
        let query_id = parts[0].to_string();
        let raw_url = parts[2].to_string();
        url_to_queries.entry(raw_url).or_default().push(query_id);
    }

    let total_unique_urls = url_to_queries.len();
    let mut urls_found_in_db = 0usize;
    let mut urls_missing_from_db: Vec<MissingEntry> = Vec::new();
    let mut urls_with_zero_chunks: Vec<ZeroChunkEntry> = Vec::new();
    let mut urls_with_chunks: HashMap<String, usize> = HashMap::new();

    let mut exact_match_count = 0usize;
    let mut canonical_match_count = 0usize;
    let mut mismatch_count = 0usize;
    let mut raw_canonical_diff_count = 0usize;
    let mut mismatches: Vec<MismatchDetail> = Vec::new();

    for (raw_url, query_ids) in &url_to_queries {
        let exact_exists = db.get_cf(content_cf, raw_url.as_bytes())?.is_some();

        let canonical = canonical_doc_key(raw_url);
        let canonical_exists = if canonical == *raw_url {
            exact_exists // avoid double lookup
        } else {
            db.get_cf(content_cf, canonical.as_bytes())?.is_some()
        };

        let found = exact_exists || canonical_exists;
        let matched_key = if exact_exists {
            raw_url.clone()
        } else if canonical_exists {
            canonical.clone()
        } else {
            String::new()
        };

        if found {
            urls_found_in_db += 1;

            if exact_exists {
                exact_match_count += 1;
            } else {
                canonical_match_count += 1;
            }

            // chunk count via page_state
            let chunk_count =
                if let Some(state_bytes) = db.get_cf(page_state_cf, matched_key.as_bytes())? {
                    if let Ok(state) = serde_json::from_slice::<PageState>(&state_bytes) {
                        state.chunk_ids.len()
                    } else {
                        0
                    }
                } else {
                    0
                };

            if chunk_count == 0 {
                urls_with_zero_chunks.push(ZeroChunkEntry {
                    url: raw_url.clone(),
                    query_ids: query_ids.clone(),
                });
            } else {
                urls_with_chunks.insert(raw_url.clone(), chunk_count);
            }

            // Canonicalization consistency check
            if exact_exists && canonical != *raw_url {
                raw_canonical_diff_count += 1;
                mismatch_count += 1;
                mismatches.push(MismatchDetail {
                    qrel_url: raw_url.clone(),
                    canonical_key: canonical.clone(),
                    db_key_used: matched_key.clone(),
                    note: "DB stores raw URL; canonical form differs (e.g. www stripped or version normalized)".to_string(),
                });
            } else if !exact_exists && canonical_exists {
                mismatch_count += 1;
                mismatches.push(MismatchDetail {
                    qrel_url: raw_url.clone(),
                    canonical_key: canonical.clone(),
                    db_key_used: matched_key.clone(),
                    note: "DB stores canonical form; raw URL not present".to_string(),
                });
            }
        } else {
            urls_missing_from_db.push(MissingEntry {
                url: raw_url.clone(),
                query_ids: query_ids.clone(),
            });

            mismatch_count += 1;
            mismatches.push(MismatchDetail {
                qrel_url: raw_url.clone(),
                canonical_key: canonical.clone(),
                db_key_used: "(not found)".to_string(),
                note: "URL missing from DB under both raw and canonical form".to_string(),
            });
        }
    }

    let coverage_percentage = if total_unique_urls > 0 {
        (urls_found_in_db as f64 / total_unique_urls as f64) * 100.0
    } else {
        0.0
    };

    // Recommendations
    let mut recommendations: Vec<String> = Vec::new();

    if !urls_missing_from_db.is_empty() {
        recommendations.push(format!(
            "Add these {} missing URLs to crawl seeds",
            urls_missing_from_db.len()
        ));
    }

    if !urls_with_zero_chunks.is_empty() {
        recommendations.push(format!(
            "These {} URLs have 0 chunks - check normalization",
            urls_with_zero_chunks.len()
        ));
    }

    if raw_canonical_diff_count > 0 {
        recommendations.push(format!(
            "{} found URLs are stored under raw form but canonical_doc_key() changes them - align crawl canonicalization with eval canonicalization",
            raw_canonical_diff_count
        ));
    }

    if canonical_match_count > 0 {
        recommendations.push(format!(
            "{} URLs were found only after canonicalization - consider aligning crawl canonicalization with eval canonical_doc_key()",
            canonical_match_count
        ));
    }

    let report = AuditReport {
        total_unique_urls,
        urls_found_in_db,
        urls_missing_from_db,
        urls_with_zero_chunks,
        urls_with_chunks,
        coverage_percentage,
        canonicalization: CanonicalizationSummary {
            exact_match_count,
            canonical_match_count,
            mismatch_count,
            mismatches,
        },
        recommendations,
    };

    std::fs::create_dir_all("reports")?;
    let json = serde_json::to_string_pretty(&report)?;
    std::fs::write(report_path, json)?;

    println!("=== Qrels Audit Complete ===");
    println!("Total unique URLs: {}", total_unique_urls);
    println!("Found in DB:       {}", urls_found_in_db);
    println!("Missing from DB:   {}", report.urls_missing_from_db.len());
    println!("Coverage:          {:.2}%", coverage_percentage);
    println!("Report written to: {}", report_path);

    Ok(())
}
