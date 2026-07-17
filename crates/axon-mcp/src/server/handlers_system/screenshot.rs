use crate::schema::{AxonToolResponse, ResponseMode, ScreenshotRequest};
use crate::server::AxonMcpServer;
use crate::server::common::{invalid_params, logged_internal_error, validate_mcp_url};
use axon_core::config::ConfigOverrides;
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

        let cfg = self.cfg.apply_overrides(&ConfigOverrides {
            viewport_width: Some(width),
            viewport_height: Some(height),
            screenshot_full_page: Some(full_page),
            output_path: Some(None),
            ..ConfigOverrides::default()
        });

        let shot = axon_services::screenshot::screenshot_capture(&cfg, &url)
            .await
            .map_err(|e| logged_internal_error("screenshot", e.as_ref()))?;
        let artifact_id = shot.artifact_id.0.clone();
        let artifact_handle = serde_json::json!({
            "artifact_id": artifact_id,
            "artifact_kind": "screenshot",
        });
        let artifact = serde_json::json!({
            "artifact_id": artifact_id,
            "artifact_kind": "screenshot",
            "content_type": "image/png",
            "content_url": format!("/v1/artifacts/{artifact_id}/content"),
        });
        let payload = serde_json::to_value(&shot)
            .map_err(|error| logged_internal_error("screenshot response", &error))?;
        let response = match req.response_mode.unwrap_or(ResponseMode::Path) {
            ResponseMode::Path => serde_json::json!({
                "response_mode": "path",
                "data": payload,
                "artifact_handle": artifact_handle,
                "artifact": artifact,
                "shape": {
                    "type": "screenshot",
                    "viewport": format!("{}x{}", width, height),
                    "full_page": full_page,
                },
            }),
            ResponseMode::Inline | ResponseMode::AutoInline => serde_json::json!({
                "response_mode": "inline",
                "data": payload,
            }),
            ResponseMode::Both => serde_json::json!({
                "response_mode": "both",
                "data": payload,
                "artifact_handle": artifact_handle,
                "artifact": artifact,
            }),
        };
        Ok(AxonToolResponse::ok("screenshot", "screenshot", response)
            .with_artifact(artifact_handle))
    }
}
