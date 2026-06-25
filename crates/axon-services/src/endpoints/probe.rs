use super::{EndpointError, validate_url_with_dns_timeout};
use crate::types::{EndpointReport, EndpointSourceKind, RpcProbeResult, RpcProtocol, RpcTransport};
use axon_core::config::Config;
use axon_core::http::{axon_ua, build_client};
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::sync::LazyLock;
use tokio::sync::Semaphore;

/// Hard upper bound on the per-probe HTTP timeout. A configured `request_timeout_ms`
/// can only make probing *faster*, never slower than this — probing must stay snappy.
const PROBE_TIMEOUT_SECS: u64 = 3;
const MAX_PROBE_ENDPOINTS: usize = 20;
/// Cap on bytes read from any single probe response body (JSON or SSE frame).
const MAX_PROBE_BODY_BYTES: usize = 256 * 1024;
/// MCP protocol version advertised in the `initialize` handshake. Servers that
/// negotiate a different version still echo their own `serverInfo`, so detection
/// is unaffected by the exact value.
const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// Per-endpoint concurrency cap, shared across all concurrent discovery sessions.
/// Override with `AXON_ENDPOINT_PROBE_CONCURRENCY` (default 4, min 1).
fn probe_concurrency() -> usize {
    std::env::var("AXON_ENDPOINT_PROBE_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(4)
        .max(1)
}

/// Process-wide semaphore so `AXON_ENDPOINT_PROBE_CONCURRENCY` bounds the total
/// number of in-flight probes across every discovery session — not just one.
static PROBE_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(probe_concurrency()));

fn probe_timeout_secs(cfg: &Config) -> u64 {
    cfg.request_timeout_ms
        .map(|ms| ms.div_ceil(1000).clamp(1, PROBE_TIMEOUT_SECS))
        .unwrap_or(PROBE_TIMEOUT_SECS)
}

pub(super) async fn probe_rpc_endpoints(
    cfg: &Config,
    target_url: &str,
    include_subdomain: bool,
    report: &mut EndpointReport,
) {
    let client = match build_client(probe_timeout_secs(cfg), Some(axon_ua())) {
        Ok(c) => c,
        Err(err) => {
            report
                .warnings
                .push(format!("rpc probe client unavailable: {err}"));
            return;
        }
    };

    let eligible: Vec<(usize, String)> = report
        .endpoints
        .iter()
        .enumerate()
        .filter_map(|(idx, ep)| {
            let url = ep.normalized_url.as_deref().unwrap_or(ep.value.as_str());
            if url.starts_with("http://") || url.starts_with("https://") {
                Some((idx, url.to_string()))
            } else {
                None
            }
        })
        .collect();

    if eligible.len() > MAX_PROBE_ENDPOINTS {
        report.warnings.push(format!(
            "rpc probe capped at {MAX_PROBE_ENDPOINTS} endpoints; skipped {} additional",
            eligible.len() - MAX_PROBE_ENDPOINTS
        ));
    }

    let targets: Vec<(usize, String)> = eligible.into_iter().take(MAX_PROBE_ENDPOINTS).collect();
    let results: Vec<_> = stream::iter(targets)
        .map(|(idx, url)| {
            let client = client.clone();
            async move {
                // Acquire per-endpoint (not per-session) so the cap is honored
                // globally; mirrors the bundle-fetch semaphore pattern.
                // `acquire()` only errors if the semaphore is closed, which never
                // happens — it is a process-wide `static` that is never dropped or
                // `close()`d — so a bare `None` here is unreachable, not a silently
                // dropped endpoint.
                let _permit = match PROBE_SEMAPHORE.acquire().await {
                    Ok(p) => p,
                    Err(_) => return (idx, None),
                };
                (idx, probe_one(&client, &url).await)
            }
        })
        .buffer_unordered(probe_concurrency())
        .collect()
        .await;

    for (idx, probe_result) in results {
        if let (Some(rpc), Some(endpoint)) = (probe_result, report.endpoints.get_mut(idx)) {
            endpoint.rpc_probe = Some(rpc);
        }
    }

    // Synthesize + probe well-known MCP candidates from the target URL itself.
    super::candidates::synthesize_and_probe_mcp(&client, target_url, include_subdomain, report)
        .await;
    if report
        .endpoints
        .iter()
        .any(|e| e.source == EndpointSourceKind::SynthesizedMcp)
    {
        super::recompute_hosts(report);
    }
}

async fn probe_one(client: &reqwest::Client, url: &str) -> Option<RpcProbeResult> {
    if validate_url_with_dns_timeout(url).await.is_err() {
        return None;
    }
    // Probe order: MCP initialize → OpenRPC rpc.discover → system.listMethods →
    // JSON-RPC fingerprint (-32601) → SSE transport detection
    if let Some(result) = probe_mcp(client, url).await {
        return Some(result);
    }
    if let Some(result) = probe_openrpc(client, url).await {
        return Some(result);
    }
    if let Some(result) = probe_list_methods(client, url).await {
        return Some(result);
    }
    if let Some(result) = probe_jsonrpc_fingerprint(client, url).await {
        return Some(result);
    }
    probe_sse_transport(client, url).await
}

