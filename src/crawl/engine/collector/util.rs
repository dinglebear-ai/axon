use super::CollectorConfig;
use crate::crawl::engine::{CrawlDiagnostic, CrawlSummary};

pub(super) fn track_waf_block(
    waf_check: bool,
    blocked_crawl: bool,
    url: &str,
    _anti_bot_tech: &impl std::fmt::Debug,
    summary: &mut CrawlSummary,
) {
    if !(waf_check || blocked_crawl) {
        return;
    }
    summary.waf_blocked_pages += 1;
    summary.waf_blocked_urls.insert(url.to_string());
    summary.push_diagnostic(
        CrawlDiagnostic::new(
            "http_fetch",
            "waf_blocked",
            "page reported WAF or anti-bot block",
        )
        .with_url(url.to_string()),
    );
}

/// Emit progress at most once per `PROGRESS_INTERVAL`. Cloning `CrawlSummary`
/// (which includes growing `HashSet<String>` fields) on every page creates
/// significant allocation pressure on large crawls. Time-gating avoids this.
pub(super) const PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

pub(super) async fn emit_progress(
    col: &CollectorConfig,
    summary: &CrawlSummary,
    last_progress: &mut std::time::Instant,
) {
    if let Some(tx) = col.progress_tx.as_ref() {
        let now = std::time::Instant::now();
        if now.duration_since(*last_progress) >= PROGRESS_INTERVAL {
            tx.send(summary.clone()).await.ok();
            *last_progress = now;
        }
    }
}
