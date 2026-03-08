/// Attach heading context to a sentence for embedding.
pub fn with_context(heading_chain: &[String], sentence: &str) -> String {
    if heading_chain.is_empty() {
        return sentence.to_string();
    }

    format!("{}\n{}", heading_chain.join("\n"), sentence)
}

/// Attach only up to `depth` levels from the end of heading chain.
pub fn with_context_depth(heading_chain: &[String], sentence: &str, depth: usize) -> String {
    if heading_chain.is_empty() || depth == 0 {
        return sentence.to_string();
    }

    let len = heading_chain.len();
    let start = len.saturating_sub(depth);
    with_context(&heading_chain[start..], sentence)
}

/// Build embedding text with page title, heading chain, and body text.
/// Used to create richer embeddings while keeping display text clean.
pub fn with_embed_context(
    page_title: Option<&str>,
    heading_chain: &[String],
    body_text: &str,
    depth: usize,
) -> String {
    let mut parts = Vec::new();

    if let Some(title) = page_title {
        if !title.is_empty() {
            parts.push(title.to_string());
        }
    }

    if !heading_chain.is_empty() && depth > 0 {
        let len = heading_chain.len();
        let start = len.saturating_sub(depth);
        for h in &heading_chain[start..] {
            // Avoid duplicating the page title
            if parts.first().map(|p| p.as_str()) != Some(h.as_str()) {
                parts.push(h.clone());
            }
        }
    }

    parts.push(body_text.to_string());
    parts.join("\n")
}
