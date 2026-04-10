use crate::crawler::types::{FetchResult, RejectReason, UrlTask};
use crate::crawler::{canon, dns, policy, robots};
use crate::{pipeline::PageState, storage};
use dashmap::DashMap;
use reqwest::{
    Client,
    header::{ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED},
};
use url::Url;

pub async fn fetch_task(
    db: &rocksdb::DB,
    client: &Client,
    robots_cache: &robots::RobotsCache,
    dns_ok_cache: &DashMap<String, bool>,
    task: UrlTask,
) -> Result<FetchResult, RejectReason> {
    let Ok(url) = Url::parse(&task.url) else {
        return Err(RejectReason::BadUrl);
    };

    let dns_pass = match dns_ok_cache.get(&task.host).map(|v| *v) {
        Some(v) => v,
        None => {
            let ok = dns::resolve_and_check(&task.host).await.is_ok();
            dns_ok_cache.insert(task.host.clone(), ok);
            ok
        }
    };
    if !dns_pass {
        return Err(RejectReason::DnsFailed);
    }

    let disallowed = robots::get_disallowed(&task.host, db, client, robots_cache)
        .await
        .unwrap_or_default();
    if !robots::is_allowed(url.path(), &disallowed) {
        return Err(RejectReason::RobotsBlocked);
    }

    let existing_state = load_page_state(db, task.url.as_str());

    let mut request = client.get(url.as_str());
    if let Some(state) = existing_state.as_ref() {
        if let Some(etag) = state.etag.as_deref() {
            request = request.header(IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = state.last_modified.as_deref() {
            request = request.header(IF_MODIFIED_SINCE, last_modified);
        }
    }

    let response = match request.send().await {
        Ok(resp) => resp,
        Err(_) => return Err(RejectReason::Http5xx),
    };

    let status = response.status();
    let etag = header_to_string(response.headers(), ETAG);
    let last_modified = header_to_string(response.headers(), LAST_MODIFIED);
    if status.as_u16() == 304 {
        let final_url = response.url().to_string();
        let final_host = response.url().host_str().unwrap_or(&task.host).to_string();
        return Ok(FetchResult {
            task,
            final_url,
            final_host,
            html: String::new(),
            x_robots_noindex: false,
            etag: etag.or_else(|| existing_state.as_ref().and_then(|state| state.etag.clone())),
            last_modified: last_modified.or_else(|| {
                existing_state
                    .as_ref()
                    .and_then(|state| state.last_modified.clone())
            }),
            not_modified: true,
        });
    }
    if !status.is_success() {
        return Err(if status.as_u16() == 404 {
            RejectReason::Http4xx
        } else if status.as_u16() == 429 {
            RejectReason::Http5xx
        } else if status.is_server_error() {
            RejectReason::Http5xx
        } else {
            RejectReason::Http4xx
        });
    }

    let final_url_raw = response.url().clone();
    let Some(final_url) = canon::canonicalize(final_url_raw.as_str()) else {
        return Err(RejectReason::RedirectBadUrl);
    };
    let Some(final_host) = final_url.host_str().map(str::to_string) else {
        return Err(RejectReason::RedirectNoHost);
    };
    if !policy::host_allowed(&final_host) {
        return Err(RejectReason::RedirectDisallowed);
    }
    if !policy::url_allowed(&final_url) {
        return Err(RejectReason::RedirectFiltered);
    }

    let final_rules = if final_host == task.host {
        disallowed
    } else {
        robots::get_disallowed(&final_host, db, client, robots_cache)
            .await
            .unwrap_or_default()
    };
    if !robots::is_allowed(final_url.path(), &final_rules) {
        return Err(RejectReason::RedirectRobotsBlocked);
    }

    let x_robots_noindex = x_robots_has_noindex(response.headers());
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !content_type.contains("text/html") && !content_type.contains("application/xhtml+xml") {
        return Err(RejectReason::NotHtml);
    }

    let html = match response.text().await {
        Ok(body) => body,
        Err(_) => return Err(RejectReason::BodyReadErr),
    };

    Ok(FetchResult {
        task,
        final_url: final_url.to_string(),
        final_host,
        html,
        x_robots_noindex,
        etag,
        last_modified,
        not_modified: false,
    })
}

fn load_page_state(db: &rocksdb::DB, url: &str) -> Option<PageState> {
    let page_state_cf = storage::cf(db, storage::CF_PAGE_STATE).ok()?;
    let bytes = db.get_cf(page_state_cf, url.as_bytes()).ok()??;
    serde_json::from_slice(&bytes).ok()
}

fn header_to_string(
    headers: &reqwest::header::HeaderMap,
    name: reqwest::header::HeaderName,
) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string())
}

fn x_robots_has_noindex(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get_all("x-robots-tag")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase())
        .any(|value| value.contains("noindex") || value.contains("none"))
}
