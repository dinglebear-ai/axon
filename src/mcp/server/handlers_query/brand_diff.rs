//! MCP handlers for the `brand` and `diff` actions.

use crate::core::config::ConfigOverrides;
use crate::mcp::schema::{AxonToolResponse, BrandRequest, DiffRequest};
use crate::mcp::server::AxonMcpServer;
use crate::mcp::server::common::{
    internal_error, invalid_params, logged_internal_error, map_render_mode, validate_mcp_url,
};
use crate::services::{brand as brand_svc, diff as diff_svc};
use rmcp::ErrorData;

impl AxonMcpServer {
    pub(in crate::mcp::server) async fn handle_diff(
        &self,
        req: DiffRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let url_a = req
            .url_a
            .clone()
            .ok_or_else(|| invalid_params("url_a is required for diff"))?;
        let url_b = req
            .url_b
            .clone()
            .ok_or_else(|| invalid_params("url_b is required for diff"))?;

        validate_mcp_url(&url_a)?;
        validate_mcp_url(&url_b)?;

        let cfg = self.cfg.apply_overrides(&ConfigOverrides {
            render_mode: req.render_mode.map(map_render_mode),
            ..ConfigOverrides::default()
        });

        let result = diff_svc::diff(&cfg, &url_a, &url_b, None)
            .await
            .map_err(|e| logged_internal_error("diff", e.as_ref()))?;

        let data = serde_json::to_value(&result)
            .map_err(|e| internal_error(format!("serialize diff result: {e}")))?;

        Ok(AxonToolResponse::ok("diff", "diff", data))
    }

    pub(in crate::mcp::server) async fn handle_brand(
        &self,
        req: BrandRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        let url = req
            .url
            .clone()
            .ok_or_else(|| invalid_params("url is required for brand"))?;

        validate_mcp_url(&url)?;

        let result = brand_svc::brand(self.cfg.as_ref(), &url, None)
            .await
            .map_err(|e| logged_internal_error("brand", e.as_ref()))?;

        let data = serde_json::to_value(&result)
            .map_err(|e| internal_error(format!("serialize brand result: {e}")))?;

        Ok(AxonToolResponse::ok("brand", "brand", data))
    }
}
