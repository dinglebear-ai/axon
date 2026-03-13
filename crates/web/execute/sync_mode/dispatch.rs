use serde_json::json;
use tokio::sync::mpsc;

use crate::crates::services::acp as acp_svc;
use crate::crates::services::types::{
    DomainsResult, MapOptions, Pagination, RetrieveOptions, SearchOptions, SourcesResult,
};

use super::super::events::CommandContext;
use super::super::files;
use super::pulse_chat::{handle_pulse_chat, handle_pulse_chat_probe};
use super::service_calls::{
    call_ask, call_debug, call_dedupe, call_doctor, call_domains, call_evaluate, call_map,
    call_query, call_research, call_retrieve, call_scrape, call_screenshot, call_search,
    call_sessions, call_sources, call_stats, call_status, call_suggest, send_json_owned,
};
use super::types::{DirectParams, ServiceMode, SvcError};

fn format_sources_payload(result: SourcesResult) -> serde_json::Value {
    let urls_json: Vec<serde_json::Value> = result
        .urls
        .into_iter()
        .map(|(u, c)| json!({"url": u, "chunks": c}))
        .collect();
    json!({
        "count": result.count,
        "limit": result.limit,
        "offset": result.offset,
        "urls": urls_json,
    })
}

fn format_domains_payload(result: DomainsResult) -> serde_json::Value {
    let domains_json: Vec<serde_json::Value> = result
        .domains
        .into_iter()
        .map(|d| json!({"domain": d.domain, "vectors": d.vectors}))
        .collect();
    json!({
        "limit": result.limit,
        "offset": result.offset,
        "domains": domains_json,
    })
}

// ── Per-mode dispatch helpers ────────────────────────────────────────────────
// Each helper handles one ServiceMode arm, keeping `dispatch_service` concise.

struct QueryPagination {
    limit: usize,
    offset: usize,
    max_points: Option<usize>,
}

async fn dispatch_query_modes(
    mode: &ServiceMode,
    cfg: std::sync::Arc<crate::crates::core::config::Config>,
    input: String,
    pagination: QueryPagination,
    tx: mpsc::Sender<String>,
    ws_ctx: CommandContext,
) -> Option<Result<(), SvcError>> {
    let QueryPagination {
        limit,
        offset,
        max_points,
    } = pagination;
    match mode {
        ServiceMode::Scrape => {
            let result = match call_scrape(cfg, input).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx.clone(), ws_ctx.clone(), result.payload).await;
            files::send_scrape_file(tx, ws_ctx).await;
        }
        ServiceMode::Map => {
            let result = match call_map(cfg, input, MapOptions { limit, offset }).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, result.payload).await;
        }
        ServiceMode::Query => {
            let result = match call_query(cfg, input, Pagination { limit, offset }).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, json!({ "results": result.results })).await;
        }
        ServiceMode::Retrieve => {
            let result = match call_retrieve(cfg, input, RetrieveOptions { max_points }).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, json!({ "chunks": result.chunks })).await;
        }
        ServiceMode::Ask => {
            let result = match call_ask(cfg, input).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, result.payload).await;
        }
        _ => return None,
    }
    Some(Ok(()))
}

