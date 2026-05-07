use std::collections::HashSet;

pub(super) const SYNONYMS: &[(&[&str], &[&str])] = &[
    (&["js"], &["javascript"]),
    (&["javascript"], &["js"]),
    (&["ts"], &["typescript"]),
    (&["typescript"], &["ts"]),
    (&["cpp", "c++"], &["c++"]),
    (&["auth"], &["authentication"]),
    (&["authentication"], &["auth"]),
    (&["gpu"], &["cuda", "graphics"]),
    (&["cuda"], &["gpu"]),
    (&["db"], &["database"]),
    (&["database"], &["db"]),
    (&["ml"], &["machine learning"]),
    (&["ai"], &["artificial intelligence"]),
    (&["api"], &["interface", "endpoint"]),
    (&["oop"], &["object oriented"]),
    (&["fp"], &["functional programming"]),
    (&["os"], &["operating system"]),
    (&["cli"], &["command line"]),
    (&["regex"], &["regular expression"]),
    (&["async"], &["asynchronous"]),
    (&["sync"], &["synchronous"]),
];

pub(super) fn expand_query_tokens(tokens: &HashSet<String>) -> HashSet<String> {
    let mut expanded = tokens.clone();
    for (triggers, expansions) in SYNONYMS {
        if triggers.iter().any(|t| tokens.contains(*t)) {
            for exp in *expansions {
                for word in exp.split_whitespace() {
                    if word.len() >= 2 {
                        expanded.insert(word.to_ascii_lowercase());
                    }
                }
            }
        }
    }
    expanded
}

pub(super) fn build_expanded_query_text(original: &str, tokens: &HashSet<String>) -> String {
    let expanded = expand_query_tokens(tokens);
    let new_terms: Vec<&String> = expanded.iter().filter(|t| !tokens.contains(*t)).collect();
    if new_terms.is_empty() {
        return original.to_string();
    }
    format!(
        "{} {}",
        original,
        new_terms
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    )
}

pub(super) fn tokenize_set(text: &str) -> HashSet<String> {
    const STOP: &[&str] = &[
        "the", "a", "an", "and", "or", "to", "of", "in", "on", "for", "with", "is", "are", "be",
        "how", "what", "why", "when", "from", "by", "as", "at", "it", "that", "this", "vs",
    ];

    let stop: HashSet<&str> = STOP.iter().copied().collect();
    let mut out = HashSet::new();
    let mut cur = String::new();

    for raw_piece in text.split_whitespace() {
        let compact = compact_token(raw_piece);
        maybe_insert_token(&mut out, &stop, compact);
    }

    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '+' {
            cur.push(ch.to_ascii_lowercase());
        } else if !cur.is_empty() {
            maybe_insert_token(&mut out, &stop, std::mem::take(&mut cur));
            cur.clear();
        }
    }

    if !cur.is_empty() {
        maybe_insert_token(&mut out, &stop, cur);
    }

    let normalized = normalize_phrase(text);
    if normalized.contains("b tree") || text.to_ascii_lowercase().contains("btree") {
        out.insert("btree".to_string());
    }

    out
}

fn compact_token(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn maybe_insert_token(tokens: &mut HashSet<String>, stop: &HashSet<&str>, token: String) {
    if token.len() >= 2 && !stop.contains(token.as_str()) {
        tokens.insert(token);
    }
}

pub(super) fn normalize_phrase(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch.is_ascii_whitespace() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn contains_phrase(query_phrase: &str, text: &str) -> bool {
    if query_phrase.is_empty() {
        return false;
    }
    normalize_phrase(text).contains(query_phrase)
}

pub(super) fn token_overlap(query_tokens: &HashSet<String>, text: &str) -> f32 {
    if query_tokens.is_empty() {
        return 0.0;
    }

    let tokens = tokenize_set(text);
    if tokens.is_empty() {
        return 0.0;
    }

    let matched = query_tokens.iter().filter(|t| tokens.contains(*t)).count() as f32;
    matched / (query_tokens.len() as f32)
}

pub(super) fn token_match_count(query_tokens: &HashSet<String>, text: &str) -> usize {
    if query_tokens.is_empty() {
        return 0;
    }

    let tokens = tokenize_set(text);
    if tokens.is_empty() {
        return 0;
    }

    query_tokens.iter().filter(|t| tokens.contains(*t)).count()
}

pub(super) fn combined_token_match_count(query_tokens: &HashSet<String>, texts: &[&str]) -> usize {
    if query_tokens.is_empty() {
        return 0;
    }

    let mut combined_tokens = HashSet::new();
    for text in texts {
        combined_tokens.extend(tokenize_set(text));
    }

    query_tokens
        .iter()
        .filter(|token| combined_tokens.contains(*token))
        .count()
}
