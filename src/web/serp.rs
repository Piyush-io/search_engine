use std::collections::HashSet;

use crate::{knowledge::wikipedia::WikiRecord, web::tracking, SearchResult};

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn render_home_page() -> String {
    r##"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Neural CS Search</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: #fff;
            color: #202124;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            min-height: 100vh;
        }
        .logo {
            font-size: 56px;
            font-weight: 300;
            letter-spacing: -2px;
            margin-bottom: 32px;
            color: #202124;
        }
        .logo b { font-weight: 600; color: #1a73e8; }
        .search-box {
            display: flex;
            align-items: center;
            border: 1px solid #dfe1e5;
            border-radius: 24px;
            padding: 6px 8px 6px 16px;
            width: 560px;
            max-width: 90vw;
            box-shadow: 0 1px 6px rgba(32,33,36,0.08);
            transition: box-shadow 0.2s;
        }
        .search-box:hover, .search-box:focus-within {
            box-shadow: 0 1px 6px rgba(32,33,36,0.28);
        }
        .search-box input {
            flex: 1;
            border: none;
            outline: none;
            font-size: 16px;
            padding: 8px 0;
            background: transparent;
        }
        .search-box button {
            background: #1a73e8;
            color: #fff;
            border: none;
            border-radius: 20px;
            padding: 10px 20px;
            font-size: 14px;
            font-weight: 500;
            cursor: pointer;
            white-space: nowrap;
        }
        .search-box button:hover { background: #1557b0; }
        .tagline {
            margin-top: 20px;
            color: #5f6368;
            font-size: 14px;
        }
        footer {
            position: fixed;
            bottom: 0;
            width: 100%;
            background: #f2f2f2;
            border-top: 1px solid #e4e4e4;
            padding: 12px 24px;
            font-size: 13px;
            color: #70757a;
            text-align: center;
        }
    </style>
</head>
<body>
    <div class="logo">Neural<b>CS</b></div>
    <form action="/search" method="get">
        <div class="search-box">
            <input type="text" name="q" placeholder="Search computer science topics..." autofocus autocomplete="off" />
            <button type="submit">Search</button>
        </div>
    </form>
    <p class="tagline">Neural search over 23K+ computer science pages</p>
    <footer>Statement-level neural search &middot; SBERT embeddings &middot; HNSW + BM25 hybrid retrieval</footer>
</body>
</html>"##
        .to_string()
}

pub fn render_results_page(
    query: &str,
    results: &[SearchResult],
    panel: Option<&WikiRecord>,
    elapsed_ms: u128,
) -> String {
    let mut html = String::new();
    html.push_str(&format!(
        r##"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{q} - Neural CS Search</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #fff; color: #202124; }}
        header {{
            border-bottom: 1px solid #e8eaed;
            padding: 16px 24px;
            display: flex;
            align-items: center;
            gap: 24px;
        }}
        .logo-sm {{ font-size: 22px; font-weight: 300; letter-spacing: -1px; text-decoration: none; color: #202124; }}
        .logo-sm b {{ font-weight: 600; color: #1a73e8; }}
        .search-box {{
            display: flex;
            align-items: center;
            border: 1px solid #dfe1e5;
            border-radius: 24px;
            padding: 4px 8px 4px 16px;
            width: 520px;
            max-width: 60vw;
        }}
        .search-box:focus-within {{ box-shadow: 0 1px 6px rgba(32,33,36,0.28); }}
        .search-box input {{
            flex: 1;
            border: none;
            outline: none;
            font-size: 15px;
            padding: 8px 0;
            background: transparent;
        }}
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
        .content {{ display: flex; max-width: 1200px; padding: 0 24px; gap: 40px; align-items: flex-start; }}
        main {{ flex: 1; min-width: 0; max-width: 700px; padding-top: 8px; }}
        aside {{
            width: 320px;
            flex-shrink: 0;
            margin-top: 16px;
            padding: 20px;
            background: #f8f9fa;
            border: 1px solid #e8eaed;
            border-radius: 8px;
        }}
        article {{
            padding: 16px 0;
            border-bottom: 1px solid #f1f3f4;
        }}
        article:last-child {{ border-bottom: none; }}
        .breadcrumbs {{
            color: #5f6368;
            font-size: 12px;
            margin-bottom: 4px;
        }}
        .breadcrumbs .sep {{ color: #bdc1c6; margin: 0 4px; }}
        .result-url {{
            font-size: 12px;
            color: #202124;
            margin-bottom: 2px;
            display: flex;
            align-items: center;
            gap: 8px;
        }}
        .result-url cite {{
            color: #006621;
            font-size: 13px;
            font-style: normal;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
        }}
        a.result-title {{
            font-size: 18px;
            line-height: 1.3;
            color: #1a0dab;
            text-decoration: none;
            display: block;
            margin-bottom: 4px;
        }}
        a.result-title:visited {{ color: #681da8; }}
        a.result-title:hover {{ text-decoration: underline; }}
        .snippet {{
            color: #4d5156;
            font-size: 14px;
            line-height: 1.58;
        }}
        .snippet b {{ color: #202124; font-weight: 600; }}
        .score {{ color: #9aa0a6; font-size: 11px; margin-top: 4px; }}
        .no-results {{ color: #5f6368; font-style: italic; padding: 40px 0; }}
        .panel-title {{ font-size: 20px; font-weight: 600; color: #202124; margin-bottom: 4px; }}
        .panel-desc {{ color: #70757a; font-size: 13px; font-style: italic; margin-bottom: 12px; }}
        .panel-summary {{ color: #4d5156; font-size: 14px; line-height: 1.6; }}
        .panel-summary img {{ max-width: 100%; border-radius: 8px; margin-top: 12px; }}
        .panel-empty {{ color: #9aa0a6; font-size: 13px; }}
    </style>
</head>
<body>
    <header>
        <a href="/" class="logo-sm">Neural<b>CS</b></a>
        <form action="/search" method="get">
            <div class="search-box">
                <input type="text" name="q" value="{q}" autocomplete="off" />
                <button type="submit">Search</button>
            </div>
        </form>
    </header>
"##,
        q = escape_html(query),
    ));

    // Stats bar
    let elapsed_sec = elapsed_ms as f64 / 1000.0;
    html.push_str(&format!(
        r#"<div class="stats">{} results ({:.2} seconds)</div>"#,
        results.len(),
        elapsed_sec,
    ));

    html.push_str(r#"<div class="content"><main>"#);

    if results.is_empty() {
        html.push_str(r#"<p class="no-results">No results found. Try different keywords or a broader query.</p>"#);
    } else {
        let query_terms = extract_query_terms(query);

        for (idx, r) in results.iter().enumerate() {
            let d = tracking::encode_click_payload(query, idx + 1, &r.source_url);

            // Heading breadcrumbs
            let breadcrumbs = if r.heading_chain.is_empty() {
                String::new()
            } else {
                let chain: Vec<_> = r.heading_chain.iter().map(|h| escape_html(h)).collect();
                format!(
                    r#"<div class="breadcrumbs">{}</div>"#,
                    chain.join(r#"<span class="sep">›</span>"#)
                )
            };

            // Display URL
            let display_url = extract_display_url(&r.source_url);

            // Snippet with highlighted query terms
            let snippet = format_snippet_with_highlights(&r.text, &query_terms);

            html.push_str("<article>");
            html.push_str(&breadcrumbs);
            html.push_str(&format!(
                r#"<div class="result-url"><cite>{}</cite></div>"#,
                escape_html(&display_url),
            ));
            html.push_str(&format!(
                r#"<a class="result-title" href="/act?d={}">{}</a>"#,
                escape_html(&d),
                escape_html(&result_title(r)),
            ));
            html.push_str(&format!(r#"<div class="snippet">{}</div>"#, snippet));
            html.push_str(&format!(r#"<div class="score">{:.4}</div>"#, r.score));
            html.push_str("</article>");
        }
    }

    html.push_str("</main>");

    // Knowledge panel sidebar
    html.push_str("<aside>");
    if let Some(k) = panel {
        html.push_str(&format!(
            r#"<div class="panel-title">{}</div>"#,
            escape_html(&k.title)
        ));
        if let Some(desc) = &k.description {
            html.push_str(&format!(
                r#"<div class="panel-desc">{}</div>"#,
                escape_html(desc)
            ));
        }
        html.push_str(&format!(
            r#"<div class="panel-summary">{}</div>"#,
            escape_html(&k.summary)
        ));
        if let Some(image_url) = &k.image_url {
            html.push_str(&format!(
                r#"<img src="{}" style="max-width:100%;border-radius:8px;margin-top:12px" />"#,
                escape_html(image_url)
            ));
        }
    } else {
        html.push_str(
            r#"<div class="panel-empty">No knowledge panel available for this query.</div>"#,
        );
    }
    html.push_str("</aside></div></body></html>");

    html
}

/// Build a display title from the result. Prefer the most specific heading so the
/// visible title reflects the section that actually matched.
fn result_title(r: &SearchResult) -> String {
    if let Some(last) = r.heading_chain.last() {
        if !last.is_empty() {
            return last.clone();
        }
    }
    if let Some(first) = r.heading_chain.first() {
        if !first.is_empty() {
            return first.clone();
        }
    }
    extract_display_url(&r.source_url)
}

fn extract_display_url(url: &str) -> String {
    use url::Url;
    Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str().map(|host| {
                let path = u.path();
                if path == "/" || path.is_empty() {
                    host.to_string()
                } else {
                    format!("{}{}", host, path)
                }
            })
        })
        .unwrap_or_else(|| url.to_string())
}

/// Extract lowercased query terms for highlighting, skipping stop words.
fn extract_query_terms(query: &str) -> HashSet<String> {
    const STOP: &[&str] = &[
        "the", "a", "an", "and", "or", "to", "of", "in", "on", "for", "with", "is", "are", "be",
        "how", "what", "why", "when", "from", "by", "as", "at", "it", "that", "this", "vs",
    ];
    let stop: HashSet<&str> = STOP.iter().copied().collect();
    let mut terms = HashSet::new();
    for word in query.split_whitespace() {
        let clean: String = word
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '+')
            .collect::<String>()
            .to_ascii_lowercase();
        if clean.len() >= 2 && !stop.contains(clean.as_str()) {
            terms.insert(clean);
        }
    }
    terms
}

/// Format a snippet with query terms wrapped in `<b>` tags.
/// The snippet text is HTML-escaped; bold tags are injected safely.
fn format_snippet_with_highlights(text: &str, query_terms: &HashSet<String>) -> String {
    let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let truncated = if cleaned.len() > 300 {
        format!("{}...", &cleaned[..cleaned.floor_char_boundary(297)])
    } else {
        cleaned
    };

    if query_terms.is_empty() {
        return escape_html(&truncated);
    }

    // Split by word boundaries, escape each piece, and bold matches.
    // Iterate by char to handle multi-byte UTF-8 correctly.
    let mut result = String::with_capacity(truncated.len() * 2);
    let mut chars = truncated.char_indices().peekable();

    while let Some(&(i, ch)) = chars.peek() {
        if ch.is_alphanumeric() || ch == '+' {
            // Collect a full word
            let word_start = i;
            let mut word_end = i;
            while let Some(&(j, c)) = chars.peek() {
                if c.is_alphanumeric() || c == '+' {
                    word_end = j + c.len_utf8();
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
            // Non-word character — escape and append
            chars.next();
            let mut buf = [0u8; 4];
            result.push_str(&escape_html(ch.encode_utf8(&mut buf)));
        }
    }

    result
}
