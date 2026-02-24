/// Split text into sentence-like chunks.
///
/// This is a lightweight fallback splitter until `srx` is wired in.
pub fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();

    for ch in text.chars() {
        buf.push(ch);
        if matches!(ch, '.' | '!' | '?' | '\n') {
            let sentence = normalize_sentence(&buf);
            if !sentence.is_empty() {
                out.push(sentence);
            }
            buf.clear();
        }
    }

    if !buf.trim().is_empty() {
        let sentence = normalize_sentence(&buf);
        if !sentence.is_empty() {
            out.push(sentence);
        }
    }

    out
}

fn normalize_sentence(raw: &str) -> String {
    let compact = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_basic_text() {
        let s = "Rust is memory-safe. It is fast! Is it ergonomic?";
        let chunks = split_sentences(s);
        assert_eq!(chunks.len(), 3);
    }
}
