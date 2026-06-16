/// Builds the canonical result JSON written to `axon_crawl_jobs.result_json`.
/// Required keys are locked by `crawl_result_json_required_keys`. The optional
/// `embed_deferred` key is only present when the embed enqueue was rejected
/// (typically due to the embed queue cap) — its presence signals that markdown
/// is on disk but not yet indexed.
pub(super) fn build_crawl_result_json(
    url: &str,
    worker_output_dir: &std::path::Path,
    caller_output_dir: &std::path::Path,
    summary: &crate::crawl::engine::CrawlSummary,
    embed_job_id: Option<&str>,
    embed_deferred: Option<&str>,
    sitemap_backfill_error: Option<&str>,
) -> serde_json::Value {
    let mut value = serde_json::json!({
        "url": url,
        "output_dir": caller_output_dir,
        "output_path": caller_output_dir.join("markdown"),
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
