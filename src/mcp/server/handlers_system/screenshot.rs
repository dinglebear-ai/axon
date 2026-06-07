use crate::core::config::ConfigOverrides;
use crate::mcp::schema::{AxonToolResponse, ResponseMode, ScreenshotRequest};
use crate::mcp::server::AxonMcpServer;
use crate::mcp::server::artifacts::{
    artifact_handle_for_path, ensure_artifact_root, resolve_artifact_output_path,
};
use crate::mcp::server::common::{invalid_params, logged_internal_error, validate_mcp_url};
use rmcp::ErrorData;

impl AxonMcpServer {
    fn parse_viewport(
        viewport: Option<&str>,
        fallback_w: u32,
        fallback_h: u32,
    ) -> Result<(u32, u32), ErrorData> {
        let Some(v) = viewport else {
            return Ok((fallback_w, fallback_h));
        };
        let mut parts = v.split('x');
        let w = parts
            .next()
            .and_then(|n| n.parse::<u32>().ok())
            .ok_or_else(|| {
                invalid_params(format!(
                    "invalid viewport '{v}': expected WxH format (e.g. 1280x720)"
                ))
            })?;
        let h = parts
            .next()
            .and_then(|n| n.parse::<u32>().ok())
            .ok_or_else(|| {
                invalid_params(format!(
                    "invalid viewport '{v}': expected WxH format (e.g. 1280x720)"
                ))
            })?;
        if w == 0 || h == 0 {
            return Err(invalid_params(format!(
                "invalid viewport '{v}': width and height must be greater than zero"
            )));
        }
        if w > 7680 || h > 4320 {
            return Err(invalid_params(format!(
                "invalid viewport '{v}': dimensions exceed maximum allowed (7680x4320)"
            )));
        }
        Ok((w, h))
    }

    pub(crate) async fn handle_screenshot(
        &self,
        req: ScreenshotRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let url = req
            .url
            .ok_or_else(|| invalid_params("url is required for screenshot"))?;
        validate_mcp_url(&url)?;
        let (width, height) = Self::parse_viewport(
            req.viewport.as_deref(),
            self.cfg.viewport_width,
            self.cfg.viewport_height,
        )?;
        let full_page = req.full_page.unwrap_or(self.cfg.screenshot_full_page);

        let artifact_root = ensure_artifact_root().await?;
        let output_path = if let Some(output) = req.output {
            resolve_artifact_output_path(&output).await?
        } else {
            let screenshots_dir = artifact_root.join("screenshots");
            tokio::fs::create_dir_all(&screenshots_dir)
                .await
                .map_err(|e| logged_internal_error("screenshot dir", &e))?;
            screenshots_dir.join(format!(
                "{}.png",
                chrono::Utc::now().format("%Y%m%d-%H%M%S-%3f")
            ))
        };

        let mut cfg = self.cfg.apply_overrides(&ConfigOverrides {
            viewport_width: Some(width),
            viewport_height: Some(height),
            screenshot_full_page: Some(full_page),
            output_path: Some(Some(output_path.clone())),
            ..ConfigOverrides::default()
        });
        cfg.output_dir = artifact_root;

        let shot = crate::services::screenshot::screenshot_capture(&cfg, &url)
            .await
            .map_err(|e| logged_internal_error("screenshot", e.as_ref()))?;
        let artifact_handle = artifact_handle_for_path(
            "screenshot",
            &output_path,
            shot.size_bytes,
            None,
            None,
            Some(shot.url.clone()),
        )
        .await?;

        let relative_path = artifact_handle.relative_path.clone();
        let display_path = artifact_handle.display_path.clone();
        let payload = serde_json::json!({
            "url": shot.url,
            "path": relative_path,
            "relative_path": relative_path,
            "display_path": display_path,
            "artifact_handle": artifact_handle,
            "size_bytes": shot.size_bytes,
            "full_page": full_page,
            "viewport": format!("{}x{}", width, height),
        });
        // Screenshot already materializes the primary artifact as a PNG on disk.
        // Returning the small metadata envelope inline avoids a second JSON
        // artifact round-trip and prevents MCP stdio crashes in this path.
        let response = match req.response_mode.unwrap_or(ResponseMode::Path) {
            ResponseMode::Path => serde_json::json!({
                "response_mode": "path",
                "data": payload.clone(),
                "artifact_handle": payload["artifact_handle"].clone(),
                "artifact": {
                    "artifact_handle": payload["artifact_handle"].clone(),
                    "path": payload["artifact_handle"]["relative_path"].clone(),
                    "relative_path": payload["artifact_handle"]["relative_path"].clone(),
                    "display_path": payload["artifact_handle"]["display_path"].clone(),
                    "bytes": payload["size_bytes"].clone(),
                    "mime_type": "image/png",
                },
                "shape": {
                    "type": "screenshot",
                    "viewport": payload["viewport"].clone(),
                    "full_page": payload["full_page"].clone(),
                },
            }),
            ResponseMode::Inline | ResponseMode::AutoInline => serde_json::json!({
                "response_mode": "inline",
                "data": payload,
            }),
            ResponseMode::Both => serde_json::json!({
                "response_mode": "both",
                "data": payload.clone(),
                "artifact_handle": payload["artifact_handle"].clone(),
                "artifact": {
                    "artifact_handle": payload["artifact_handle"].clone(),
                    "path": payload["artifact_handle"]["relative_path"].clone(),
                    "relative_path": payload["artifact_handle"]["relative_path"].clone(),
                    "display_path": payload["artifact_handle"]["display_path"].clone(),
                    "bytes": payload["size_bytes"].clone(),
                    "mime_type": "image/png",
                },
            }),
        };
        Ok(AxonToolResponse::ok("screenshot", "screenshot", response))
    }
}
