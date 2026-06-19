/// Builds the canonical result JSON written to `axon_crawl_jobs.result_json`.
/// Required keys are locked by `crawl_result_json_required_keys`. The optional
/// `embed_deferred` key is only present when the embed enqueue was rejected
/// (typically due to the embed queue cap) — its presence signals that markdown
/// is on disk but not yet indexed.
#[expect(
    clippy::too_many_arguments,
    reason = "keeps the persisted crawl result contract explicit at the call site"
)]
pub(super) fn build_crawl_result_json(
    url: &str,
    worker_output_dir: &std::path::Path,
    caller_output_dir: &std::path::Path,
    max_pages: u32,
    summary: &crate::crawl::engine::CrawlSummary,
    embed_job_id: Option<&str>,
    embed_deferred: Option<&str>,
    sitemap_backfill_error: Option<&str>,
) -> serde_json::Value {
    let coverage = coverage_status(max_pages, summary);
    let mut value = serde_json::json!({
        "url": url,
        "output_dir": caller_output_dir,
        "output_path": caller_output_dir.join("markdown"),
        "coverage_status": coverage.status,
        "coverage_reason": coverage.reason,
        "coverage_summary": coverage.summary(),
        "coverage_limit_pages": max_pages,
        "pages_crawled": summary.pages_seen,
        "md_created": summary.markdown_files,
        "pages_discovered": summary.pages_discovered,
        "queued": summary.queued(),
        "depth_max": summary.depth_max,
        "thin_md": summary.thin_pages,
        "error_pages": summary.error_pages,
        "waf_blocked_pages": summary.waf_blocked_pages,
        "diagnostic_count": summary.diagnostics.len(),
        "diagnostic_counts": diagnostic_counts_json(summary),
        "diagnostics": &summary.diagnostics,
        "events": &summary.recent_events,
        "rate_limited": &summary.rate_limited,
        "elapsed_ms": summary.elapsed_ms,
        "embed_job_id": embed_job_id,
    });
    if worker_output_dir != caller_output_dir
        && let Some(obj) = value.as_object_mut()
    {
        obj.insert(
            "worker_output_dir".to_string(),
            serde_json::Value::String(worker_output_dir.to_string_lossy().into_owned()),
        );
        obj.insert(
            "worker_output_path".to_string(),
            serde_json::Value::String(
                worker_output_dir
                    .join("markdown")
                    .to_string_lossy()
                    .into_owned(),
            ),
        );
    }
    if let (Some(reason), Some(obj)) = (embed_deferred, value.as_object_mut()) {
        obj.insert(
            "embed_deferred".to_string(),
            serde_json::Value::String(reason.to_string()),
        );
    }
    if let (Some(adaptive), Some(obj)) = (summary.adaptive.as_ref(), value.as_object_mut()) {
        obj.insert(
            "adaptive_concurrency".to_string(),
            serde_json::to_value(adaptive).unwrap_or(serde_json::Value::Null),
        );
    }
    if let (Some(error), Some(obj)) = (sitemap_backfill_error, value.as_object_mut()) {
        obj.insert(
            "sitemap_backfill_error".to_string(),
            serde_json::Value::String(error.to_string()),
        );
    }
    value
}

struct CoverageStatus {
    status: &'static str,
    reason: Option<&'static str>,
}

impl CoverageStatus {
    fn summary(&self) -> &'static str {
        match (self.status, self.reason) {
            ("partial", Some("max_pages_limit")) => "max pages hit",
            ("partial", _) => "partial",
            ("complete_or_exhausted", _) => "complete",
            _ => self.status,
        }
    }
}

fn coverage_status(max_pages: u32, summary: &crate::crawl::engine::CrawlSummary) -> CoverageStatus {
    if max_pages > 0 && summary.pages_seen >= max_pages {
        CoverageStatus {
            status: "partial",
            reason: Some("max_pages_limit"),
        }
    } else {
        CoverageStatus {
            status: "complete_or_exhausted",
            reason: None,
        }
    }
}

fn diagnostic_counts_json(summary: &crate::crawl::engine::CrawlSummary) -> serde_json::Value {
    let mut counts = serde_json::Map::new();
    for diagnostic in &summary.diagnostics {
        let key = format!("{}:{}", diagnostic.phase, diagnostic.class);
        let next = counts
            .get(&key)
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            + 1;
        counts.insert(key, serde_json::Value::from(next));
    }
    serde_json::Value::Object(counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crawl::engine::CrawlSummary;

    #[test]
    fn coverage_is_partial_when_page_cap_is_hit() {
        let summary = CrawlSummary {
            pages_seen: 100,
            pages_discovered: 150,
            ..CrawlSummary::default()
        };

        let coverage = coverage_status(100, &summary);

        assert_eq!(coverage.status, "partial");
        assert_eq!(coverage.reason, Some("max_pages_limit"));
        assert_eq!(coverage.summary(), "max pages hit");
    }

    #[test]
    fn coverage_is_not_partial_for_uncapped_crawl() {
        let summary = CrawlSummary {
            pages_seen: 100,
            pages_discovered: 150,
            ..CrawlSummary::default()
        };

        let coverage = coverage_status(0, &summary);

        assert_eq!(coverage.status, "complete_or_exhausted");
        assert_eq!(coverage.reason, None);
        assert_eq!(coverage.summary(), "complete");
    }

    #[test]
    fn coverage_is_not_partial_when_site_exhausts_before_cap() {
        let summary = CrawlSummary {
            pages_seen: 12,
            pages_discovered: 12,
            ..CrawlSummary::default()
        };

        let coverage = coverage_status(100, &summary);

        assert_eq!(coverage.status, "complete_or_exhausted");
        assert_eq!(coverage.reason, None);
    }
}
