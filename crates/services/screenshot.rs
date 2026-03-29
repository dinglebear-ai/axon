use crate::crates::core::config::Config;
use crate::crates::core::http::{normalize_url, validate_url};
use crate::crates::crawl::screenshot::{
    spider_screenshot_with_options, url_to_screenshot_filename,
};
use crate::crates::services::types::ScreenshotResult;
use std::error::Error;

// --- Pure mapping helper (no I/O, testable without live services) ---

pub fn map_screenshot_result(payload: serde_json::Value) -> ScreenshotResult {
    ScreenshotResult { payload }
}

// --- Service functions ---

/// Capture a screenshot of the given URL and save it to the output directory.
///
/// Requires Chrome to be configured via cfg.chrome_remote_url. Returns a
/// `ScreenshotResult` containing the URL, output path, and file size in bytes.
#[must_use = "screenshot_capture returns a Result that should be handled"]
pub async fn screenshot_capture(
    cfg: &Config,
    url: &str,
) -> Result<ScreenshotResult, Box<dyn Error>> {
    if cfg.chrome_remote_url.is_none() {
        return Err(
            "screenshot requires Chrome — set AXON_CHROME_REMOTE_URL or pass --chrome-remote-url"
                .into(),
        );
    }

    let normalized = normalize_url(url);
    validate_url(&normalized)?;

    let bytes = capture_screenshot_bytes(
        cfg.clone(),
        normalized.to_string(),
        cfg.viewport_width,
        cfg.viewport_height,
        cfg.screenshot_full_page,
    )
    .await?;

    let path = if let Some(p) = &cfg.output_path {
        p.clone()
    } else {
        let dir = cfg.output_dir.join("screenshots");
        dir.join(url_to_screenshot_filename(&normalized, 1))
    };

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, &bytes).await?;

    let size_bytes = bytes.len() as u64;
    let payload = serde_json::json!({
        "url": normalized,
        "path": path.to_string_lossy(),
        "size_bytes": size_bytes,
    });

    Ok(map_screenshot_result(payload))
}

async fn capture_screenshot_bytes(
    cfg: Config,
    normalized: String,
    width: u32,
    height: u32,
    full_page: bool,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let task = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let worker = std::thread::Builder::new()
            .name("axon-screenshot".to_string())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || -> Result<Vec<u8>, String> {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| format!("failed to build screenshot runtime: {e}"))?;
                runtime
                    .block_on(spider_screenshot_with_options(
                        &cfg,
                        &normalized,
                        width,
                        height,
                        full_page,
                    ))
                    .map_err(|e| e.to_string())
            })
            .map_err(|e| format!("failed to spawn screenshot thread: {e}"))?;
        worker
            .join()
            .map_err(|_| "screenshot thread panicked".to_string())?
    })
    .await
    .map_err(|e| format!("screenshot task failed: {e}"))?;

    task.map_err(Into::into)
}
