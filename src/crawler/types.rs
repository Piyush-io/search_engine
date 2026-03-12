use serde::{Deserialize, Serialize};
use crate::PageRecord;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RejectReason {
    BadUrl,
    ScopeFiltered,
    RobotsBlocked,
    RedirectFiltered,
    RedirectRobotsBlocked,
    DnsFailed,
    Timeout,
    Http4xx,
    Http5xx,
    NotHtml,
    NoIndex,
    LowText,
    Duplicate,
    AlreadyStored,
    ParsePanic,
    BodyReadErr,
    RedirectBadUrl,
    RedirectNoHost,
    RedirectDisallowed,
}

impl RejectReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BadUrl => "bad_url",
            Self::ScopeFiltered => "scope_filtered",
            Self::RobotsBlocked => "robots_blocked",
            Self::RedirectFiltered => "redirect_filtered",
            Self::RedirectRobotsBlocked => "redirect_robots_blocked",
            Self::DnsFailed => "dns_fail",
            Self::Timeout => "timeout",
            Self::Http4xx => "http_4xx",
            Self::Http5xx => "http_5xx",
            Self::NotHtml => "not_html",
            Self::NoIndex => "noindex",
            Self::LowText => "low_text",
            Self::Duplicate => "duplicate",
            Self::AlreadyStored => "already_stored",
            Self::ParsePanic => "parse_panic",
            Self::BodyReadErr => "body_read_err",
            Self::RedirectBadUrl => "redirect_bad_url",
            Self::RedirectNoHost => "redirect_no_host",
            Self::RedirectDisallowed => "redirect_disallowed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UrlTask {
    pub url: String,
    pub host: String,
    pub depth: u16,
    pub priority: i32,
}

impl Ord for UrlTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher priority first
        self.priority.cmp(&other.priority)
            .then_with(|| other.depth.cmp(&self.depth)) // Shallower depth first if priority equal
    }
}

impl PartialOrd for UrlTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug)]
pub struct FetchResult {
    pub task: UrlTask,
    pub final_url: String,
    pub final_host: String,
    pub html: String,
    pub x_robots_noindex: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageQuality {
    pub text_bytes: usize,
    pub block_count: usize,
    pub link_density: f32,
    pub should_store: bool,
    pub reject_reason: Option<RejectReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedPage {
    pub final_url: String,
    pub canonical_url: String,
    pub title: String,
    pub description: Option<String>,
    pub page_record: PageRecord,
    pub outlinks: Vec<String>,
    pub noindex: bool,
    pub quality: PageQuality,
}
