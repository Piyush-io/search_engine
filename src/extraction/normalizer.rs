use crate::{PageRecord, TextBlock};
use kuchiki::traits::TendrilSink;
use scraper::{ElementRef, Html, Selector};

use super::{density, metadata};

fn extract_text(el: &ElementRef<'_>) -> String {
    el.text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

fn heading_level(tag: &str) -> Option<usize> {
    match tag {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

/// Parse raw HTML into a `PageRecord` with heading-aware text blocks.
pub fn normalize(html: &str, url: &str) -> PageRecord {
    // 1) Parse + strip boilerplate tags.
    let doc = kuchiki::parse_html().one(html);
    if let Ok(nodes) = doc.select("script, style, nav, header, footer") {
        for node in nodes {
            node.as_node().detach();
        }
    }

    let mut cleaned_bytes = Vec::new();
    let _ = doc.serialize(&mut cleaned_bytes);
    let cleaned = String::from_utf8_lossy(&cleaned_bytes).to_string();

    // 2) Metadata.
    let meta = metadata::extract(&cleaned);

    // 3) Walk content in document order and build heading-context blocks.
    let parsed = Html::parse_document(&cleaned);
    let all_sel = Selector::parse("h1,h2,h3,h4,h5,h6,p,li,dd,dt,pre,code,td,th").unwrap();
    let anchor_sel = Selector::parse("a").unwrap();

    let mut headings: Vec<Option<String>> = vec![None; 6];
    let mut blocks = Vec::new();
    let mut last_dt: Option<String> = None;

    for el in parsed.select(&all_sel) {
        let tag = el.value().name();
        let text = extract_text(&el);
        if text.is_empty() {
            continue;
        }

        if let Some(level) = heading_level(tag) {
            headings[level - 1] = Some(text);
            for h in headings.iter_mut().skip(level) {
                *h = None;
            }
            continue;
        }

        if tag == "dt" {
            last_dt = Some(text);
            continue;
        }

        let mut block_text = text;
        if tag == "dd" {
            if let Some(term) = &last_dt {
                block_text = format!("{term}: {block_text}");
            }
        }

        let anchor_text_bytes: usize = el.select(&anchor_sel)
            .map(|a| a.text().collect::<String>().len())
            .sum();

        if !density::keep(block_text.len(), el.html().len(), anchor_text_bytes) {
            continue;
        }

        let heading_chain = headings
            .iter()
            .filter_map(|h| h.clone())
            .collect::<Vec<_>>();

        blocks.push(TextBlock {
            heading_chain,
            text: block_text,
        });
    }

    PageRecord {
        url: url.to_string(),
        title: meta.title.unwrap_or_else(|| url.to_string()),
        description: meta.description,
        blocks,
    }
}
