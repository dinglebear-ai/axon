//! MCP `vertical_scrape` action handler (axon_rust-kxot).
//!
//! Exposes the vertical-extractor framework via a single
//! `action=vertical_scrape` action with option-C dispatch
//! (extractor as a param — not 27 separate subactions).

use crate::extract::{VerticalContext, dispatch_by_name, list_extractors};
use crate::mcp::schema::{AxonToolResponse, VerticalScrapeRequest, VerticalScrapeSubaction};
use crate::mcp::server::common::{internal_error, invalid_params};
use crate::services::context::ServiceContext;
use crate::services::error::ServiceTaxonomyError;
use rmcp::ErrorData;
use serde_json::json;

impl super::super::AxonMcpServer {
    pub(super) async fn handle_vertical_scrape(
        &self,
        req: VerticalScrapeRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        match req.subaction {
            VerticalScrapeSubaction::List => Ok(handle_list()),
            VerticalScrapeSubaction::Capabilities => handle_capabilities(req.extractor),
            VerticalScrapeSubaction::Run => {
                let svc = self
                    .base_service_context()
                    .await
                    .map_err(|e| internal_error(e.to_string()))?;
                handle_run(req.extractor, req.url, &svc).await
            }
        }
    }
}

fn handle_list() -> AxonToolResponse {
    let verticals: Vec<serde_json::Value> = list_extractors()
        .into_iter()
        .map(|info| {
            json!({
                "id": info.name,
                "label": info.label,
                "description": info.description,
                "url_patterns": info.url_patterns,
                "auto_dispatch": info.auto_dispatch,
            })
        })
        .collect();
    let count = verticals.len();
    AxonToolResponse::ok(
        "vertical_scrape",
        "list",
        json!({ "verticals": verticals, "count": count }),
    )
}

fn handle_capabilities(extractor: Option<String>) -> Result<AxonToolResponse, ErrorData> {
    let catalog = list_extractors();
    let items: Vec<serde_json::Value> = if let Some(name) = &extractor {
        catalog
            .into_iter()
            .filter(|e| e.name == name.as_str())
            .map(extractor_cap_json)
            .collect()
    } else {
        catalog.into_iter().map(extractor_cap_json).collect()
    };

    if items.is_empty() {
        return Err(invalid_params(format!(
            "unknown extractor '{}'; use subaction=list to discover available extractors",
            extractor.unwrap_or_default()
        )));
    }
    Ok(AxonToolResponse::ok(
        "vertical_scrape",
        "capabilities",
        json!({ "extractors": items }),
    ))
}

fn extractor_cap_json(info: crate::extract::ExtractorInfo) -> serde_json::Value {
    let auth_required = matches!(info.name, "github_repo" | "github_release" | "reddit");
    let env_vars: &[&str] = match info.name {
        "github_repo" | "github_release" => {
            &["GITHUB_TOKEN (optional — 60/hr anon, 5000/hr auth)"]
        }
        "reddit" => &["REDDIT_CLIENT_ID", "REDDIT_CLIENT_SECRET"],
        "huggingface_model" => &["HF_TOKEN (optional)"],
        _ => &[],
    };
    json!({
        "id": info.name,
        "label": info.label,
        "url_patterns": info.url_patterns,
        "auto_dispatch": info.auto_dispatch,
        "auth_required": auth_required,
        "env_vars": env_vars,
    })
}

async fn handle_run(
    extractor: Option<String>,
    url: Option<String>,
    svc: &ServiceContext,
) -> Result<AxonToolResponse, ErrorData> {
    let extractor_name = extractor
        .ok_or_else(|| invalid_params("extractor is required for subaction=run"))?;
    let url = url.ok_or_else(|| invalid_params("url is required for subaction=run"))?;

    let ctx = VerticalContext::from(svc);
    match dispatch_by_name(&extractor_name, &url, &ctx).await {
        Ok(doc) => Ok(AxonToolResponse::ok(
            "vertical_scrape",
            "run",
            json!({
                "url": doc.url,
                "extractor": doc.extractor_name,
                "extractor_version": doc.extractor_version,
                "title": doc.title,
                "markdown": doc.markdown,
                "structured": doc.structured,
            }),
        )),
        Err(e) => Err(vertical_error_to_mcp(e, &extractor_name)),
    }
}

/// Map VerticalError variants to machine-readable MCP `ErrorData` so agents
/// can branch on retry strategy (per kxot locked decisions from lavra-research).
fn vertical_error_to_mcp(e: ServiceTaxonomyError, extractor: &str) -> ErrorData {
    let (retriable, code, msg) = match &e {
        ServiceTaxonomyError::VerticalRateLimited { .. } => {
            (true, "vertical_rate_limited", e.to_string())
        }
        ServiceTaxonomyError::VerticalAuthMissing { .. } => {
            (false, "vertical_auth_missing", e.to_string())
        }
        ServiceTaxonomyError::VerticalAuthInvalid { .. } => {
            (false, "vertical_auth_invalid", e.to_string())
        }
        ServiceTaxonomyError::VerticalUnsupportedUrl { .. } => {
            (false, "vertical_unsupported_url", e.to_string())
        }
        ServiceTaxonomyError::VerticalTargetNotFound { .. } => {
            (false, "vertical_target_not_found", e.to_string())
        }
        ServiceTaxonomyError::VerticalBlockedAntibot { .. } => {
            (true, "vertical_blocked_antibot", e.to_string())
        }
        ServiceTaxonomyError::VerticalTargetUnavailable { .. } => {
            (true, "vertical_target_unavailable", e.to_string())
        }
        _ => (false, "vertical_error", e.to_string()),
    };
    ErrorData::internal_error(
        msg,
        Some(json!({
            "error_code": code,
            "retriable": retriable,
            "extractor": extractor,
        })),
    )
}
