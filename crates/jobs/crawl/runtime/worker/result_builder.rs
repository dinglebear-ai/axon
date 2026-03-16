use crate::crates::crawl::engine::CrawlSummary;
use crate::crates::crawl::manifest::read_manifest_data;
use std::error::Error;
use std::path::Path;

use super::super::robots::{RobotsBackfillStats, RobotsDiscoveryStats};
use super::super::{read_manifest_urls, write_audit_diff};
use super::job_context::JobExecutionContext;

fn sorted_vec(values: &std::collections::HashSet<String>) -> Vec<String> {
    let mut out: Vec<String> = values.iter().cloned().collect();
    out.sort();
    out
}

fn build_output_files(
    output_dir: &Path,
    manifest_path: &Path,
    audit_report_path: &Path,
    manifest_relative_paths: impl IntoIterator<Item = String>,
) -> Vec<String> {
    let mut files = vec![
        manifest_path.to_string_lossy().into_owned(),
        audit_report_path.to_string_lossy().into_owned(),
    ];
    files.extend(manifest_relative_paths.into_iter().map(|relative_path| {
        output_dir
            .join(relative_path)
            .to_string_lossy()
            .into_owned()
    }));
    files.sort();
    files.dedup();
    files
}

pub(super) struct CompletedResultContext {
    pub(super) summary: CrawlSummary,
    pub(super) final_summary: CrawlSummary,
    pub(super) robots_backfill_stats: RobotsBackfillStats,
    pub(super) robots_discovery_stats: RobotsDiscoveryStats,
    pub(super) final_prompt: Option<String>,
}

pub(super) async fn build_completed_result(
    ctx: &JobExecutionContext,
    manifest_path: &Path,
    result_ctx: CompletedResultContext,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let crawl_discovered = result_ctx.summary.pages_seen as u64;
    let robots_extra = result_ctx.robots_backfill_stats.candidates as u64;
    let pages_discovered = crawl_discovered.saturating_add(robots_extra);
    let filtered_urls =
        pages_discovered.saturating_sub(result_ctx.final_summary.markdown_files as u64);
    let pages_crawled = result_ctx.summary.pages_seen as u64;
    let current_urls = read_manifest_urls(manifest_path).await?;
    let manifest_entries = read_manifest_data(manifest_path).await?;
    let (report_path, diff_report) = write_audit_diff(
        &ctx.job_cfg.output_dir,
        &ctx.url,
        &ctx.previous_urls,
        &current_urls,
        false,
        ctx.cache_source.clone(),
    )
    .await?;
    let output_files = build_output_files(
        &ctx.job_cfg.output_dir,
        manifest_path,
        &report_path,
        manifest_entries
            .into_values()
            .map(|entry| entry.relative_path),
    );

    Ok(serde_json::json!({
        "phase": "completed",
        "cache_hit": false,
        "cache_skip_browser": ctx.job_cfg.cache_skip_browser,
        "md_created": result_ctx.final_summary.markdown_files,
        "thin_md": result_ctx.final_summary.thin_pages,
        "filtered_urls": filtered_urls,
        "error_pages": result_ctx.final_summary.error_pages,
        "waf_blocked_pages": result_ctx.final_summary.waf_blocked_pages,
        "thin_urls": sorted_vec(&result_ctx.final_summary.thin_urls),
        "waf_blocked_urls": sorted_vec(&result_ctx.final_summary.waf_blocked_urls),
        "pages_crawled": pages_crawled,
        "pages_discovered": pages_discovered,
        "crawl_stream_pages": result_ctx.summary.pages_seen,
        "robots_sitemap_docs_parsed": result_ctx.robots_discovery_stats.parsed_sitemap_documents,
        "robots_declared_sitemaps": result_ctx.robots_discovery_stats.robots_declared_sitemaps,
        "robots_discovered_urls": result_ctx.robots_backfill_stats.discovered_urls,
        "robots_candidates": result_ctx.robots_backfill_stats.candidates,
        "robots_written": result_ctx.robots_backfill_stats.written,
        "robots_failed": result_ctx.robots_backfill_stats.failed,
        "robots_filtered_existing": result_ctx.robots_backfill_stats.filtered_existing,
        "elapsed_ms": result_ctx.final_summary.elapsed_ms,
        "output_dir": ctx.job_cfg.output_dir.to_string_lossy(),
        "output_files": output_files,
        "audit_diff": diff_report,
        "audit_report_path": report_path.to_string_lossy(),
        "extraction_prompt": result_ctx.final_prompt,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::Config;
    use crate::crates::crawl::manifest::ManifestEntry;
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    #[tokio::test]
    async fn crawl_result_json_includes_artifact_url_lists_and_error_metrics() {
        let temp = tempfile::tempdir().expect("tempdir");
        let manifest_path = temp.path().join("manifest.jsonl");
        tokio::fs::write(
            &manifest_path,
            "{\"url\":\"https://example.com/a\",\"relative_path\":\"markdown/a.md\",\"markdown_chars\":123,\"changed\":true}\n",
        )
        .await
        .expect("write manifest");

        let cfg = Config {
            output_dir: temp.path().to_path_buf(),
            ..Config::default()
        };

        let ctx = JobExecutionContext {
            url: "https://example.com".to_string(),
            job_cfg: cfg,
            extraction_prompt: None,
            previous_urls: HashSet::new(),
            previous_manifest: Arc::new(HashMap::<String, ManifestEntry>::new()),
            cache_source: None,
        };

        let mut thin_urls = HashSet::new();
        thin_urls.insert("https://example.com/thin".to_string());
        let mut waf_urls = HashSet::new();
        waf_urls.insert("https://example.com/blocked".to_string());

        let summary = CrawlSummary {
            pages_seen: 1,
            markdown_files: 1,
            thin_pages: 0,
            reused_pages: 0,
            pages_discovered: 1,
            elapsed_ms: 100,
            thin_urls: HashSet::new(),
            error_pages: 0,
            waf_blocked_pages: 0,
            waf_blocked_urls: HashSet::new(),
        };
        let final_summary = CrawlSummary {
            pages_seen: 2,
            markdown_files: 1,
            thin_pages: 1,
            reused_pages: 0,
            pages_discovered: 2,
            elapsed_ms: 200,
            thin_urls,
            error_pages: 3,
            waf_blocked_pages: 4,
            waf_blocked_urls: waf_urls,
        };

        let json = build_completed_result(
            &ctx,
            &manifest_path,
            CompletedResultContext {
                summary,
                final_summary,
                robots_backfill_stats: RobotsBackfillStats::default(),
                robots_discovery_stats: RobotsDiscoveryStats::default(),
                final_prompt: None,
            },
        )
        .await
        .expect("result json");

        let obj = json.as_object().expect("json object");
        assert!(obj.contains_key("thin_urls"));
        assert!(obj.contains_key("waf_blocked_urls"));
        assert!(obj.contains_key("error_pages"));
        assert!(obj.contains_key("waf_blocked_pages"));
        let output_files = obj["output_files"].as_array().expect("output_files array");
        assert_eq!(output_files.len(), 3);
        assert_eq!(
            output_files[0],
            temp.path()
                .join("audit")
                .join("example-com-diff-report.json")
                .to_string_lossy()
                .to_string()
        );
        assert_eq!(output_files[1], manifest_path.to_string_lossy().to_string());
        assert_eq!(
            output_files[2],
            temp.path()
                .join("markdown/a.md")
                .to_string_lossy()
                .to_string()
        );
    }
}
