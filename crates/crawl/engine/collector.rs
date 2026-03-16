use super::thin_refetch::{
    RefetchResult, THIN_REFETCH_CONCURRENCY, render_html_with_chrome, write_refetch_results,
};
use super::{
    CrawlSummary, MapScope, canonicalize_url_for_dedupe, is_excluded_url_path,
    normalize_map_candidate_url,
};
use crate::crates::core::content::{
    BOILERPLATE_SELECTORS, clean_markdown_whitespace, url_to_filename,
};
use crate::crates::core::logging::{log_debug, log_info, log_warn};
use crate::crates::crawl::manifest::ManifestEntry;
use sha2::{Digest, Sha256};
use spider_transformations::transformation::content::{
    SelectorConfiguration, TransformInput, transform_content_input,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tokio::sync::mpsc::Sender;
use tokio::task::JoinSet;

/// Configuration for the crawl page collector.
pub(super) struct CollectorConfig {
    pub markdown_dir: std::path::PathBuf,
    pub manifest_path: std::path::PathBuf,
    pub min_chars: usize,
    pub drop_thin: bool,
    pub exclude_path_prefix: Vec<String>,
    pub scope: Option<MapScope>,
    pub transform_cfg: &'static spider_transformations::transformation::content::TransformConfig,
    pub progress_tx: Option<Sender<CrawlSummary>>,
    pub previous_manifest: Arc<HashMap<String, ManifestEntry>>,
    /// Optional CSS selectors for content scoping (root_selector / exclude_selector).
    pub selector_config: Option<SelectorConfiguration>,
    /// Pre-resolved Chrome WebSocket URL for inline thin-page re-rendering.
    /// When `Some`, thin pages are immediately re-rendered with Chrome while
    /// the HTTP crawl loop continues receiving more pages — no second pass.
    /// When `None`, thin pages are deferred to the post-crawl batch fallback.
    pub chrome_ws_url: Option<String>,
    /// Seconds to wait for Chrome to finish rendering a page.
    pub chrome_timeout_secs: u64,
    /// Output directory root (parent of `markdown/`), needed to write
    /// Chrome-recovered pages via `write_refetch_results`.
    pub output_dir: std::path::PathBuf,
}

/// Outcome of `process_page` — what the collector loop should do next.
pub(super) enum PageOutcome {
    /// Page is thin; skip writing it (when `drop_thin` is true).
    Thin,
    /// Page body is empty after transformation; skip it.
    Empty,
    /// Page is unchanged from a previous crawl; reuse the cached file.
    Reused {
        filename: String,
        entry: ManifestEntry,
    },
    /// Page is new or changed; write content to disk.
    Write {
        filename: String,
        trimmed: String,
        entry: ManifestEntry,
    },
}

/// Pure page processing: transform HTML → check thin → hash → manifest dedup.
///
/// Does no I/O. Returns a `PageOutcome` telling the caller what action to take.
pub(super) fn process_page(
    html_bytes: &[u8],
    url: &str,
    col: &CollectorConfig,
    next_file_count: u32,
) -> PageOutcome {
    let input = TransformInput {
        url: None,
        content: html_bytes,
        screenshot_bytes: None,
        encoding: None,
        selector_config: col.selector_config.as_ref(),
        ignore_tags: Some(BOILERPLATE_SELECTORS),
    };
    let markdown = transform_content_input(input, col.transform_cfg);
    let trimmed = clean_markdown_whitespace(markdown.trim());
    let chars = trimmed.len();

    if chars < col.min_chars {
        log_debug(&format!(
            "content thin_page url={url} chars={chars} min={}",
            col.min_chars
        ));
        return PageOutcome::Thin;
    }
    if trimmed.is_empty() {
        return PageOutcome::Empty;
    }

    let mut hasher = Sha256::new();
    hasher.update(trimmed.as_bytes());
    let content_hash = hex::encode(hasher.finalize());

    if let Some(prev) = col.previous_manifest.get(url)
        && prev.content_hash.as_deref() == Some(&content_hash)
    {
        let prev_path = std::path::Path::new(&prev.relative_path);
        if prev_path.exists() {
            let filename = url_to_filename(url, next_file_count);
            let entry = ManifestEntry {
                url: url.to_string(),
                relative_path: format!("markdown/{filename}"),
                markdown_chars: chars,
                content_hash: Some(content_hash),
                changed: false,
            };
            return PageOutcome::Reused { filename, entry };
        }
    }

    let filename = url_to_filename(url, next_file_count);
    let entry = ManifestEntry {
        url: url.to_string(),
        relative_path: format!("markdown/{filename}"),
        markdown_chars: chars,
        content_hash: Some(content_hash),
        changed: true,
    };
    PageOutcome::Write {
        filename,
        trimmed,
        entry,
    }
}

/// Write a page to disk (or relink from cache) and append its manifest entry.
///
/// Returns `true` on success, `false` on any I/O failure (the caller should
/// not increment counters on failure).
pub(super) async fn write_page_to_manifest(
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
    outcome: &PageOutcome,
    markdown_dir: &std::path::Path,
    prev_manifest: &HashMap<String, ManifestEntry>,
    url: &str,
) -> Result<bool, String> {
    match outcome {
        PageOutcome::Reused { filename, entry } => {
            let prev_path = prev_manifest
                .get(url)
                .map(|m| std::path::PathBuf::from(&m.relative_path));
            let path = markdown_dir.join(filename);
            let link_res = if let Some(ref prev) = prev_path {
                if reflink_copy::reflink_or_copy(prev, &path).is_ok() {
                    Ok(())
                } else {
                    tokio::fs::hard_link(prev, &path).await
                }
            } else {
                Err(std::io::Error::other("no previous path"))
            };
            if link_res.is_err() {
                return Ok(false);
            }
            append_manifest_entry(manifest, entry).await?;
            Ok(true)
        }
        PageOutcome::Write {
            filename,
            trimmed,
            entry,
        } => {
            let path = markdown_dir.join(filename);
            tokio::fs::write(&path, trimmed.as_bytes())
                .await
                .map_err(|e| format!("write failed: {e}"))?;
            append_manifest_entry(manifest, entry).await?;
            Ok(true)
        }
        // Thin / Empty are not written; caller should not call this.
        _ => Ok(false),
    }
}

async fn append_manifest_entry(
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
    entry: &ManifestEntry,
) -> Result<(), String> {
    let mut line =
        serde_json::to_string(entry).map_err(|e| format!("json serialize failed: {e}"))?;
    line.push('\n');
    manifest
        .write_all(line.as_bytes())
        .await
        .map_err(|e| format!("manifest failed: {e}"))
}

/// Spawn an inline Chrome render task for a thin page, bounded by `sem`.
///
/// Uses the HTML bytes already in hand — no second HTTP request.
fn spawn_chrome_render(
    chrome_tasks: &mut JoinSet<RefetchResult>,
    sem: Arc<Semaphore>,
    ws_url: String,
    html_bytes: Vec<u8>,
    url: String,
    min_chars: usize,
    timeout_secs: u64,
) {
    chrome_tasks.spawn(async move {
        let _permit = match sem.acquire().await {
            Ok(p) => p,
            Err(_) => {
                return RefetchResult {
                    url,
                    markdown: None,
                };
            }
        };
        let markdown =
            render_html_with_chrome(&ws_url, html_bytes, &url, min_chars, timeout_secs).await;
        RefetchResult { url, markdown }
    });
}

/// Drain all in-flight Chrome render tasks and collect their results.
async fn drain_chrome_tasks(
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

/// Apply the outcome of `process_page()`: update summary counters, spawn Chrome
/// renders for thin pages, write good pages to the manifest. Returns `true` when
/// the caller should `continue` (skip further per-page work).
#[allow(clippy::too_many_arguments)]
async fn apply_page_outcome(
    outcome: PageOutcome,
    html_bytes: &[u8],
    url: &str,
    col: &CollectorConfig,
    summary: &mut CrawlSummary,
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
    chrome_tasks: &mut JoinSet<RefetchResult>,
    chrome_semaphore: Arc<Semaphore>,
) -> Result<bool, String> {
    match outcome {
        PageOutcome::Thin => {
            return apply_thin_page_outcome(
                html_bytes,
                url,
                col,
                summary,
                manifest,
                chrome_tasks,
                chrome_semaphore,
            )
            .await;
        }
        PageOutcome::Empty => return Ok(true),
        ref w @ (PageOutcome::Reused { .. } | PageOutcome::Write { .. }) => {
            apply_written_page_outcome(w, url, col, summary, manifest).await?;
        }
    }
    Ok(false)
}

async fn apply_thin_page_outcome(
    html_bytes: &[u8],
    url: &str,
    col: &CollectorConfig,
    summary: &mut CrawlSummary,
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
    chrome_tasks: &mut JoinSet<RefetchResult>,
    chrome_semaphore: Arc<Semaphore>,
) -> Result<bool, String> {
    summary.thin_pages += 1;
    summary.thin_urls.insert(url.to_string());
    if let Some(ref ws_url) = col.chrome_ws_url {
        log_info(&format!(
            "thin_refetch: inline Chrome render spawned for {url}"
        ));
        spawn_chrome_render(
            chrome_tasks,
            chrome_semaphore,
            ws_url.clone(),
            html_bytes.to_vec(),
            url.to_string(),
            col.min_chars,
            col.chrome_timeout_secs,
        );
    }
    if col.drop_thin {
        return Ok(true);
    }
    write_thin_page_if_needed(html_bytes, url, col, summary, manifest).await?;
    Ok(false)
}

async fn write_thin_page_if_needed(
    html_bytes: &[u8],
    url: &str,
    col: &CollectorConfig,
    summary: &mut CrawlSummary,
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
) -> Result<(), String> {
    let input = TransformInput {
        url: None,
        content: html_bytes,
        screenshot_bytes: None,
        encoding: None,
        selector_config: col.selector_config.as_ref(),
        ignore_tags: Some(BOILERPLATE_SELECTORS),
    };
    let markdown = transform_content_input(input, col.transform_cfg);
    let trimmed = clean_markdown_whitespace(markdown.trim());
    if trimmed.is_empty() {
        return Ok(());
    }
    let mut hasher = Sha256::new();
    hasher.update(trimmed.as_bytes());
    let content_hash = hex::encode(hasher.finalize());
    let filename = url_to_filename(url, summary.markdown_files + 1);
    let entry = ManifestEntry {
        url: url.to_string(),
        relative_path: format!("markdown/{filename}"),
        markdown_chars: trimmed.len(),
        content_hash: Some(content_hash),
        changed: true,
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

async fn apply_written_page_outcome(
    outcome: &PageOutcome,
    url: &str,
    col: &CollectorConfig,
    summary: &mut CrawlSummary,
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
) -> Result<(), String> {
    let wrote = write_page_to_manifest(
        manifest,
        outcome,
        &col.markdown_dir,
        &col.previous_manifest,
        url,
    )
    .await?;
    if wrote {
        summary.markdown_files += 1;
        if matches!(outcome, PageOutcome::Reused { .. }) {
            summary.reused_pages += 1;
        }
    }
    Ok(())
}

/// Drives the spider broadcast subscription to collect, filter, render, and
/// persist crawled pages. Runs in a spawned task while `website.crawl*()`
/// executes concurrently. Returns when the broadcast channel closes
/// (i.e. the crawl or sitemap phase has finished and `unsubscribe()` was called).
///
/// When `col.chrome_ws_url` is `Some`, thin pages are immediately spawned as
/// Chrome render tasks (bounded to `THIN_REFETCH_CONCURRENCY` concurrent tasks)
/// using the HTML bytes already in hand — no second network request. All
/// in-flight Chrome tasks are awaited after the crawl loop ends.
pub(super) async fn collect_crawl_pages(
    mut rx: tokio::sync::broadcast::Receiver<spider::page::Page>,
    col: CollectorConfig,
) -> Result<(CrawlSummary, HashSet<String>), String> {
    let manifest_file = tokio::fs::File::create(&col.manifest_path)
        .await
        .map_err(|e| format!("manifest create failed: {e}"))?;
    let mut manifest = tokio::io::BufWriter::new(manifest_file);
    let mut summary = CrawlSummary::default();
    let mut urls = HashSet::new();
    let mut seen_canonical = HashSet::new();
    let mut chrome_tasks: JoinSet<RefetchResult> = JoinSet::new();
    let mut chrome_results: Vec<RefetchResult> = Vec::new();
    let chrome_semaphore: Arc<Semaphore> = Arc::new(Semaphore::new(THIN_REFETCH_CONCURRENCY));

    loop {
        while let Some(r) = chrome_tasks.try_join_next() {
            match r {
                Ok(res) => chrome_results.push(res),
                Err(e) => log_warn(&format!("thin_refetch: Chrome task panicked: {e}")),
            }
        }

        let page = match rx.recv().await {
            Ok(p) => p,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                log_warn(&format!(
                    "crawl broadcast lagged: {n} pages dropped — increase subscribe buffer or reduce concurrency"
                ));
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        };
        process_received_page(
            page,
            &col,
            &mut summary,
            &mut urls,
            &mut seen_canonical,
            &mut manifest,
            &mut chrome_tasks,
            chrome_semaphore.clone(),
        )
        .await?;
    }

    drain_chrome_tasks(&mut chrome_tasks, &mut chrome_results).await;
    manifest
        .flush()
        .await
        .map_err(|e| format!("manifest flush failed: {e}"))?;
    if !chrome_results.is_empty() {
        summary = write_refetch_results(summary, chrome_results, &col.output_dir).await;
    }
    if let Some(tx) = col.progress_tx.as_ref() {
        tx.send(summary.clone()).await.ok();
    }
    Ok((summary, urls))
}

#[allow(
    clippy::too_many_arguments,
    reason = "Collector step threads mutable crawl state and async task handles; kept explicit for clarity"
)]
async fn process_received_page(
    page: spider::page::Page,
    col: &CollectorConfig,
    summary: &mut CrawlSummary,
    urls: &mut HashSet<String>,
    seen_canonical: &mut HashSet<String>,
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
    chrome_tasks: &mut JoinSet<RefetchResult>,
    chrome_semaphore: Arc<Semaphore>,
) -> Result<(), String> {
    let Some(url) = canonicalize_and_track_page(page.get_url(), col, summary, urls, seen_canonical)
    else {
        return Ok(());
    };
    // Restore pages_discovered tracking: track the running maximum of
    // (seen URLs so far + outbound links on this page) to give the UI a
    // realistic "total discovered" count in live progress updates.
    if let Some(links) = &page.page_links {
        summary.pages_discovered = summary
            .pages_discovered
            .max(seen_canonical.len() as u32 + links.len() as u32);
    }
    if !page.status_code.is_success() {
        log_info(&format!(
            "skip: {} (HTTP {})",
            url,
            page.status_code.as_u16()
        ));
        summary.error_pages += 1;
        emit_progress(col, summary).await;
        return Ok(());
    }
    track_waf_block(
        page.waf_check,
        page.blocked_crawl,
        &url,
        &page.anti_bot_tech,
        summary,
    );

    let html_bytes: Vec<u8> = page.get_html_bytes_u8().to_vec();
    let outcome = process_page(&html_bytes, &url, col, summary.markdown_files + 1);
    let _skip = apply_page_outcome(
        outcome,
        &html_bytes,
        &url,
        col,
        summary,
        manifest,
        chrome_tasks,
        chrome_semaphore,
    )
    .await?;
    emit_progress(col, summary).await;
    Ok(())
}

fn canonicalize_and_track_page(
    raw_url: &str,
    col: &CollectorConfig,
    summary: &mut CrawlSummary,
    urls: &mut HashSet<String>,
    seen_canonical: &mut HashSet<String>,
) -> Option<String> {
    let raw_url = raw_url.to_string();
    if is_excluded_url_path(&raw_url, &col.exclude_path_prefix) {
        return None;
    }
    let url = match col.scope.as_ref() {
        Some(scope) => normalize_map_candidate_url(&raw_url, scope, false)?,
        None => canonicalize_url_for_dedupe(&raw_url)?,
    };
    if !seen_canonical.insert(url.clone()) {
        return None;
    }
    summary.pages_seen += 1;
    urls.insert(url.clone());
    Some(url)
}

fn track_waf_block(
    waf_check: bool,
    blocked_crawl: bool,
    url: &str,
    anti_bot_tech: &impl std::fmt::Debug,
    summary: &mut CrawlSummary,
) {
    if !(waf_check || blocked_crawl) {
        return;
    }
    log_warn(&format!("waf: {} blocked by {:?}", url, anti_bot_tech));
    summary.waf_blocked_pages += 1;
    summary.waf_blocked_urls.insert(url.to_string());
}

async fn emit_progress(col: &CollectorConfig, summary: &CrawlSummary) {
    if let Some(tx) = col.progress_tx.as_ref() {
        tx.send(summary.clone()).await.ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::content::build_transform_config;

    fn test_collector_config(scope: Option<MapScope>) -> CollectorConfig {
        CollectorConfig {
            markdown_dir: std::env::temp_dir(),
            manifest_path: std::env::temp_dir().join("collector-manifest.jsonl"),
            min_chars: 10,
            drop_thin: false,
            exclude_path_prefix: Vec::new(),
            scope,
            transform_cfg: build_transform_config(),
            progress_tx: None,
            previous_manifest: Arc::new(HashMap::new()),
            selector_config: None,
            chrome_ws_url: None,
            chrome_timeout_secs: 1,
            output_dir: std::env::temp_dir(),
        }
    }

    #[test]
    fn canonicalize_and_track_page_rejects_same_host_root_outside_project_scope() {
        let col = test_collector_config(Some(MapScope {
            host: "example.github.io".to_string(),
            path_prefix: Some("/project".to_string()),
        }));
        let mut summary = CrawlSummary::default();
        let mut urls = HashSet::new();
        let mut seen = HashSet::new();

        let url = canonicalize_and_track_page(
            "https://example.github.io/",
            &col,
            &mut summary,
            &mut urls,
            &mut seen,
        );

        assert!(url.is_none());
        assert_eq!(summary.pages_seen, 0);
        assert!(urls.is_empty());
    }

    #[test]
    fn canonicalize_and_track_page_accepts_in_scope_project_page() {
        let col = test_collector_config(Some(MapScope {
            host: "example.github.io".to_string(),
            path_prefix: Some("/project".to_string()),
        }));
        let mut summary = CrawlSummary::default();
        let mut urls = HashSet::new();
        let mut seen = HashSet::new();

        let url = canonicalize_and_track_page(
            "https://example.github.io/project/docs/",
            &col,
            &mut summary,
            &mut urls,
            &mut seen,
        );

        assert_eq!(
            url.as_deref(),
            Some("https://example.github.io/project/docs")
        );
        assert_eq!(summary.pages_seen, 1);
        assert!(urls.contains("https://example.github.io/project/docs"));
    }
}