async fn dispatch_search_and_info_modes(
    mode: &ServiceMode,
    cfg: std::sync::Arc<crate::crates::core::config::Config>,
    input: String,
    limit: usize,
    offset: usize,
    tx: mpsc::Sender<String>,
    ws_ctx: CommandContext,
) -> Option<Result<(), SvcError>> {
    let search_opts = || SearchOptions {
        limit,
        offset,
        time_range: None,
    };
    match mode {
        ServiceMode::Search => {
            let result = match call_search(cfg, input, search_opts()).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, json!({ "results": result.results })).await;
        }
        ServiceMode::Research => {
            let result = match call_research(cfg, input, search_opts()).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, result.payload).await;
        }
        ServiceMode::Stats => {
            let result = match call_stats(cfg).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, result.payload).await;
        }
        ServiceMode::Sources => {
            let result = match call_sources(cfg, Pagination { limit, offset }).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, format_sources_payload(result)).await;
        }
        ServiceMode::Domains => {
            let result = match call_domains(cfg, Pagination { limit, offset }).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, format_domains_payload(result)).await;
        }
        ServiceMode::Doctor => {
            let result = match call_doctor(cfg).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, result.payload).await;
        }
        ServiceMode::Status => {
            let result = match call_status(cfg).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, result.payload).await;
        }
        ServiceMode::Suggest => {
            let focus = if input.is_empty() { None } else { Some(input) };
            let result = match call_suggest(cfg, focus).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, json!({ "urls": result.urls })).await;
        }
        ServiceMode::Evaluate => {
            let result = match call_evaluate(cfg, input).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, result.payload).await;
        }
        ServiceMode::Dedupe => {
            let result = match call_dedupe(cfg).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(
                tx,
                ws_ctx,
                json!({
                    "completed": result.completed,
                    "duplicate_groups": result.duplicate_groups,
                    "deleted": result.deleted,
                }),
            )
            .await;
        }
        ServiceMode::Screenshot => {
            let result = match call_screenshot(cfg, input).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, result.payload).await;
        }
        ServiceMode::Debug => {
            let result = match call_debug(cfg, input).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, result.payload).await;
        }
        ServiceMode::Sessions => {
            let result = match call_sessions(cfg).await {
                Ok(r) => r,
                Err(e) => return Some(Err(e)),
            };
            send_json_owned(tx, ws_ctx, result.payload).await;
        }
        _ => return None,
    }
    Some(Ok(()))
}

/// Inner dispatch — routes a pre-classified `ServiceMode` to the appropriate
/// service call and streams the result back over the WS channel.
///
/// Uses a `ServiceMode` enum rather than `match mode.as_str()` to prevent any
/// `&str` borrow from entering the async state machine, which would trigger an
/// HRTB `Send` diagnostic when the future is submitted to `tokio::task::spawn`.
pub(super) async fn dispatch_service(
    params: DirectParams,
    tx: mpsc::Sender<String>,
    ws_ctx: CommandContext,
    permission_responders: acp_svc::PermissionResponderMap,
) -> Result<(), SvcError> {
    let DirectParams {
        mode,
        input,
        cfg,
        limit,
        offset,
        max_points,
        agent,
        session_id,
        model,
        session_mode,
        enabled_mcp_servers,
        blocked_mcp_tools,
        assistant_mode,
        // ACP adapter capability fields — extracted from WS flags, threaded here for
        // future wiring into handle_pulse_chat / resolve_acp_adapter_command_from_values.
        enable_fs: _enable_fs,
        enable_terminal: _enable_terminal,
        permission_timeout_secs: _permission_timeout_secs,
        adapter_timeout_secs: _adapter_timeout_secs,
    } = params;

    if let Some(result) = dispatch_query_modes(
        &mode,
        cfg.clone(),
        input.clone(),
        QueryPagination {
            limit,
            offset,
            max_points,
        },
        tx.clone(),
        ws_ctx.clone(),
    )
    .await
    {
        return result;
    }

    if let Some(result) = dispatch_search_and_info_modes(
        &mode,
        cfg.clone(),
        input.clone(),
        limit,
        offset,
        tx.clone(),
        ws_ctx.clone(),
    )
    .await
    {
        return result;
    }

    match mode {
        ServiceMode::PulseChat => {
            handle_pulse_chat(
                cfg,
                input,
                session_id,
                model,
                session_mode,
                enabled_mcp_servers,
                blocked_mcp_tools,
                agent,
                assistant_mode,
                tx,
                ws_ctx,
                permission_responders,
            )
            .await?;
        }
        ServiceMode::PulseChatProbe => {
            handle_pulse_chat_probe(
                cfg,
                session_id,
                model,
                agent,
                tx,
                ws_ctx,
                permission_responders,
            )
            .await?;
        }
        ServiceMode::McpRefresh => {
            use crate::crates::services::acp::SESSION_CACHE;
            log::info!("[mcp_refresh] clearing ACP session cache");
            for key in SESSION_CACHE.agent_keys() {
                SESSION_CACHE.remove(&key);
            }
            send_json_owned(
                tx,
                ws_ctx,
                json!({
                    "type": "status",
                    "level": "info",
                    "message": "MCP servers connection handles cleared. They will be re-established on the next turn."
                }),
            )
            .await;
        }
        _ => {}
    }

    Ok(())
}
