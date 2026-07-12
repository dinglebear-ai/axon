//! WAF (web application firewall) block/recovery diagnostics.
//!
//! Transport-neutral data contract: describes whether a crawl hit a WAF,
//! whether recovery was attempted, and which URLs remain blocked. Moved
//! verbatim out of `axon-crawl` so it can outlive that crate's eventual
//! deletion and surface through adapter capability/degraded-mode reporting
//! (#298). The constructor (`build_waf_diagnostics`) stays in `axon-crawl`
//! since it depends on crawl-engine types (`CrawlSummary`).

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WafDiagnostics {
    pub status: String,
    pub attempted_recovery: bool,
    pub detected_pages: u32,
    pub recovered_pages: u32,
    pub remaining_pages: u32,
    pub detected_urls: Vec<String>,
    pub remaining_urls: Vec<String>,
}
