use crate::context::ServiceContext;
use crate::endpoints as endpoints_svc;
use crate::extract as extract_svc;
use crate::screenshot as screenshot_svc;
use crate::summarize as summarize_svc;
use crate::types::ClientActionError;
use axon_api::mcp_schema::{
    EndpointsRequest, ExtractRequest, ExtractSubaction, ScreenshotRequest, SummarizeRequest,
};
use axon_core::config::{Config, ConfigOverrides};

use super::super::internal_error;
use super::helpers::{map_render_mode, parse_viewport};

pub async fn dispatch_extract(
    service_context: &ServiceContext,
    req: ExtractRequest,
) -> Result<serde_json::Value, ClientActionError> {
    match req.subaction.unwrap_or(ExtractSubaction::Start) {
        ExtractSubaction::Start => {
            let urls = req.urls.ok_or_else(|| {
                ClientActionError::new("invalid_request", "urls are required", false, None)
            })?;
            let prompt = req.prompt.clone();
            let cfg = service_context.cfg.apply_overrides(&ConfigOverrides {
                query: Some(prompt.clone()),
                max_pages: req.max_pages,
                render_mode: req.render_mode.map(map_render_mode),
                embed: req.embed,
                ..ConfigOverrides::default()
            });
            // KNOWN GAP (not "fine by design"): the web-panel/CLI generic
            // client-action dispatch path (`action_api::dispatch_action`,
            // reached from `axon-web/src/server/handlers/config.rs`'s panel
            // command route) does not have a caller `AuthContext` threaded
            // into it, so this `None` silently falls back to
            // `AuthSnapshot::trusted_system` and the job's recorded
            // `auth_snapshot` cannot reflect the real panel/CLI caller. The
            // MCP-side equivalents (extract.start via `axon-mcp`) now thread
            // a real caller-derived `AuthSnapshot` — see
            // `crates/axon-mcp/src/server/common.rs::CURRENT_CALLER_AUTH_SNAPSHOT`.
            // Wiring the same here needs the panel route handler to accept
            // `Extension<AuthContext>` and for that identity to flow through
            // `dispatch_action`'s call sites — tracked as a follow-up, not
            // fixed in this pass. Does not grant extra privilege today
            // (`require_job_scope` only enforces on Reset/Prune), but it
            // corrupts audit attribution and is a trap for future
            // finer-grained scoping work.
            let outcome = extract_svc::extract_start_with_context(
                &cfg,
                &urls,
                prompt,
                service_context,
                None,
                None,
            )
            .await
            .map_err(internal_error)?;
            Ok(serde_json::json!({ "job_id": outcome.result.job_id, "status": "pending" }))
        }
    }
}

pub async fn dispatch_endpoints(
    service_context: &ServiceContext,
    req: EndpointsRequest,
) -> Result<serde_json::Value, ClientActionError> {
    let url = req
        .url
        .ok_or_else(|| ClientActionError::new("invalid_request", "url is required", false, None))?;
    let mut options = endpoints_svc::options_from_config(service_context.cfg.as_ref());
    if let Some(value) = req.include_bundles {
        options.include_bundles = value;
    }
    if let Some(value) = req.first_party_only {
        options.first_party_only = value;
    }
    if let Some(value) = req.unique_only {
        options.unique_only = value;
    }
    if let Some(value) = req.max_scripts {
        options.max_scripts = value;
    }
    if let Some(value) = req.max_scan_bytes {
        options.max_scan_bytes = value;
    }
    if let Some(value) = req.verify {
        options.verify = value;
    }
    if let Some(value) = req.capture_network {
        options.capture_network = value;
    }
    if let Some(value) = req.probe_rpc {
        options.probe_rpc = value;
    }
    if let Some(value) = req.probe_rpc_subdomains {
        options.probe_rpc_subdomains = value;
    }
    let result = endpoints_svc::discover(service_context.cfg.as_ref(), &url, options, None)
        .await
        .map_err(|err| ClientActionError::new("internal", err.to_string(), true, None))?;
    serde_json::to_value(result).map_err(|err| {
        ClientActionError::new(
            "internal",
            format!("serialize endpoints result: {err}"),
            false,
            None,
        )
    })
}

pub async fn dispatch_summarize(
    service_context: &ServiceContext,
    req: SummarizeRequest,
) -> Result<serde_json::Value, ClientActionError> {
    let urls = {
        let collected = crate::action_api::collect_unique_urls(req.url, req.urls);
        if collected.is_empty() {
            return Err(ClientActionError::new(
                "invalid_request",
                "url or urls is required",
                false,
                None,
            ));
        }
        collected
    };
    let cfg = service_context.cfg.apply_overrides(&ConfigOverrides {
        render_mode: req.render_mode.map(map_render_mode),
        root_selector: req.root_selector,
        exclude_selector: req.exclude_selector,
        ..ConfigOverrides::default()
    });
    let result = summarize_svc::summarize(&cfg, &urls, None)
        .await
        .map_err(internal_error)?;
    serde_json::to_value(result).map_err(|err| {
        ClientActionError::new(
            "internal",
            format!("serialize summarize result: {err}"),
            false,
            None,
        )
    })
}

pub async fn dispatch_screenshot(
    service_context: &ServiceContext,
    req: ScreenshotRequest,
) -> Result<serde_json::Value, ClientActionError> {
    let url = req
        .url
        .ok_or_else(|| ClientActionError::new("invalid_request", "url is required", false, None))?;
    let (width, height) = parse_viewport(
        req.viewport.as_deref(),
        service_context.cfg.viewport_width,
        service_context.cfg.viewport_height,
    )?;
    let cfg = service_context.cfg.apply_overrides(&ConfigOverrides {
        viewport_width: Some(width),
        viewport_height: Some(height),
        screenshot_full_page: req.full_page,
        ..ConfigOverrides::default()
    });
    let result = screenshot_svc::screenshot_capture(&cfg, &url)
        .await
        .map_err(internal_error)?;
    serde_json::to_value(result).map_err(|err| {
        ClientActionError::new(
            "internal",
            format!("serialize screenshot result: {err}"),
            false,
            None,
        )
    })
}
