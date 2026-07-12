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
        axon_adapters::web_engine::engine::resolve_cdp_ws_url(remote_url),
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
        None,
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
        None,
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
        None,
    )
    .await;
    capture_result
}

/// Enable Network, Page, and Fetch CDP domains for a session.
/// Fetch.enable with `requestStage: Request` intercepts all requests before
/// dispatch so the event loop can SSRF-check and block unsafe targets.
async fn enable_capture_domains<Tx, Rx>(
    tx: &mut Tx,
    rx: &mut Rx,
    session_id: &str,
) -> Result<(), String>
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
        None,
    )
    .await?;
    send_capture_cdp_cmd(
        tx,
        rx,
        Some(session_id),
        "Page.enable",
        serde_json::json!({}),
        cmd_timeout,
        None,
    )
    .await?;
    // Intercept every request BEFORE Chrome dispatches it (satisfies bead w2wf.5).
    send_capture_cdp_cmd(
        tx,
        rx,
        Some(session_id),
        "Fetch.enable",
        serde_json::json!({ "patterns": [{ "urlPattern": "*", "requestStage": "Request" }] }),
        cmd_timeout,
        None,
    )
    .await?;
    Ok(())
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
    enable_capture_domains(tx, rx, session_id).await?;
    let cmd_timeout = Duration::from_secs(CAPTURE_CDP_TIMEOUT_SECS);
    let mut captured = Vec::new();
    let mut last_network_event = tokio::time::Instant::now();
    let mut page_loaded = false;
    {
        // Fetch.enable (requestStage: Request) intercepts the navigation request itself,
        // so Page.navigate's response will never arrive unless we reply to
        // Fetch.requestPaused events while waiting. The generic send_capture_cdp_cmd
        // callback is FnMut(&Value) with no tx access, so we inline the navigate wait.
        let nav_id = CAPTURE_CDP_ID.fetch_add(1, Ordering::Relaxed);
        let mut nav_msg = serde_json::json!({ "id": nav_id, "method": "Page.navigate", "params": { "url": page_url } });
        nav_msg["sessionId"] = serde_json::Value::String(session_id.to_string());
        tx.send(Message::Text(nav_msg.to_string().into()))
            .await
            .map_err(|err| format!("Chrome WebSocket send failed for Page.navigate: {err}"))?;
        let nav_deadline = tokio::time::Instant::now() + cmd_timeout;
        loop {
            let frame = tokio::time::timeout_at(nav_deadline, rx.next())
                .await
                .map_err(|_| "timeout waiting for Chrome response to Page.navigate".to_string())?
                .ok_or_else(|| "Chrome WebSocket closed waiting for Page.navigate".to_string())?
                .map_err(|err| {
                    format!("Chrome WebSocket read failed waiting for Page.navigate: {err}")
                })?;
            let Message::Text(text) = frame else {
                continue;
            };
            let value: serde_json::Value = serde_json::from_str(&text)
                .map_err(|err| format!("CDP response JSON parse failed: {err}"))?;
            // Reply to any intercepted requests so Chrome doesn't stall.
            if value.get("method").and_then(|m| m.as_str()) == Some("Fetch.requestPaused")
                && value.get("sessionId").and_then(|id| id.as_str()) == Some(session_id)
            {
                send_fetch_intercept_reply(tx, session_id, &value).await;
                continue;
            }
            // Capture early network/page events before the navigate ack arrives.
            if value.get("sessionId").and_then(|id| id.as_str()) == Some(session_id) {
                match value.get("method").and_then(|m| m.as_str()) {
                    Some("Network.requestWillBeSent") => {
                        if captured.len() < max_requests
                            && let Some(request) = captured_request_from_event(&value)
                        {
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
            if value.get("id").and_then(|v| v.as_u64()) != Some(nav_id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                return Err(format!("CDP error on Page.navigate: {error}"));
            }
            break;
        }
    }

    let deadline = tokio::time::Instant::now()
        + Duration::from_secs(network_idle_secs.clamp(5, 60) + CAPTURE_CDP_TIMEOUT_SECS);

    while tokio::time::Instant::now() < deadline && captured.len() < max_requests {
        let idle_deadline = last_network_event + Duration::from_millis(CAPTURE_IDLE_MS);
        if page_loaded && tokio::time::Instant::now() >= idle_deadline {
            break;
        }
        let next_deadline = if page_loaded {
            deadline.min(idle_deadline)
        } else {
            deadline
        };
        let frame = match tokio::time::timeout_at(next_deadline, rx.next()).await {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(_) if page_loaded => break,
            Err(_) => continue,
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
            Some("Fetch.requestPaused") => {
                // Pre-dispatch interception: validate URL before Chrome sends the request.
                // IMPORTANT: Fetch.continueRequest and Fetch.failRequest are
                // fire-and-forget by CDP design — no response id is returned.
                // Do NOT use send_capture_cdp_cmd (which blocks waiting for a
                // response) — that would stall the event loop and cause Chrome to
                // timeout the paused request. Send directly through tx instead.
                send_fetch_intercept_reply(tx, session_id, &value).await;
            }
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

/// Handle a `Fetch.requestPaused` CDP event: validate the URL and either
/// continue or fail the request. Fire-and-forget — no response is expected.
async fn send_fetch_intercept_reply<Tx>(tx: &mut Tx, session_id: &str, event: &serde_json::Value)
where
    Tx: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    let params = event.get("params").cloned().unwrap_or_default();
    let request_id = params
        .get("requestId")
        .and_then(|id| id.as_str())
        .unwrap_or_default()
        .to_string();
    let url = params
        .get("request")
        .and_then(|r| r.get("url"))
        .and_then(|u| u.as_str())
        .unwrap_or_default()
        .to_string();
    // SSRF check: block private/loopback/link-local targets before dispatch.
    let is_blocked = validate_url_with_dns_timeout(&url).await.is_err();
    let id = CAPTURE_CDP_ID.fetch_add(1, Ordering::Relaxed);
    let (cdp_method, cdp_params) = if is_blocked {
        (
            "Fetch.failRequest",
            serde_json::json!({ "requestId": request_id, "errorReason": "AccessDenied" }),
        )
    } else {
        (
            "Fetch.continueRequest",
            serde_json::json!({ "requestId": request_id }),
        )
    };
    let mut msg = serde_json::json!({ "id": id, "method": cdp_method, "params": cdp_params });
    msg["sessionId"] = serde_json::Value::String(session_id.to_string());
    // Fire-and-forget: ignore send errors (page may already be navigated away).
    let _ = tx.send(Message::Text(msg.to_string().into())).await;
}

fn captured_request_from_event(value: &serde_json::Value) -> Option<CapturedRequest> {
    let request = value.get("params")?.get("request")?;
    let url = request.get("url")?.as_str()?;
    if !(url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("ws://")
        || url.starts_with("wss://"))
    {
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
    mut on_event: Option<&mut (dyn FnMut(&serde_json::Value) + Send)>,
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
            if let Some(handler) = on_event.as_deref_mut() {
                handler(&value);
            }
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
