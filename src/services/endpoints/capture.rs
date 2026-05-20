use super::{CapturedRequest, validate_url_with_dns_timeout};
use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

const CAPTURE_IDLE_MS: u64 = 750;
const CAPTURE_CDP_TIMEOUT_SECS: u64 = 5;
static CAPTURE_CDP_ID: AtomicU64 = AtomicU64::new(2_000_000);

pub(super) async fn capture_requests_with_chrome(
    remote_url: &str,
    page_url: &str,
    max_requests: usize,
    network_idle_secs: u64,
) -> Result<Vec<CapturedRequest>, String> {
    validate_url_with_dns_timeout(page_url)
        .await
        .map_err(|err| format!("capture target rejected: {err}"))?;
    let resolved_ws_url = tokio::time::timeout(
        Duration::from_secs(CAPTURE_CDP_TIMEOUT_SECS),
        crate::crawl::engine::resolve_cdp_ws_url(remote_url),
    )
    .await
    .map_err(|_| "timeout resolving Chrome CDP WebSocket URL".to_string())?
    .ok_or_else(|| format!("Chrome URL {remote_url} did not resolve to a ws:// endpoint"))?;

    let (stream, _) = tokio::time::timeout(
        Duration::from_secs(CAPTURE_CDP_TIMEOUT_SECS),
        tokio_tungstenite::connect_async(&resolved_ws_url),
    )
    .await
    .map_err(|_| format!("timeout connecting to Chrome at {resolved_ws_url}"))?
    .map_err(|err| format!("Chrome WebSocket connection failed: {err}"))?;
    let (mut tx, mut rx) = stream.split();
    let cmd_timeout = Duration::from_secs(CAPTURE_CDP_TIMEOUT_SECS);

    let target_id = send_capture_cdp_cmd(
        &mut tx,
        &mut rx,
        None,
        "Target.createTarget",
        serde_json::json!({ "url": "about:blank" }),
        cmd_timeout,
    )
    .await?
    .get("targetId")
    .and_then(|value| value.as_str())
    .filter(|value| !value.is_empty())
    .map(str::to_string)
    .ok_or_else(|| "Chrome Target.createTarget returned no targetId".to_string())?;

    let session_id = send_capture_cdp_cmd(
        &mut tx,
        &mut rx,
        None,
        "Target.attachToTarget",
        serde_json::json!({ "targetId": target_id, "flatten": true }),
        cmd_timeout,
    )
    .await?
    .get("sessionId")
    .and_then(|value| value.as_str())
    .filter(|value| !value.is_empty())
    .map(str::to_string)
    .ok_or_else(|| "Chrome Target.attachToTarget returned no sessionId".to_string())?;

    let capture_result = capture_session_requests(
        &mut tx,
        &mut rx,
        &session_id,
        page_url,
        max_requests,
        network_idle_secs,
    )
    .await;
    let _ = send_capture_cdp_cmd(
        &mut tx,
        &mut rx,
        None,
        "Target.closeTarget",
        serde_json::json!({ "targetId": target_id }),
        cmd_timeout,
    )
    .await;
    capture_result
}

async fn capture_session_requests<Tx, Rx>(
    tx: &mut Tx,
    rx: &mut Rx,
    session_id: &str,
    page_url: &str,
    max_requests: usize,
    network_idle_secs: u64,
) -> Result<Vec<CapturedRequest>, String>
where
    Tx: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    Rx: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let cmd_timeout = Duration::from_secs(CAPTURE_CDP_TIMEOUT_SECS);
    send_capture_cdp_cmd(
        tx,
        rx,
        Some(session_id),
        "Network.enable",
        serde_json::json!({}),
        cmd_timeout,
    )
    .await?;
    send_capture_cdp_cmd(
        tx,
        rx,
        Some(session_id),
        "Page.enable",
        serde_json::json!({}),
        cmd_timeout,
    )
    .await?;
    send_capture_cdp_cmd(
        tx,
        rx,
        Some(session_id),
        "Page.navigate",
        serde_json::json!({ "url": page_url }),
        cmd_timeout,
    )
    .await?;

    let deadline = tokio::time::Instant::now()
        + Duration::from_secs(network_idle_secs.clamp(5, 60) + CAPTURE_CDP_TIMEOUT_SECS);
    let mut last_network_event = tokio::time::Instant::now();
    let mut page_loaded = false;
    let mut captured = Vec::new();

    while tokio::time::Instant::now() < deadline && captured.len() < max_requests {
        let idle_deadline = last_network_event + Duration::from_millis(CAPTURE_IDLE_MS);
        if page_loaded && tokio::time::Instant::now() >= idle_deadline {
            break;
        }
        let next_deadline = deadline.min(idle_deadline);
        let Some(frame) = tokio::time::timeout_at(next_deadline, rx.next())
            .await
            .ok()
            .flatten()
        else {
            if page_loaded {
                break;
            }
            continue;
        };
        let frame = frame.map_err(|err| format!("Chrome WebSocket read failed: {err}"))?;
        let Message::Text(text) = frame else {
            continue;
        };
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|err| format!("CDP event JSON parse failed: {err}"))?;
        if value.get("sessionId").and_then(|id| id.as_str()) != Some(session_id) {
            continue;
        }
        match value.get("method").and_then(|method| method.as_str()) {
            Some("Network.requestWillBeSent") => {
                if let Some(request) = captured_request_from_event(&value) {
                    captured.push(request);
                    last_network_event = tokio::time::Instant::now();
                }
            }
            Some("Page.loadEventFired") => {
                page_loaded = true;
            }
            _ => {}
        }
    }
    Ok(captured)
}

fn captured_request_from_event(value: &serde_json::Value) -> Option<CapturedRequest> {
    let request = value.get("params")?.get("request")?;
    let url = request.get("url")?.as_str()?;
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    Some(CapturedRequest {
        url: url.to_string(),
        method: request
            .get("method")
            .and_then(|method| method.as_str())
            .map(str::to_string),
    })
}

async fn send_capture_cdp_cmd<Tx, Rx>(
    tx: &mut Tx,
    rx: &mut Rx,
    session_id: Option<&str>,
    method: &str,
    params: serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String>
where
    Tx: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
    Rx: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let id = CAPTURE_CDP_ID.fetch_add(1, Ordering::Relaxed);
    let mut message = serde_json::json!({ "id": id, "method": method, "params": params });
    if let Some(session_id) = session_id {
        message["sessionId"] = serde_json::Value::String(session_id.to_string());
    }
    tx.send(Message::Text(message.to_string().into()))
        .await
        .map_err(|err| format!("Chrome WebSocket send failed for {method}: {err}"))?;

    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let frame = tokio::time::timeout_at(deadline, rx.next())
            .await
            .map_err(|_| format!("timeout waiting for Chrome response to {method}"))?
            .ok_or_else(|| format!("Chrome WebSocket closed waiting for {method}"))?
            .map_err(|err| format!("Chrome WebSocket read failed waiting for {method}: {err}"))?;
        let Message::Text(text) = frame else {
            continue;
        };
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|err| format!("CDP response JSON parse failed: {err}"))?;
        if value.get("id").and_then(|value| value.as_u64()) != Some(id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            return Err(format!("CDP error on {method}: {error}"));
        }
        return Ok(value
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null));
    }
}
