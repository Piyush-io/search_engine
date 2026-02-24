/// Heuristic statement chaining fallback.
///
/// If a sentence starts with a reference/pronoun cue, prepend the previous
/// sentence to make it more self-contained. Returns `(text, is_leaf)`.
pub fn apply_statement_chaining(sentence: &str, preceding: &[String]) -> (String, bool) {
    let lowered = sentence.trim().to_ascii_lowercase();
    let cues = [
        "it ", "this ", "that ", "these ", "those ", "therefore", "however", "thus", "they ",
    ];

    let needs_context = cues.iter().any(|cue| lowered.starts_with(cue));
    if needs_context {
        if let Some(prev) = preceding.last() {
            return (format!("{} {}", prev.trim(), sentence.trim()), true);
        }
    }

    (sentence.to_string(), true)
}
