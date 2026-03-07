use std::net::IpAddr;
use url::{Url, form_urlencoded};

const MAX_URL_LEN: usize = 2048;

fn contains_control_chars(s: &str) -> bool {
    s.chars().any(|c| {
        let code = c as u32;
        (code <= 0x1F) || (0x7F..=0x9F).contains(&code)
    })
}

/// Canonicalize a raw URL string into a consistent, validated form.
///
/// Current implementation covers strict scheme checks, host normalization,
/// auth/port stripping, query sorting, slash dedupe, control-char rejection,
/// and max length checks.
pub fn canonicalize(raw: &str) -> Option<Url> {
    if raw.is_empty() || contains_control_chars(raw) {
        return None;
    }

    let mut url = Url::parse(raw).ok()?;

    if url.scheme() != "https" {
        return None;
    }

    let host = url.host_str()?.to_ascii_lowercase();

    // Basic hostname sanity guard (placeholder for full eTLD validation).
    if host == "localhost" {
        return None;
    }
    if host.parse::<IpAddr>().is_err() && !host.contains('.') {
        return None;
    }

    url.set_host(Some(&host)).ok()?;

    // Strip credentials and explicit port.
    url.set_username("").ok()?;
    url.set_password(None).ok()?;
    url.set_port(None).ok()?;

    // Remove fragment.
    url.set_fragment(None);

    // Sort query params for deterministic dedupe.
    if url.query().is_some() {
        let mut pairs: Vec<(String, String)> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        pairs.sort_by(|a, b| a.cmp(b));

        if pairs.is_empty() {
            url.set_query(None);
        } else {
            let mut serializer = form_urlencoded::Serializer::new(String::new());
            for (k, v) in pairs {
                serializer.append_pair(&k, &v);
            }
            let sorted = serializer.finish();
            url.set_query(Some(&sorted));
        }
    }

    // Deduplicate trailing slashes (except root path).
    let mut path = url.path().to_string();
    if path.len() > 1 {
        while path.ends_with('/') {
            path.pop();
        }
    }
    if path.is_empty() {
        path.push('/');
    }
    url.set_path(&path);

    let canonical = url.as_str();
    if canonical.len() > MAX_URL_LEN || contains_control_chars(canonical) {
        return None;
    }

    Some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_https() {
        assert!(canonicalize("ftp://example.com/file").is_none());
        assert!(canonicalize("javascript:void(0)").is_none());
    }

    #[test]
    fn lowercases_host() {
        let result = canonicalize("https://Docs.RS/tokio").unwrap();
        assert_eq!(result.host_str().unwrap(), "docs.rs");
    }

    #[test]
    fn strips_trailing_slash() {
        let a = canonicalize("https://example.com/path/").unwrap();
        let b = canonicalize("https://example.com/path").unwrap();
        assert_eq!(a.as_str(), b.as_str());
    }

    #[test]
    fn sorts_query_parameters() {
        let u = canonicalize("https://example.com/search?z=9&a=1").unwrap();
        assert_eq!(u.as_str(), "https://example.com/search?a=1&z=9");
    }
}
