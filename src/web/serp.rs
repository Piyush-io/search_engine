use crate::{SearchResult, knowledge::wikipedia::WikiRecord, web::tracking};

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn render_home_page() -> String {
    r#"<!doctype html>
<html>
<head><meta charset="utf-8"><title>Neural CS Search</title></head>
<body>
  <h1>Neural Search Engine (CS)</h1>
  <form action="/search" method="get">
    <input type="text" name="q" placeholder="Search computer science topics" style="width: 420px" />
    <button type="submit">Search</button>
  </form>
</body>
</html>"#
        .to_string()
}

pub fn render_results_page(query: &str, results: &[SearchResult], panel: Option<&WikiRecord>) -> String {
    let mut html = String::new();
    html.push_str("<!doctype html><html><head><meta charset=\"utf-8\"><title>Results</title></head><body>");
    html.push_str("<form action=\"/search\" method=\"get\">");
    html.push_str(&format!(
        "<input type=\"text\" name=\"q\" value=\"{}\" style=\"width:420px\"/>",
        escape_html(query)
    ));
    html.push_str("<button type=\"submit\">Search</button></form>");

    html.push_str("<div style=\"display:flex;gap:24px;align-items:flex-start;margin-top:20px\">");
    html.push_str("<main style=\"flex:2\">");

    if results.is_empty() {
        html.push_str("<p>No results found.</p>");
    } else {
        for (idx, r) in results.iter().enumerate() {
            let d = tracking::encode_click_payload(query, idx + 1, &r.source_url);
            let context = if r.heading_chain.is_empty() {
                String::new()
            } else {
                format!("<div style=\"color:#666;font-size:12px\">{}</div>", escape_html(&r.heading_chain.join(" › ")))
            };
            html.push_str("<article style=\"margin:14px 0;padding-bottom:10px;border-bottom:1px solid #ddd\">");
            html.push_str(&context);
            html.push_str(&format!(
                "<a href=\"/act?d={}\" style=\"font-size:18px\">{}</a>",
                escape_html(&d),
                escape_html(&r.source_url)
            ));
            html.push_str(&format!(
                "<p>{}</p><small>score: {:.4}</small>",
                escape_html(&r.text),
                r.score
            ));
            html.push_str("</article>");
        }
    }

    html.push_str("</main>");

    html.push_str("<aside style=\"flex:1;border-left:1px solid #ddd;padding-left:16px\">");
    if let Some(k) = panel {
        html.push_str("<h3>Knowledge Panel</h3>");
        html.push_str(&format!("<strong>{}</strong>", escape_html(&k.title)));
        if let Some(desc) = &k.description {
            html.push_str(&format!("<p><em>{}</em></p>", escape_html(desc)));
        }
        html.push_str(&format!("<p>{}</p>", escape_html(&k.summary)));
        if let Some(image_url) = &k.image_url {
            html.push_str(&format!("<img src=\"{}\" style=\"max-width:100%\"/>", escape_html(image_url)));
        }
    } else {
        html.push_str("<h3>Knowledge Panel</h3><p>No panel match.</p>");
    }
    html.push_str("</aside></div></body></html>");

    html
}
