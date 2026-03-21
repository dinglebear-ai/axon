use crate::crates::mcp::schema::{AxonToolResponse, ScreenshotRequest};
use crate::crates::mcp::server::AxonMcpServer;
use crate::crates::mcp::server::artifacts::{
    ensure_artifact_root, resolve_artifact_output_path, respond_with_mode,
};
use crate::crates::mcp::server::common::{invalid_params, logged_internal_error, validate_mcp_url};
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
        let response_mode = req.response_mode;

        let (width, height) = Self::parse_viewport(
            req.viewport.as_deref(),
            self.cfg.viewport_width,
            self.cfg.viewport_height,
        )?;
        let full_page = req.full_page.unwrap_or(self.cfg.screenshot_full_page);

        let output_path = if let Some(output) = req.output {
            resolve_artifact_output_path(&output).await?
        } else {
            let screenshots_dir = ensure_artifact_root().await?.join("screenshots");
            tokio::fs::create_dir_all(&screenshots_dir)
                .await
                .map_err(|e| logged_internal_error("screenshot dir", &e))?;
            screenshots_dir.join(format!(
                "{}.png",
                chrono::Utc::now().format("%Y%m%d-%H%M%S-%3f")
            ))
        };

        let mut cfg = self.cfg.as_ref().clone();
        cfg.viewport_width = width;
        cfg.viewport_height = height;
        cfg.screenshot_full_page = full_page;
        cfg.output_path = Some(output_path.clone());

        let mut shot = crate::crates::services::screenshot::screenshot_capture(&cfg, &url)
            .await
            .map_err(|e| logged_internal_error("screenshot", e.as_ref()))?;

        let (size_bytes, normalized, path) = if let Some(map) = shot.payload.as_object_mut() {
            (
                map.remove("size_bytes").unwrap_or(serde_json::json!(0)),
                map.remove("url").unwrap_or_else(|| serde_json::json!(url)),
                map.remove("path")
                    .unwrap_or_else(|| serde_json::json!(output_path)),
            )
        } else {
            (
                serde_json::json!(0),
                serde_json::json!(url),
                serde_json::json!(output_path),
            )
        };

        let payload = serde_json::json!({
            "url": normalized,
            "path": path,
            "size_bytes": size_bytes,
            "full_page": full_page,
            "viewport": format!("{}x{}", width, height),
        });
        respond_with_mode(
            "screenshot",
            "screenshot",
            response_mode,
            "screenshot",
            payload,
        )
        .await
    }
}
