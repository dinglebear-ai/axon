use std::collections::HashSet;

use super::CrawlSummary;

// `WafDiagnostics` is a transport-neutral DTO and now lives in `axon-api` so
// it can outlive this crate's eventual deletion (#298). Re-exported here so
// all in-crate `axon_crawl::engine::WafDiagnostics` call sites keep working
// unchanged.
pub use axon_api::source::waf_diagnostics::WafDiagnostics;

fn sorted_urls(values: &HashSet<String>) -> Vec<String> {
    let mut out: Vec<String> = values.iter().cloned().collect();
    out.sort();
    out
}

pub fn build_waf_diagnostics(
    initial_summary: &CrawlSummary,
    final_summary: &CrawlSummary,
    attempted_recovery: bool,
    remaining_urls: Option<&HashSet<String>>,
) -> Option<WafDiagnostics> {
    let detected_urls = if initial_summary.waf_blocked_urls.is_empty() {
        sorted_urls(&final_summary.waf_blocked_urls)
    } else {
        sorted_urls(&initial_summary.waf_blocked_urls)
    };
    let detected_pages = initial_summary
        .waf_blocked_pages
        .max(final_summary.waf_blocked_pages)
        .max(detected_urls.len() as u32);
    if detected_pages == 0 && detected_urls.is_empty() {
        return None;
    }

    let remaining_url_set = remaining_urls.cloned().unwrap_or_else(|| {
        if attempted_recovery {
            final_summary.waf_blocked_urls.clone()
        } else if !initial_summary.waf_blocked_urls.is_empty() {
            initial_summary.waf_blocked_urls.clone()
        } else {
            final_summary.waf_blocked_urls.clone()
        }
    });
    let remaining_urls = sorted_urls(&remaining_url_set);
    let remaining_pages = if !remaining_urls.is_empty() {
        remaining_urls.len() as u32
    } else if attempted_recovery {
        0
    } else {
        detected_pages
    };
    let recovered_pages = detected_pages.saturating_sub(remaining_pages);
    let status = if !attempted_recovery {
        "detected_unrecovered"
    } else if remaining_pages == 0 {
        "recovered_full"
    } else if recovered_pages > 0 {
        "recovered_partial"
    } else {
        "detected_unrecovered"
    };

    Some(WafDiagnostics {
        status: status.to_string(),
        attempted_recovery,
        detected_pages,
        recovered_pages,
        remaining_pages,
        detected_urls,
        remaining_urls,
    })
}
