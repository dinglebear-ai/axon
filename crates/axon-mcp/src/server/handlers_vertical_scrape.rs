//! MCP `vertical_scrape` action handler (axon_rust-kxot) — discovery only.
//!
//! `subaction=list` and `subaction=capabilities` expose the extractor catalog.
//!
//! Actual extraction happens transparently through `action=scrape` — the
//! service layer (`services::scrape::scrape`) calls `dispatch_by_url()` before
//! the generic HTTP path. No separate action needed for running extractors.
//!
//! `subaction=run` was removed; use `action=scrape url=<url>` instead.

use crate::schema::{AxonToolResponse, VerticalScrapeRequest, VerticalScrapeSubaction};
use crate::server::common::invalid_params;
use axon_extract::list_extractors;
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
                // Redirect: extraction now lives in action=scrape (services layer).
                Err(invalid_params(
                    "subaction=run is no longer needed: use action=scrape with any URL — \
                    vertical extractors fire automatically. \
                    Use subaction=list to discover which URLs each extractor claims.",
                ))
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

fn extractor_cap_json(info: axon_extract::ExtractorInfo) -> serde_json::Value {
    let auth_required = matches!(info.name, "github_repo" | "github_release" | "reddit");
    let env_vars: &[&str] = match info.name {
        "github_repo" | "github_release" => &["GITHUB_TOKEN (optional — 60/hr anon, 5000/hr auth)"],
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
