/// Heuristic statement chaining fallback.
///
/// - If a sentence starts with anaphora/connector cues, prepend the previous
///   sentence when available.
/// - Introductory sentences (lead-ins) are marked non-leaf and should not be
///   directly surfaced as final snippets.
pub fn apply_statement_chaining(sentence: &str, preceding: &[String]) -> (String, bool) {
    let trimmed = sentence.trim();
    let lowered = trimmed.to_ascii_lowercase();

    let pronoun_cues = [
        "it ", "this ", "that ", "these ", "those ", "they ", "he ", "she ",
    ];
    let connector_cues = [
        "therefore",
        "however",
        "thus",
        "so ",
        "then ",
        "instead",
        "also",
    ];

    let needs_context = pronoun_cues.iter().any(|cue| lowered.starts_with(cue))
        || connector_cues.iter().any(|cue| lowered.starts_with(cue));

    let looks_intro = lowered.ends_with(':')
        || lowered.starts_with("here are")
        || lowered.starts_with("for example")
        || lowered.starts_with("note:")
        || lowered.starts_with("in summary");

    if looks_intro {
        return (trimmed.to_string(), false);
    }

    if needs_context {
        if let Some(prev) = preceding.last() {
            return (format!("{} {}", prev.trim(), trimmed), true);
        }

        // Needs context but none available: keep sentence, but avoid surfacing
        // as a standalone leaf match.
        return (trimmed.to_string(), false);
    }

    (trimmed.to_string(), true)
}
