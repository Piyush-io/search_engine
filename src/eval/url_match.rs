use url::Url;

pub fn canonical_doc_key(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let Ok(mut url) = Url::parse(trimmed) else {
        return trimmed.to_ascii_lowercase();
    };

    url.set_fragment(None);
    url.set_query(None);

    if let Some(host) = url.host_str() {
        let normalized_host = normalize_host(host);
        let _ = url.set_host(Some(&normalized_host));
    }

    let mut path = collapse_slashes(url.path());
    if path.len() > 1 && path.ends_with('/') {
        path.pop();
    }

    if let Some(host) = url.host_str() {
        path = normalize_postgres_docs_path(host, &path);
        path = normalize_rust_docs_path(host, &path);
    }

    url.set_path(&path);

    if (url.scheme() == "https" && url.port() == Some(443))
        || (url.scheme() == "http" && url.port() == Some(80))
    {
        let _ = url.set_port(None);
    }

    url.to_string()
}

fn normalize_host(host: &str) -> String {
    let lowered = host.to_ascii_lowercase();
    let stripped = lowered.strip_prefix("www.").unwrap_or(&lowered);
    if matches!(stripped, "docs.rust-lang.org" | "doc.rust-lang.org") {
        return "doc.rust-lang.org".to_string();
    }
    stripped.to_string()
}

fn normalize_postgres_docs_path(host: &str, path: &str) -> String {
    if !matches!(host, "postgresql.org" | "www.postgresql.org") {
        return path.to_string();
    }

    let mut parts = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
    if parts.len() < 3 || parts[0] != "docs" {
        return path.to_string();
    }

    if is_pg_docs_version_segment(parts[1]) {
        parts[1] = "current";
    }

    if parts.len() >= 4 && parts[2] == "static" {
        parts.remove(2);
    }

    format!("/{}", parts.join("/"))
}

fn normalize_rust_docs_path(host: &str, path: &str) -> String {
    if !matches!(host, "doc.rust-lang.org" | "docs.rust-lang.org") {
        return path.to_string();
    }

    let parts = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
    if parts.is_empty() {
        return "/".to_string();
    }

    let stripped = if matches!(parts[0], "stable" | "nightly" | "beta") {
        &parts[1..]
    } else {
        &parts[..]
    };

    if stripped.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", stripped.join("/"))
    }
}

fn is_pg_docs_version_segment(seg: &str) -> bool {
    if matches!(seg, "current" | "devel" | "latest" | "stable") {
        return true;
    }

    let mut has_digit = false;
    for ch in seg.chars() {
        if ch.is_ascii_digit() {
            has_digit = true;
            continue;
        }
        if ch != '.' {
            return false;
        }
    }
    has_digit
}

fn collapse_slashes(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut last_was_slash = false;
    for ch in path.chars() {
        if ch == '/' {
            if !last_was_slash {
                out.push(ch);
            }
            last_was_slash = true;
        } else {
            out.push(ch);
            last_was_slash = false;
        }
    }
    if out.is_empty() {
        "/".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::canonical_doc_key;

    #[test]
    fn normalizes_www_host_aliases() {
        let a = canonical_doc_key("https://www.postgresql.org/docs/current/wal-intro.html");
        let b = canonical_doc_key("https://postgresql.org/docs/current/wal-intro.html");
        assert_eq!(a, b);
    }

    #[test]
    fn normalizes_postgres_versioned_docs() {
        let a = canonical_doc_key("https://postgresql.org/docs/17/wal-intro.html");
        let b = canonical_doc_key("https://postgresql.org/docs/current/wal-intro.html");
        assert_eq!(a, b);
    }

    #[test]
    fn normalizes_postgres_devel_and_static() {
        let a = canonical_doc_key("https://www.postgresql.org/docs/devel/static/wal-intro.html");
        let b = canonical_doc_key("https://postgresql.org/docs/current/wal-intro.html");
        assert_eq!(a, b);
    }

    #[test]
    fn strips_query_and_fragment_for_non_pg() {
        let a = canonical_doc_key("https://sqlite.org/wal.html?x=1#top");
        let b = canonical_doc_key("https://sqlite.org/wal.html");
        assert_eq!(a, b);
    }

    #[test]
    fn normalizes_rust_doc_channel_paths() {
        let a = canonical_doc_key("https://doc.rust-lang.org/stable/reference/lifetime-elision.html");
        let b = canonical_doc_key("https://docs.rust-lang.org/nightly/reference/lifetime-elision.html");
        assert_eq!(a, b);
    }
}
