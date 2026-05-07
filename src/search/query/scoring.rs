use std::collections::HashSet;

use crate::ChunkId;

pub(super) struct Candidate {
    pub chunk_id: ChunkId,
    pub text: String,
    pub source_url: String,
    pub heading_chain: Vec<String>,
    pub final_score: f32,
    pub vector_score: f32,
    pub lexical_score: f32,
    pub title_overlap: f32,
    pub heading_overlap: f32,
    pub body_overlap: f32,
    pub authority_bonus_val: f32,
    pub exact_heading_phrase: bool,
    pub exact_body_phrase: bool,
    pub heading_match_count: usize,
    pub overall_match_count: usize,
    pub query_token_count: usize,
}

pub(super) fn domain_authority_bonus(
    query_tokens: &HashSet<String>,
    host: &str,
    bonus: f32,
) -> f32 {
    const AUTHORITY: &[(&str, &[&str])] = &[
        ("rust", &["doc.rust-lang.org", "docs.rust-lang.org"]),
        ("python", &["docs.python.org"]),
        ("go", &["go.dev", "pkg.go.dev"]),
        ("java", &["docs.oracle.com"]),
        ("javascript", &["developer.mozilla.org"]),
        ("typescript", &["www.typescriptlang.org"]),
        ("linux", &["kernel.org", "man7.org"]),
        ("git", &["git-scm.com"]),
        ("sql", &["www.postgresql.org", "dev.mysql.com"]),
        (
            "postgres",
            &[
                "www.postgresql.org",
                "postgresql.org",
                "wiki.postgresql.org",
            ],
        ),
        (
            "postgresql",
            &[
                "www.postgresql.org",
                "postgresql.org",
                "wiki.postgresql.org",
            ],
        ),
        ("sqlite", &["sqlite.org", "www.sqlite.org"]),
        (
            "wal",
            &["www.postgresql.org", "postgresql.org", "sqlite.org"],
        ),
        ("mvcc", &["www.postgresql.org", "postgresql.org"]),
        ("xid", &["www.postgresql.org", "postgresql.org"]),
        ("http", &["developer.mozilla.org", "httpwg.org"]),
        ("css", &["developer.mozilla.org"]),
        ("html", &["developer.mozilla.org"]),
        ("haskell", &["www.haskell.org", "hackage.haskell.org"]),
        ("c++", &["en.cppreference.com"]),
        ("cpp", &["en.cppreference.com"]),
    ];

    let host = super::dedupe::canonical_host(host);

    for (keyword, canonical_hosts) in AUTHORITY {
        if query_tokens.contains(*keyword) {
            if canonical_hosts
                .iter()
                .map(|h| super::dedupe::canonical_host(h))
                .any(|h| host == h || host.ends_with(&format!(".{h}")))
            {
                return bonus;
            }
        }
    }
    0.0
}
