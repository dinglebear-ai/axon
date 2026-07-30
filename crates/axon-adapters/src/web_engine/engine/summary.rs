//! Crawl diagnostic and summary types.
//!
//! Pure data carried out of the collector and serialized into a crawl job's
//! `result_json` for the palette's live view. Split out of `engine.rs` to keep
//! that file under the repository's 500-line cap; the orchestration that
//! *populates* these types stays there.

use std::collections::HashSet;

use super::adaptive::AdaptiveCrawlSnapshot;

/// Upper bound on diagnostic samples retained for `axon crawl errors`.
pub const MAX_CRAWL_DIAGNOSTICS: usize = 100;

/// Upper bound on the live per-page event ring carried in `CrawlSummary` and
/// persisted into the crawl job's `result_json` for the palette's log tail.
pub const MAX_CRAWL_EVENTS: usize = 60;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CrawlDiagnostic {
    pub phase: String,
    pub class: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dropped: Option<u64>,
}

impl CrawlDiagnostic {
    pub fn new(
        phase: impl Into<String>,
        class: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            phase: phase.into(),
            class: class.into(),
            message: message.into(),
            url: None,
            http_status: None,
            dropped: None,
        }
    }

    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn with_http_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    pub fn with_dropped(mut self, dropped: u64) -> Self {
        self.dropped = Some(dropped);
        self
    }
}

/// A single per-page fetch event surfaced to the live crawl view. Serialized into
/// `result_json.events` by the progress persister. `t` is milliseconds since the
/// collector started; the frontend renders `<t>ms fetch <url> → <status> · <n> links`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PageEvent {
    pub t: u64,
    pub url: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<u32>,
}

/// A host that returned 429 during the crawl, with the configured retry backoff.
/// Drives the "N hosts rate-limited · backing off Ns" banner.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RateLimitHost {
    pub host: String,
    pub backoff_ms: u64,
}

#[derive(Debug, Default, Clone)]
pub struct CrawlSummary {
    pub pages_seen: u32,
    pub markdown_files: u32,
    pub thin_pages: u32,
    pub reused_pages: u32,
    pub pages_discovered: u32,
    pub elapsed_ms: u128,
    /// Canonical URLs of pages that were below `min_markdown_chars`.
    /// Populated by the collector and used by the auto-switch path to
    /// perform targeted per-URL Chrome re-fetches instead of a full re-crawl.
    pub thin_urls: HashSet<String>,
    /// Pages skipped due to non-2xx HTTP status codes.
    pub error_pages: u32,
    /// Pages blocked by a WAF or anti-bot system (`waf_check || blocked_crawl`).
    pub waf_blocked_pages: u32,
    /// Canonical URLs of WAF-blocked pages; used for targeted stealth Chrome retry.
    pub waf_blocked_urls: HashSet<String>,
    /// Bounded diagnostic samples for operator-facing `axon crawl errors`.
    pub diagnostics: Vec<CrawlDiagnostic>,
    /// Bounded ring of recent per-page fetch events for the live log tail.
    pub recent_events: Vec<PageEvent>,
    /// Hosts seen returning 429, with the configured backoff (for the banner).
    pub rate_limited: Vec<RateLimitHost>,
    /// Max crawl depth from config — the denominator of the DEPTH stat.
    pub depth_max: u32,
    pub adaptive: Option<AdaptiveCrawlSnapshot>,
}

impl CrawlSummary {
    pub fn push_diagnostic(&mut self, diagnostic: CrawlDiagnostic) {
        if self.diagnostics.len() < MAX_CRAWL_DIAGNOSTICS {
            self.diagnostics.push(diagnostic);
        }
    }

    /// Append a per-page event, evicting the oldest beyond `MAX_CRAWL_EVENTS`.
    pub fn push_event(&mut self, event: PageEvent) {
        if self.recent_events.len() >= MAX_CRAWL_EVENTS {
            self.recent_events.remove(0);
        }
        self.recent_events.push(event);
    }

    /// Record (or refresh the backoff of) a rate-limited host.
    pub fn note_rate_limited(&mut self, host: &str, backoff_ms: u64) {
        if host.is_empty() {
            return;
        }
        if let Some(existing) = self.rate_limited.iter_mut().find(|h| h.host == host) {
            existing.backoff_ms = backoff_ms;
        } else {
            self.rate_limited.push(RateLimitHost {
                host: host.to_string(),
                backoff_ms,
            });
        }
    }

    /// Pages discovered but not yet fetched (the live QUEUED backlog).
    pub fn queued(&self) -> u32 {
        self.pages_discovered.saturating_sub(self.pages_seen)
    }
}
