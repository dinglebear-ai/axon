use super::{EndpointError, timeout_secs, validate_url_with_dns_timeout};
use crate::core::config::Config;
use crate::core::http::{axon_ua, build_client};
use crate::services::types::{EndpointReport, RpcProbeResult};
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};
use std::sync::LazyLock;
use tokio::sync::Semaphore;

const PROBE_TIMEOUT_SECS: u64 = 3;
const MAX_PROBE_ENDPOINTS: usize = 20;
const PROBE_CONCURRENCY: usize = 4;

static PROBE_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| {
    let cap = std::env::var("AXON_ENDPOINT_PROBE_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(4)
        .max(1);
    Semaphore::new(cap)
});

pub(super) async fn probe_rpc_endpoints(cfg: &Config, report: &mut EndpointReport) {
    let _permit = match PROBE_SEMAPHORE.acquire().await {
        Ok(p) => p,
        Err(err) => {
            report
                .warnings
                .push(format!("rpc probe semaphore closed: {err}"));
            return;
        }
    };
    let client = match build_client(timeout_secs(cfg, PROBE_TIMEOUT_SECS), Some(axon_ua())) {
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
            async move { (idx, probe_one(&client, &url).await) }
        })
        .buffer_unordered(PROBE_CONCURRENCY)
        .collect()
        .await;

    for (idx, probe_result) in results {
        if let Some(rpc) = probe_result {
            if let Some(endpoint) = report.endpoints.get_mut(idx) {
                endpoint.rpc_probe = Some(rpc);
            }
        }
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

async fn post_jsonrpc(
    client: &reqwest::Client,
    url: &str,
    body: Value,
) -> Result<Value, EndpointError> {
    let resp = client
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .json(&body)
        .send()
        .await?;
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !ct.contains("json") {
        return Err("response is not JSON".into());
    }
    let text = resp.text().await?;
    serde_json::from_str(&text).map_err(Into::into)
}

async fn probe_mcp(client: &reqwest::Client, url: &str) -> Option<RpcProbeResult> {
    let body = json!({
        "jsonrpc": "2.0",
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "axon-probe", "version": "1.0" }
        },
        "id": 1
    });
    let resp = post_jsonrpc(client, url, body).await.ok()?;
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
    let tools = probe_mcp_tools(client, url).await.unwrap_or_default();
    Some(RpcProbeResult {
        protocol: Some("mcp".to_string()),
        transport: Some("http".to_string()),
        server_name,
        server_version,
        methods: Vec::new(),
        tools,
        error: None,
    })
}

async fn probe_mcp_tools(client: &reqwest::Client, url: &str) -> Option<Vec<String>> {
    let body = json!({"jsonrpc": "2.0", "method": "tools/list", "id": 2});
    let resp = post_jsonrpc(client, url, body).await.ok()?;
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
        protocol: Some("openrpc".to_string()),
        transport: Some("http".to_string()),
        server_name: None,
        server_version: None,
        methods,
        tools: Vec::new(),
        error: None,
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
        protocol: Some("jsonrpc2".to_string()),
        transport: Some("http".to_string()),
        server_name: None,
        server_version: None,
        methods,
        tools: Vec::new(),
        error: None,
    })
}

async fn probe_jsonrpc_fingerprint(client: &reqwest::Client, url: &str) -> Option<RpcProbeResult> {
    let body = json!({"jsonrpc": "2.0", "method": "__axon_probe__", "id": 4});
    let resp = post_jsonrpc(client, url, body).await.ok()?;
    let code = resp.get("error")?.get("code")?.as_i64()?;
    // -32601 = Method not found per JSON-RPC 2.0 spec
    if code == -32601 {
        Some(RpcProbeResult {
            protocol: Some("jsonrpc2".to_string()),
            transport: Some("http".to_string()),
            server_name: None,
            server_version: None,
            methods: Vec::new(),
            tools: Vec::new(),
            error: None,
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
            protocol: Some("mcp".to_string()),
            transport: Some("sse".to_string()),
            server_name: None,
            server_version: None,
            methods: Vec::new(),
            tools: Vec::new(),
            error: None,
        })
    } else {
        None
    }
}
