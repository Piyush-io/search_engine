/// Return true if a block's text content is dense enough to keep.
///
/// Rules:
/// - text/html ratio > 50%
/// - anchor-text share < 20%
pub fn keep(text_bytes: usize, html_bytes: usize, anchor_text_bytes: usize) -> bool {
    if html_bytes == 0 || text_bytes == 0 {
        return false;
    }

    let text_ratio = text_bytes as f64 / html_bytes as f64;
    let anchor_ratio = anchor_text_bytes as f64 / text_bytes as f64;

    text_ratio > 0.50 && anchor_ratio < 0.20
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_dense_low_anchor_blocks() {
        assert!(keep(80, 120, 8));
    }

    #[test]
    fn drops_link_heavy_blocks() {
        assert!(!keep(100, 120, 30));
    }
}
