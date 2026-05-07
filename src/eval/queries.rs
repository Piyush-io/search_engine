use std::io::BufRead;

/// Query classification buckets for per-bucket metrics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueryBucket {
    /// Exact identifier queries (config parameters, function names)
    ExactIdentifier,
    /// Acronym and term disambiguation queries
    AcronymDisambiguation,
    /// Conceptual paraphrase queries
    ConceptualParaphrase,
    /// Mixed long-form queries
    LongFormMixed,
    /// Unclassified/default bucket
    Other,
}

impl serde::Serialize for QueryBucket {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl QueryBucket {
    pub fn as_str(&self) -> &'static str {
        match self {
            QueryBucket::ExactIdentifier => "exact_identifier",
            QueryBucket::AcronymDisambiguation => "acronym_disambiguation",
            QueryBucket::ConceptualParaphrase => "conceptual_paraphrase",
            QueryBucket::LongFormMixed => "long_form_mixed",
            QueryBucket::Other => "other",
        }
    }
}

/// A query with its classification bucket
#[derive(Debug, Clone)]
pub struct Query {
    pub id: String,
    pub text: String,
    pub bucket: QueryBucket,
}

/// Classify a query into a bucket based on its characteristics
pub fn classify_query(query_text: &str) -> QueryBucket {
    let text_lower = query_text.to_ascii_lowercase();
    let tokens: Vec<&str> = text_lower.split_whitespace().collect();
    let token_count = tokens.len();

    // Known database-related acronyms
    const ACRONYMS: &[&str] = &[
        "wal", "xid", "cid", "txid", "lsn", "ctid", "oid", "mvcc", "gin", "gist", "btree", "hash",
        "fts5", "rtree", "cte", "json1", "mmap",
    ];

    // Check for acronym disambiguation first (before long-form check)
    let has_acronym = ACRONYMS.iter().any(|acronym| {
        text_lower.contains(&format!(" {} ", acronym))
            || text_lower.starts_with(&format!("{} ", acronym))
            || text_lower.ends_with(&format!(" {}", acronym))
            || text_lower == *acronym
    });

    // Check for disambiguation patterns
    let is_disambiguation = text_lower.contains(" vs ")
        || text_lower.contains(" versus ")
        || text_lower.contains(" difference ")
        || text_lower.contains(" vs. ");

    // Prioritize acronym disambiguation for short queries with acronyms
    if has_acronym && (is_disambiguation || token_count <= 5) {
        return QueryBucket::AcronymDisambiguation;
    }

    // Check for long-form queries (many tokens or multiple clauses)
    // Note: Exclude vs/versus/difference when an acronym is present (handled above)
    if token_count > 10
        || text_lower.contains(" with ")
        || text_lower.contains(" and ")
        || (!has_acronym && is_disambiguation)
    {
        return QueryBucket::LongFormMixed;
    }

    // Check for exact identifier queries
    // These are typically short (1-3 tokens) and look like config parameters or identifiers
    if token_count <= 3 {
        // Check if it looks like a config parameter (contains underscore or is short technical term)
        let looks_like_identifier =
            text_lower.contains('_') || (token_count <= 2 && !text_lower.contains(' '));
        if looks_like_identifier {
            return QueryBucket::ExactIdentifier;
        }
    }

    // Check for conceptual paraphrase queries
    // These often start with question words or contain "how does", "what is", "why does"
    let conceptual_starters = [
        "how ",
        "why ",
        "what ",
        "explain ",
        "understand ",
        "describe ",
    ];
    if conceptual_starters
        .iter()
        .any(|starter| text_lower.starts_with(starter))
    {
        return QueryBucket::ConceptualParaphrase;
    }

    // Default based on length
    if token_count <= 4 {
        QueryBucket::ExactIdentifier
    } else if has_acronym {
        QueryBucket::AcronymDisambiguation
    } else if token_count >= 6 {
        QueryBucket::ConceptualParaphrase
    } else {
        QueryBucket::Other
    }
}

/// Load queries with their classification buckets
pub fn load_queries_with_buckets(path: &str) -> Result<Vec<Query>, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut queries = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() < 2 {
            continue;
        }

        let id = parts[0].to_string();
        let text = parts[1].to_string();
        let bucket = classify_query(&text);

        queries.push(Query { id, text, bucket });
    }

    Ok(queries)
}

/// Load queries without classification (backward compatible)
pub fn load_queries(path: &str) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut queries = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() < 2 {
            continue;
        }

        queries.push((parts[0].to_string(), parts[1].to_string()));
    }

    Ok(queries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_exact_identifier() {
        assert_eq!(
            classify_query("wal_level postgresql"),
            QueryBucket::ExactIdentifier
        );
        assert_eq!(
            classify_query("full_page_writes"),
            QueryBucket::ExactIdentifier
        );
    }

    #[test]
    fn test_classify_acronym() {
        assert_eq!(
            classify_query("xid postgres transaction id"),
            QueryBucket::AcronymDisambiguation
        );
        assert_eq!(
            classify_query("wal vs checkpoint"),
            QueryBucket::AcronymDisambiguation
        );
    }

    #[test]
    fn test_classify_conceptual() {
        assert_eq!(
            classify_query("how does postgres handle concurrent writes"),
            QueryBucket::ConceptualParaphrase
        );
        assert_eq!(
            classify_query("what prevents dirty reads in sqlite"),
            QueryBucket::ConceptualParaphrase
        );
    }

    #[test]
    fn test_classify_long_form() {
        assert_eq!(
            classify_query("explain postgres mvcc transaction isolation levels with examples"),
            QueryBucket::LongFormMixed
        );
    }
}
