use super::{CollectorConfig, PageOutcome, write_page_to_manifest};
use crate::engine::thin_refetch::{RefetchResult, render_html_with_chrome};
use crate::engine::{CrawlDiagnostic, CrawlSummary};
use crate::manifest::ManifestEntry;
use axon_core::content::url_to_stable_filename;
use axon_core::logging::{log_info, log_warn};
use spider_transformations::transformation::content::SelectorConfiguration;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinSet;

/// Spawn an inline Chrome render task for a thin page after a permit is acquired.
///
/// Uses the HTML bytes already in hand — no second HTTP request.
#[expect(
    clippy::too_many_arguments,
    reason = "task spawn passes ownership of render inputs into the async task"
)]
pub(super) fn spawn_chrome_render(
    chrome_tasks: &mut JoinSet<RefetchResult>,
    permit: OwnedSemaphorePermit,
    ws_url: String,
    html_bytes: Vec<u8>,
    url: String,
    min_chars: usize,
    timeout_secs: u64,
    selector_config: Option<SelectorConfiguration>,
) {
    chrome_tasks.spawn(async move {
        let _permit = permit;
        let markdown = render_html_with_chrome(
            &ws_url,
            html_bytes,
            &url,
            min_chars,
            timeout_secs,
            selector_config,
        )
        .await;
        let diagnostic = markdown.is_none().then(|| {
            CrawlDiagnostic::new(
                "chrome_render",
                "chrome_render_failed_or_thin",
                "inline Chrome render failed or still produced thin markdown",
            )
            .with_url(url.clone())
        });
        RefetchResult {
            url,
            markdown,
            diagnostic,
        }
    });
}

/// Drain all in-flight Chrome render tasks and collect their results.
pub(super) async fn drain_chrome_tasks(
    chrome_tasks: &mut JoinSet<RefetchResult>,
    chrome_results: &mut Vec<RefetchResult>,
) {
    if chrome_tasks.is_empty() {
        return;
    }
    let pending = chrome_tasks.len();
    log_info(&format!(
        "thin_refetch: waiting for {pending} in-flight Chrome render(s) to complete"
    ));
    while let Some(task_result) = chrome_tasks.join_next().await {
        match task_result {
            Ok(r) => chrome_results.push(r),
            Err(e) => log_warn(&format!("thin_refetch: Chrome task panicked: {e}")),
        }
    }
}

/// Write a thin page to disk using the already-transformed markdown and hash
/// from `process_page`. This avoids re-running the transform pipeline.
pub(super) async fn write_thin_page_if_needed(
    url: &str,
    col: &CollectorConfig,
    summary: &mut CrawlSummary,
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
    trimmed: String,
    content_hash: String,
) -> Result<(), String> {
    if trimmed.is_empty() {
        return Ok(());
    }
    let filename = url_to_stable_filename(url);
    let entry = ManifestEntry {
        url: url.to_string(),
        relative_path: format!("markdown/{filename}"),
        markdown_chars: trimmed.len(),
        content_hash: Some(content_hash),
        changed: true,
        // Chrome re-render of thin pages: raw HTML is not threaded here, so
        // structured data from this path is absent (None). A follow-up could
        // thread HTML bytes through the RefetchResult to add structured data.
        structured: None,
    };
    let thin_write = PageOutcome::Write {
        filename,
        trimmed,
        entry,
    };
    let wrote = write_page_to_manifest(
        manifest,
        &thin_write,
        &col.markdown_dir,
        &col.previous_manifest,
        url,
    )
    .await?;
    if wrote {
        summary.markdown_files += 1;
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "all params are pass-through from apply_page_outcome"
)]
pub(super) async fn apply_thin_page_outcome(
    html_bytes: Vec<u8>,
    url: &str,
    col: &CollectorConfig,
    summary: &mut CrawlSummary,
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
    chrome_tasks: &mut JoinSet<RefetchResult>,
    chrome_semaphore: Arc<Semaphore>,
    trimmed: String,
    content_hash: String,
) -> Result<bool, String> {
    summary.thin_pages += 1;
    summary.thin_urls.insert(url.to_string());
    if let Some(ref ws_url) = col.chrome_ws_url {
        let permit = match chrome_semaphore.acquire_owned().await {
            Ok(permit) => permit,
            Err(_) => {
                summary.push_diagnostic(
                    CrawlDiagnostic::new(
                        "chrome_render",
                        "chrome_semaphore_closed",
                        "Chrome render semaphore closed before task acquired a permit",
                    )
                    .with_url(url.to_string()),
                );
                return Ok(true);
            }
        };
        log_info(&format!(
            "thin_refetch: inline Chrome render spawned for {url}"
        ));
        spawn_chrome_render(
            chrome_tasks,
            permit,
            ws_url.clone(),
            html_bytes,
            url.to_string(),
            col.min_chars,
            col.chrome_timeout_secs,
            col.selector_config.clone(),
        );
    }
    if col.drop_thin {
        return Ok(true);
    }
    write_thin_page_if_needed(url, col, summary, manifest, trimmed, content_hash).await?;
    Ok(false)
}
