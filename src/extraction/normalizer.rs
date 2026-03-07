use crate::{PageRecord, TextBlock};
use kuchiki::traits::TendrilSink;
use scraper::{ElementRef, Html, Selector};

use super::{density, metadata};

fn extract_text(el: &ElementRef<'_>) -> String {
    el.text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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

fn select_content_root<'a>(doc: &'a Html) -> Option<ElementRef<'a>> {
    let candidates = ["main", "article", "[role='main']", "body"];

    for sel in candidates {
        let selector = Selector::parse(sel).ok()?;
        if let Some(root) = doc.select(&selector).next() {
            return Some(root);
        }
    }

    None
}

fn table_headers_for_row(row: &ElementRef<'_>) -> Vec<String> {
    let table = row
        .ancestors()
        .filter_map(ElementRef::wrap)
        .find(|el| el.value().name() == "table");

    let Some(table) = table else {
        return Vec::new();
    };

    let thead_sel = Selector::parse("thead th").unwrap();
    let first_row_th_sel = Selector::parse("tr th").unwrap();

    let mut headers: Vec<String> = table
        .select(&thead_sel)
        .map(|h| extract_text(&h))
        .filter(|t| !t.is_empty())
        .collect();

    if headers.is_empty() {
        headers = table
            .select(&first_row_th_sel)
            .take(16)
            .map(|h| extract_text(&h))
            .filter(|t| !t.is_empty())
            .collect();
    }

    headers
}

fn denormalize_table_row(row: &ElementRef<'_>) -> Option<String> {
    let cell_sel = Selector::parse("th,td").ok()?;

    let mut cells = Vec::new();
    let mut saw_td = false;
    for c in row.select(&cell_sel) {
        let tag = c.value().name();
        if tag == "td" {
            saw_td = true;
        }

        let txt = extract_text(&c);
        if !txt.is_empty() {
            cells.push(txt);
        }
    }

    if cells.is_empty() {
        return None;
    }

    // Skip pure header rows; keep data rows for retrieval.
    if !saw_td {
        return None;
    }

    let headers = table_headers_for_row(row);
    if !headers.is_empty() && headers.len() == cells.len() {
        let pairs = headers
            .iter()
            .zip(cells.iter())
            .map(|(h, c)| format!("{}: {}", h, c))
            .collect::<Vec<_>>();
        return Some(pairs.join(" | "));
    }

    Some(cells.join(" | "))
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
    let root = select_content_root(&parsed).unwrap_or_else(|| parsed.root_element());

    let all_sel = Selector::parse("h1,h2,h3,h4,h5,h6,p,li,dd,dt,pre,code,td,th,tr").unwrap();
    let anchor_sel = Selector::parse("a").unwrap();

    let mut headings: Vec<Option<String>> = vec![None; 6];
    let mut blocks = Vec::new();
    let mut last_dt: Option<String> = None;

    for el in root.select(&all_sel) {
        let tag = el.value().name();

        if let Some(level) = heading_level(tag) {
            let text = extract_text(&el);
            if text.is_empty() {
                continue;
            }

            headings[level - 1] = Some(text);
            for h in headings.iter_mut().skip(level) {
                *h = None;
            }
            continue;
        }

        if tag == "dt" {
            let text = extract_text(&el);
            if !text.is_empty() {
                last_dt = Some(text);
            }
            continue;
        }

        let mut block_text = if tag == "tr" {
            let Some(row) = denormalize_table_row(&el) else {
                continue;
            };
            row
        } else {
            let text = extract_text(&el);
            if text.is_empty() {
                continue;
            }
            text
        };

        if tag == "dd" {
            if let Some(term) = &last_dt {
                block_text = format!("{term}: {block_text}");
            }
        }

        if tag != "tr" {
            let anchor_text_bytes: usize = el
                .select(&anchor_sel)
                .map(|a| a.text().collect::<String>().len())
                .sum();

            if !density::keep(block_text.len(), el.html().len(), anchor_text_bytes) {
                continue;
            }
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
