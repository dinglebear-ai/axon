//! Action-API dispatcher functions for the `diff` and `brand` actions.

use super::super::internal_error;
use super::helpers::map_render_mode;
use crate::core::config::ConfigOverrides;
use crate::mcp::schema::{BrandRequest, DiffRequest};
use crate::services::brand as brand_svc;
use crate::services::context::ServiceContext;
use crate::services::diff as diff_svc;
use crate::services::types::ClientActionError;

pub async fn dispatch_diff(
    service_context: &ServiceContext,
    req: DiffRequest,
) -> Result<serde_json::Value, ClientActionError> {
    let url_a = req.url_a.ok_or_else(|| {
        ClientActionError::new("invalid_request", "url_a is required for diff", false, None)
    })?;
    let url_b = req.url_b.ok_or_else(|| {
        ClientActionError::new("invalid_request", "url_b is required for diff", false, None)
    })?;

    let cfg = service_context.cfg.apply_overrides(&ConfigOverrides {
        render_mode: req.render_mode.map(map_render_mode),
        ..ConfigOverrides::default()
    });

    let result = diff_svc::diff(&cfg, &url_a, &url_b, None)
        .await
        .map_err(internal_error)?;

    serde_json::to_value(result).map_err(|e| {
        ClientActionError::new(
            "internal",
            format!("serialize diff result: {e}"),
            false,
            None,
        )
    })
}

pub async fn dispatch_brand(
    service_context: &ServiceContext,
    req: BrandRequest,
) -> Result<serde_json::Value, ClientActionError> {
    let url = req.url.ok_or_else(|| {
        ClientActionError::new("invalid_request", "url is required for brand", false, None)
    })?;

    let result = brand_svc::brand(service_context.cfg.as_ref(), &url, None)
        .await
        .map_err(internal_error)?;

    serde_json::to_value(result).map_err(|e| {
        ClientActionError::new(
            "internal",
            format!("serialize brand result: {e}"),
            false,
            None,
        )
    })
}