/// Strict probe for synthesized candidates: positive-signal POST probes only,
/// no bare-SSE content-type fallback (too false-positive-prone for guesses).
/// Streamable-HTTP MCP is still detected because `probe_mcp`'s `initialize`
/// response (incl. `text/event-stream` bodies) is parsed inside `send_jsonrpc`.
///
/// Validates the URL through the SSRF guard before issuing any request, like
/// `probe_one` — so it is self-guarding, not reliant on the caller.
pub(super) async fn probe_candidate(client: &reqwest::Client, url: &str) -> Option<RpcProbeResult> {
    let _permit = PROBE_SEMAPHORE.acquire().await.ok()?;
    if validate_url_with_dns_timeout(url).await.is_err() {
        return None;
    }
    if let Some(r) = probe_mcp(client, url).await {
        return Some(r);
    }
    if let Some(r) = probe_openrpc(client, url).await {
        return Some(r);
    }
    if let Some(r) = probe_list_methods(client, url).await {
        return Some(r);
    }
    probe_jsonrpc_fingerprint(client, url).await
}

/// Read up to `cap` bytes of a response body, returning lossy UTF-8 text.
async fn read_body_capped(resp: reqwest::Response, cap: usize) -> Result<String, EndpointError> {
    let mut stream = resp.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let remaining = cap.saturating_sub(bytes.len());
        if remaining == 0 {
            break;
        }
        let take = remaining.min(chunk.len());
        bytes.extend_from_slice(&chunk[..take]);
        if take < chunk.len() {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Stream an SSE body and return the JSON payload of the first `data:` event.
/// Stops as soon as one complete event is parsed so a server that keeps the
/// stream open after replying does not stall the probe until timeout.
async fn read_first_sse_json(
    resp: reqwest::Response,
    cap: usize,
) -> Result<Option<Value>, EndpointError> {
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let remaining = cap.saturating_sub(buf.len());
        if remaining == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..remaining.min(chunk.len())]);
        // Consume every complete event block that has fully arrived, returning
        // the first that yields JSON. Draining consumed blocks is what prevents
        // a non-data preamble (keepalive `:` comments, empty events) from being
        // re-scanned forever against a stream the server keeps open.
        while let Some((content_len, sep_len)) = sse_event_boundary(&buf) {
            let block = String::from_utf8_lossy(&buf[..content_len]).into_owned();
            buf.drain(..content_len + sep_len);
            if let Some(value) = parse_sse_event(&block) {
                return Ok(Some(value));
            }
        }
        if buf.len() >= cap {
            break;
        }
    }
    // Trailing event with no final blank line (e.g. stream closed mid-frame).
    Ok(parse_sse_event(&String::from_utf8_lossy(&buf)))
}

/// Locate the first SSE event boundary (a blank line) in `buf`, scanning raw
/// bytes so split multibyte chars can't desync the offset. Returns
/// `(content_len, separator_len)` where `buf[..content_len]` is the event block
/// and `separator_len` is the blank-line delimiter (`\n\n` or `\r\n\r\n`).
fn sse_event_boundary(buf: &[u8]) -> Option<(usize, usize)> {
    for i in 0..buf.len() {
        if buf[i..].starts_with(b"\r\n\r\n") {
            return Some((i, 4));
        }
        if buf[i..].starts_with(b"\n\n") {
            return Some((i, 2));
        }
    }
    None
}

/// Concatenate the `data:` lines of one SSE event block and parse them as JSON.
fn parse_sse_event(block: &str) -> Option<Value> {
    let mut data = String::new();
    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            let rest = rest.strip_prefix(' ').unwrap_or(rest);
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(rest);
        }
    }
    if data.is_empty() {
        return None;
    }
    serde_json::from_str(&data).ok()
}

/// POST a JSON-RPC request and return the parsed response, handling both
/// `application/json` and Streamable-HTTP `text/event-stream` replies. Returns
/// any `Mcp-Session-Id` the server assigned so callers can replay it.
async fn send_jsonrpc(
    client: &reqwest::Client,
    url: &str,
    body: Value,
    session_id: Option<&str>,
) -> Result<(Option<String>, Value), EndpointError> {
    let mut req = client
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(
            reqwest::header::ACCEPT,
            "application/json, text/event-stream",
        );
    if let Some(sid) = session_id {
        req = req.header("mcp-session-id", sid);
    }
    let resp = req.json(&body).send().await?;
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let session_out = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let value = if ct.contains("text/event-stream") {
        read_first_sse_json(resp, MAX_PROBE_BODY_BYTES)
            .await?
            .ok_or_else(|| EndpointError::from("no JSON-RPC frame in SSE response"))?
    } else if ct.contains("json") {
        let text = read_body_capped(resp, MAX_PROBE_BODY_BYTES).await?;
        serde_json::from_str(&text)?
    } else {
        return Err("response is not JSON".into());
    };
    Ok((session_out, value))
}

