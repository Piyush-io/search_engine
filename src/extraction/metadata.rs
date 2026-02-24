use scraper::{Html, Selector};

/// Metadata extracted from a page's <head>
pub struct PageMeta {
    pub title: Option<String>,
    pub description: Option<String>,
    pub og_title: Option<String>,
    pub og_description: Option<String>,
    pub og_image: Option<String>,
}

impl Default for PageMeta {
    fn default() -> Self {
        Self {
            title: None,
            description: None,
            og_title: None,
            og_description: None,
            og_image: None,
        }
    }
}

/// Extract metadata from raw HTML head tags.
pub fn extract(html: &str) -> PageMeta {
    let doc = Html::parse_document(html);
    let mut out = PageMeta::default();

    let title_sel = Selector::parse("title").unwrap();
    let meta_sel = Selector::parse("meta").unwrap();

    if let Some(title) = doc.select(&title_sel).next() {
        let t = title.text().collect::<String>().trim().to_string();
        if !t.is_empty() {
            out.title = Some(t);
        }
    }

    for meta in doc.select(&meta_sel) {
        let name = meta
            .value()
            .attr("name")
            .map(|v| v.to_ascii_lowercase());
        let property = meta
            .value()
            .attr("property")
            .map(|v| v.to_ascii_lowercase());
        let content = meta
            .value()
            .attr("content")
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(ToOwned::to_owned);

        if let Some(value) = content {
            if name.as_deref() == Some("description") && out.description.is_none() {
                out.description = Some(value.clone());
            }

            match property.as_deref() {
                Some("og:title") if out.og_title.is_none() => out.og_title = Some(value),
                Some("og:description") if out.og_description.is_none() => {
                    out.og_description = Some(value)
                }
                Some("og:image") if out.og_image.is_none() => out.og_image = Some(value),
                _ => {}
            }
        }
    }

    out
}
