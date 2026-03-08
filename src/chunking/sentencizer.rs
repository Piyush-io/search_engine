/// Split text into sentence-like chunks.
///
/// Heuristic splitter that handles common false boundaries:
/// - Abbreviations (e.g., i.e., Dr., Mr., etc., vs., Prof., al.)
/// - Decimal numbers (3.14, 0.001)
/// - URLs (http://..., ftp://...)
/// - Ellipses (...)
/// - Single-letter initials (A. B. Smith)
///
/// Upgrades to an SRX-based splitter are planned for later.
pub fn split_sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut out = Vec::new();
    let mut start = 0;

    let mut i = 0;
    while i < len {
        let ch = chars[i];

        if ch == '\n' {
            // Newlines always split (paragraph / list boundary).
            let sentence = normalize_sentence(&chars[start..=i]);
            if !sentence.is_empty() {
                out.push(sentence);
            }
            start = i + 1;
            i += 1;
            continue;
        }

        if matches!(ch, '!' | '?') {
            // Unambiguous sentence-ending punctuation.
            let sentence = normalize_sentence(&chars[start..=i]);
            if !sentence.is_empty() {
                out.push(sentence);
            }
            start = i + 1;
            i += 1;
            continue;
        }

        if ch == '.' {
            // Check if this period is a real sentence boundary.
            if is_sentence_boundary(&chars, i) {
                let sentence = normalize_sentence(&chars[start..=i]);
                if !sentence.is_empty() {
                    out.push(sentence);
                }
                start = i + 1;
            }
            i += 1;
            continue;
        }

        i += 1;
    }

    // Trailing text without terminal punctuation.
    if start < len {
        let sentence = normalize_sentence(&chars[start..len]);
        if !sentence.is_empty() {
            out.push(sentence);
        }
    }

    out
}

/// Common abbreviations that end with a period but are NOT sentence boundaries.
const ABBREVIATIONS: &[&str] = &[
    "e.g", "i.e", "etc", "vs", "dr", "mr", "mrs", "ms", "prof", "sr", "jr", "al", "fig", "eq",
    "no", "vol", "rev", "dept", "approx", "incl", "est", "st", "ave", "blvd", "gen", "gov", "sgt",
    "cpl", "pvt", "capt", "lt", "col", "maj", "cmdr", "adm", "jan", "feb", "mar", "apr", "jun",
    "jul", "aug", "sep", "oct", "nov", "dec", "cf", "viz",
];

/// Decide whether the period at `pos` is a true sentence boundary.
fn is_sentence_boundary(chars: &[char], pos: usize) -> bool {
    let len = chars.len();

    // --- Ellipsis: "..." ---
    // Non-final dots in a sequence are never boundaries.
    if pos + 1 < len && chars[pos + 1] == '.' {
        return false;
    }
    // Final dot in an ellipsis: treat as boundary only if followed by
    // whitespace + uppercase (i.e., a new sentence starts after it).
    if pos > 0 && chars[pos - 1] == '.' {
        if let Some(&after) = peek_past_whitespace(chars, pos + 1) {
            return after.is_ascii_uppercase();
        }
        return true; // end of text
    }

    // --- Nothing after the period (end of text) → boundary ---
    if pos + 1 >= len {
        return true;
    }

    let next = chars[pos + 1];

    // If the next character is NOT whitespace or end-of-text, it's likely
    // mid-token (e.g. "3.14", "example.com", "U.S.A.").
    if !next.is_whitespace() {
        return false;
    }

    // --- Look back to find the word preceding the period ---
    let word_before = word_before_period(chars, pos);

    // Single-letter initial: "A. Smith", "J. K. Rowling"
    if word_before.len() == 1
        && word_before
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
    {
        return false;
    }

    // Known abbreviation list (case-insensitive)
    let lower = word_before.to_ascii_lowercase();
    if ABBREVIATIONS.contains(&lower.as_str()) {
        return false;
    }

    // Multi-period abbreviation: "U.S.", "Ph.D." — the word_before would be
    // something like "U.S" or "Ph.D".
    if word_before.contains('.') {
        return false;
    }

    // Decimal number: "3.14" — digit immediately before the period.
    // Already handled by the mid-token check above (no whitespace after),
    // but handle the edge case "scored 3. " which IS a boundary but rare.

    // If the character after the space is lowercase, it's probably not a new
    // sentence.  "called fn. to" → not a boundary.
    if let Some(&after_space) = peek_past_whitespace(chars, pos + 1) {
        // Lowercase letter after space → likely not a sentence break.
        // Exception: could be a legit sentence starting with a lowercase word,
        // but that's rare enough to accept as a trade-off.
        if after_space.is_ascii_lowercase() {
            return false;
        }
    }

    true
}

