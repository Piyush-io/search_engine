use url::Url;
use scraper::{Html, Selector};
use std::collections::HashSet;
use crate::crawler::types::{FetchResult, ParsedPage, PageQuality, RejectReason};
use crate::crawler::{canon, policy};
use crate::extraction::normalizer;

const MIN_PAGE_TEXT_BYTES: usize = 400;
const MIN_PAGE_BLOCKS: usize = 2;

pub fn parse_result(result: FetchResult) -> Result<ParsedPage, RejectReason> {
    let base = match Url::parse(&result.final_url) {
        Ok(url) => url,
        Err(_) => return Err(RejectReason::RedirectBadUrl),
    };

    let (mut page, cleaned, meta) =
        normalizer::normalize_with_cleaned(&result.html, &result.final_url);
    
    let canonical_url = resolve_page_canonical(&base, meta.canonical_url.as_deref())
        .map(|u| u.to_string())
        .unwrap_or_else(|| result.final_url.clone());
    page.url = canonical_url.clone();

    let outlinks = extract_links(&base, &cleaned);

    if result.x_robots_noindex || meta.noindex {
        return Err(RejectReason::NoIndex);
    }

    let text_bytes: usize = page.blocks.iter().map(|block| block.text.len()).sum();
    
    // Quality check
    let block_count = page.blocks.len();
    let link_density = calculate_link_density(&cleaned);
    
    let is_low_quality = block_count < MIN_PAGE_BLOCKS || text_bytes < MIN_PAGE_TEXT_BYTES;
    
    if is_low_quality {
        return Err(RejectReason::LowText);
    }

    Ok(ParsedPage {
        final_url: result.final_url,
        canonical_url,
        title: page.title.clone(),
        description: page.description.clone(),
        page_record: page,
        outlinks,
        noindex: meta.noindex,
        quality: PageQuality {
            text_bytes,
            block_count,
            link_density,
            should_store: true,
            reject_reason: None,
        },
    })
}

fn resolve_page_canonical(base_url: &Url, raw_canonical: Option<&str>) -> Option<Url> {
    let raw = raw_canonical?.trim();
    if raw.is_empty() {
        return None;
    }

    let joined = base_url.join(raw).ok()?;
    let canonical = canon::canonicalize(joined.as_str())?;
    if policy::url_allowed(&canonical) {
        Some(canonical)
    } else {
        None
    }
}

fn extract_links(base_url: &Url, body: &str) -> Vec<String> {
    let Ok(selector) = Selector::parse("a[href]") else {
        return Vec::new();
    };
    let document = Html::parse_document(body);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let discovery_cap = base_url
        .host_str()
        .map(policy::discovery_limit_for_host)
        .unwrap_or(200);

    for element in document.select(&selector) {
        if out.len() >= discovery_cap {
            break;
        }

        let Some(href) = element.value().attr("href") else {
            continue;
        };
        let Ok(absolute) = base_url.join(href) else {
            continue;
        };
        let Some(canon_url) = canon::canonicalize(absolute.as_str()) else {
            continue;
        };
        if !policy::url_allowed(&canon_url) {
            continue;
        }

        let url = canon_url.to_string();
        if seen.insert(url.clone()) {
            out.push(url);
        }
    }

    out
}

fn calculate_link_density(html: &str) -> f32 {
    let document = Html::parse_document(html);
    let text_selector = Selector::parse("*").unwrap();
    let link_selector = Selector::parse("a").unwrap();
    
    let total_text_len: usize = document.select(&text_selector)
        .map(|n| n.text().collect::<String>().len())
        .sum();
    
    let link_text_len: usize = document.select(&link_selector)
        .map(|n| n.text().collect::<String>().len())
        .sum();
    
    if total_text_len == 0 {
        0.0
    } else {
        link_text_len as f32 / total_text_len as f32
    }
}
