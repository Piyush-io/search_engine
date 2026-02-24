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
