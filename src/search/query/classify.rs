/// Detect queries that look like technical identifiers (config keys, acronyms,
/// namespace paths). These queries benefit from exact lexical matching and are
/// often hurt by semantic reranking that boosts conceptual similarity over
/// token-level precision.
pub(super) fn is_identifier_query(query_text: &str) -> bool {
    let lower = query_text.to_lowercase();
    // Contains underscore (e.g. wal_level, full_page_writes)
    if lower.contains('_') {
        return true;
    }
    // Contains namespace separator (e.g. std::vector, pg_catalog::pg_class)
    if lower.contains("::") {
        return true;
    }
    // Contains a short all-caps acronym (e.g. WAL, LSN, XID, CID)
    for word in query_text.split_whitespace() {
        let clean: String = word.chars().filter(|c| c.is_ascii_alphabetic()).collect();
        if clean.len() >= 2 && clean.len() <= 5 && clean.chars().all(|c| c.is_ascii_uppercase()) {
            return true;
        }
    }
    false
}