/// Extract the word immediately before the period at `pos`.
fn word_before_period(chars: &[char], pos: usize) -> String {
    if pos == 0 {
        return String::new();
    }
    let end = pos; // exclusive
    let mut j = pos as isize - 1;
    while j >= 0 {
        let c = chars[j as usize];
        if c.is_whitespace() {
            break;
        }
        j -= 1;
    }
    let start = (j + 1) as usize;
    chars[start..end].iter().collect()
}

/// Peek past whitespace to find the first non-whitespace char after `from`.
fn peek_past_whitespace(chars: &[char], from: usize) -> Option<&char> {
    chars[from..].iter().find(|c| !c.is_whitespace())
}

fn normalize_sentence(chars: &[char]) -> String {
    let raw: String = chars.iter().collect();
    let compact = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.trim().to_string()
}

/// Merge sentences into sliding-window chunks.
///
/// `window_size` controls how many sentences per chunk (default 3).
/// `overlap` controls how many sentences overlap between adjacent windows (default 1).
/// Returns merged sentence strings; single sentences are returned as-is when there
/// aren't enough to fill a window.
pub fn merge_windows(sentences: &[String], window_size: usize, overlap: usize) -> Vec<String> {
    if sentences.is_empty() {
        return Vec::new();
    }

    let window_size = window_size.max(1);
    let overlap = overlap.min(window_size.saturating_sub(1));
    let stride = window_size - overlap;

    if sentences.len() <= window_size {
        return vec![sentences.join(" ")];
    }

    let mut windows = Vec::new();
    let mut start = 0;

    while start < sentences.len() {
        let end = (start + window_size).min(sentences.len());
        windows.push(sentences[start..end].join(" "));

        let next = start + stride;
        if next >= sentences.len() {
            break;
        }

        // If the remaining sentences can't fill a full window, merge them
        // into one final chunk to avoid tiny tail windows.
        if sentences.len() - next < window_size && end < sentences.len() {
            windows.push(sentences[next..].join(" "));
            break;
        }

        start = next;
    }

    windows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_basic_text() {
        let s = "Rust is memory-safe. It is fast! Is it ergonomic?";
        let chunks = split_sentences(s);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], "Rust is memory-safe.");
        assert_eq!(chunks[1], "It is fast!");
        assert_eq!(chunks[2], "Is it ergonomic?");
    }

    #[test]
    fn preserves_abbreviations() {
        let s = "For example, e.g. this should not split. But this is new.";
        let chunks = split_sentences(s);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].contains("e.g."));
    }

    #[test]
    fn preserves_decimal_numbers() {
        let s = "The value is 3.14 approximately. Done.";
        let chunks = split_sentences(s);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].contains("3.14"));
    }

    #[test]
    fn preserves_urls() {
        let s = "Visit http://example.com for more. Another sentence.";
        let chunks = split_sentences(s);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].contains("http://example.com"));
    }

    #[test]
    fn handles_ellipsis() {
        let s = "Wait for it... The answer is here.";
        let chunks = split_sentences(s);
        assert_eq!(chunks.len(), 2);
    }

    #[test]
    fn handles_initials() {
        let s = "Written by J. K. Rowling. The book is great.";
        let chunks = split_sentences(s);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].contains("J. K. Rowling."));
    }

    #[test]
    fn handles_newlines_as_splits() {
        let s = "First line\nSecond line\nThird line";
        let chunks = split_sentences(s);
        assert_eq!(chunks.len(), 3);
    }

    #[test]
    fn empty_input() {
        let chunks = split_sentences("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn whitespace_only() {
        let chunks = split_sentences("   \n  \n  ");
        assert!(chunks.is_empty());
    }

    #[test]
    fn handles_et_al() {
        let s = "According to Smith et al. the results were significant. More details follow.";
        let chunks = split_sentences(s);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].contains("et al."));
    }

    #[test]
    fn handles_lowercase_continuation() {
        let s = "Use fn. to define functions in Rust. Structs are different.";
        let chunks = split_sentences(s);
        // "fn. to" should NOT split because 'to' is lowercase
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].contains("fn. to"));
    }

    #[test]
    fn merge_windows_basic() {
        let s: Vec<String> = vec![
            "A.".into(), "B.".into(), "C.".into(), "D.".into(), "E.".into(),
        ];
        let windows = merge_windows(&s, 3, 1);
        assert_eq!(windows.len(), 3);
        assert_eq!(windows[0], "A. B. C.");
        assert_eq!(windows[1], "C. D. E.");
    }

    #[test]
    fn merge_windows_small_input() {
        let s: Vec<String> = vec!["A.".into(), "B.".into()];
        let windows = merge_windows(&s, 3, 1);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0], "A. B.");
    }

    #[test]
    fn merge_windows_single() {
        let s: Vec<String> = vec!["A.".into()];
        let windows = merge_windows(&s, 3, 1);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0], "A.");
    }
}