async fn post_jsonrpc(
    client: &reqwest::Client,
    url: &str,
    body: Value,
) -> Result<Value, EndpointError> {
    Ok(send_jsonrpc(client, url, body, None).await?.1)
}

/// Fire-and-forget JSON-RPC notification (no response body is parsed).
async fn post_notification(
    client: &reqwest::Client,
    url: &str,
    body: Value,
    session_id: Option<&str>,
) {
    let mut req = client
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(
            reqwest::header::ACCEPT,
            "application/json, text/event-stream",
        );
    if let Some(sid) = session_id {
        req = req.header("mcp-session-id", sid);
    }
    let _ = req.json(&body).send().await;
}

async fn probe_mcp(client: &reqwest::Client, url: &str) -> Option<RpcProbeResult> {
    let body = json!({
        "jsonrpc": "2.0",
        "method": "initialize",
        "params": {
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "axon-probe", "version": "1.0" }
        },
        "id": 1
    });
    let (session_id, resp) = send_jsonrpc(client, url, body, None).await.ok()?;
    let result = resp.get("result")?;
    let server_info = result.get("serverInfo")?;
    let server_name = server_info
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string);
    let server_version = server_info
        .get("version")
        .and_then(Value::as_str)
        .map(str::to_string);
    // Complete the handshake before listing tools — stateful servers reject
    // requests issued before `notifications/initialized`.
    post_notification(
        client,
        url,
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        session_id.as_deref(),
    )
    .await;
    let tools = probe_mcp_tools(client, url, session_id.as_deref())
        .await
        .unwrap_or_default();
    Some(RpcProbeResult {
        protocol: Some(RpcProtocol::Mcp),
        transport: Some(RpcTransport::Http),
        server_name,
        server_version,
        methods: Vec::new(),
        tools,
    })
}

async fn probe_mcp_tools(
    client: &reqwest::Client,
    url: &str,
    session_id: Option<&str>,
) -> Option<Vec<String>> {
    let body = json!({"jsonrpc": "2.0", "method": "tools/list", "id": 2});
    let (_session, resp) = send_jsonrpc(client, url, body, session_id).await.ok()?;
    let tools = resp.get("result")?.get("tools")?.as_array()?;
    Some(
        tools
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str).map(str::to_string))
            .collect(),
    )
}

async fn probe_openrpc(client: &reqwest::Client, url: &str) -> Option<RpcProbeResult> {
    let body = json!({"jsonrpc": "2.0", "method": "rpc.discover", "id": 2});
    let resp = post_jsonrpc(client, url, body).await.ok()?;
    let result = resp.get("result")?;
    result.get("openrpc")?; // Must have "openrpc" field
    let methods: Vec<String> = result
        .get("methods")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("name").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Some(RpcProbeResult {
        protocol: Some(RpcProtocol::Openrpc),
        transport: Some(RpcTransport::Http),
        server_name: None,
        server_version: None,
        methods,
        tools: Vec::new(),
    })
}

async fn probe_list_methods(client: &reqwest::Client, url: &str) -> Option<RpcProbeResult> {
    let body = json!({"jsonrpc": "2.0", "method": "system.listMethods", "id": 3});
    let resp = post_jsonrpc(client, url, body).await.ok()?;
    let result = resp.get("result")?;
    let arr = result.as_array()?;
    let methods: Vec<String> = arr
        .iter()
        .filter_map(|m| m.as_str().map(str::to_string))
        .collect();
    if methods.is_empty() {
        return None;
    }
    Some(RpcProbeResult {
        protocol: Some(RpcProtocol::Jsonrpc2),
        transport: Some(RpcTransport::Http),
        server_name: None,
        server_version: None,
        methods,
        tools: Vec::new(),
    })
}

async fn probe_jsonrpc_fingerprint(client: &reqwest::Client, url: &str) -> Option<RpcProbeResult> {
    let body = json!({"jsonrpc": "2.0", "method": "__axon_probe__", "id": 4});
    let resp = post_jsonrpc(client, url, body).await.ok()?;
    let code = resp.get("error")?.get("code")?.as_i64()?;
    // -32601 = Method not found per JSON-RPC 2.0 spec
    if code == -32601 {
        Some(RpcProbeResult {
            protocol: Some(RpcProtocol::Jsonrpc2),
            transport: Some(RpcTransport::Http),
            server_name: None,
            server_version: None,
            methods: Vec::new(),
            tools: Vec::new(),
        })
    } else {
        None
    }
}

async fn probe_sse_transport(client: &reqwest::Client, url: &str) -> Option<RpcProbeResult> {
    let resp = client
        .get(url)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .send()
        .await
        .ok()?;
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ct.contains("text/event-stream") {
        Some(RpcProbeResult {
            protocol: Some(RpcProtocol::Mcp),
            transport: Some(RpcTransport::Sse),
            server_name: None,
            server_version: None,
            methods: Vec::new(),
            tools: Vec::new(),
        })
    } else {
        None
    }
}

#[cfg(test)]
#[path = "probe_tests.rs"]
mod tests;
