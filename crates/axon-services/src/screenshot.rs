use crate::types::ScreenshotResult;
use axon_adapters::web_engine::screenshot::spider_screenshot_with_options;
use axon_api::source::{ArtifactKind, MetadataMap, Timestamp};
use axon_core::artifacts::atomic_write_explicit;
use axon_core::boundary::{ArtifactBytesWriteRequest, ArtifactStore, FileArtifactStore};
use axon_core::config::Config;
use axon_core::http::{normalize_url, validate_url};
use std::error::Error;

// --- Service functions ---

/// Capture a screenshot and persist it behind an opaque artifact identifier.
///
/// An explicitly configured output path is also written for CLI convenience,
/// but filesystem paths never cross the service contract.
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

    if let Some(output_path) = cfg.output_path.as_deref() {
        atomic_write_explicit(output_path, &bytes)
            .await
            .map_err(|err| -> Box<dyn Error> { err.to_string().into() })?;
    }

    let captured_at = Timestamp::from(chrono::Utc::now());
    let mut metadata = MetadataMap::new();
    metadata.insert("source_url".to_string(), normalized.to_string().into());
    metadata.insert("width".to_string(), cfg.viewport_width.into());
    metadata.insert("height".to_string(), cfg.viewport_height.into());
    metadata.insert("full_page".to_string(), cfg.screenshot_full_page.into());
    metadata.insert("captured_at".to_string(), captured_at.0.clone().into());
    metadata.insert("label".to_string(), "screenshot.png".into());
    let handle = FileArtifactStore::new(cfg.output_dir.join("artifacts"))
        .put_bytes(ArtifactBytesWriteRequest {
            kind: ArtifactKind::Screenshot,
            content_type: "image/png".to_string(),
            bytes,
            source_id: None,
            job_id: None,
            metadata,
        })
        .await
        .map_err(|err| -> Box<dyn Error> { Box::new(err) })?;

    Ok(ScreenshotResult {
        artifact_id: handle.artifact_id,
        width: cfg.viewport_width,
        height: cfg.viewport_height,
        captured_at,
        warnings: Vec::new(),
    })
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

#[cfg(test)]
#[path = "screenshot_tests.rs"]
mod tests;
