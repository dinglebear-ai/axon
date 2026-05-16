//! Inline Chrome rendering via raw CDP WebSocket.
//!
//! Takes already-fetched HTML bytes and re-renders them inside a Chrome tab
//! using `Page.setContent()` — no second HTTP request. Used by the collector
//! to recover thin pages while the HTTP crawl is still in progress.

use crate::core::content::bytes_to_markdown;
use crate::core::logging::log_warn;
use futures_util::{SinkExt, StreamExt};
use spider_transformations::transformation::content::SelectorConfiguration;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Process-wide CDP message ID counter shared across all inline Chrome renders.
static REFETCH_CDP_ID: AtomicU64 = AtomicU64::new(1_000_000);

fn next_id() -> u64 {
    REFETCH_CDP_ID.fetch_add(1, Ordering::Relaxed)
}

/// Send a CDP command (browser-level or session-scoped) and wait for the matching response.
///
/// Pass `session_id = Some(sid)` for session-scoped commands; `None` for browser-level.
/// The `timeout` controls how long to wait for the response frame.
async fn send_cdp_cmd<Tx, Rx>(
    tx: &mut Tx,
    rx: &mut Rx,
    session_id: Option<&str>,
    method: &str,
    params: serde_json::Value,
    timeout: Duration,
) -> Result<serde_json::Value, String>
where
    Tx: SinkExt<
            tokio_tungstenite::tungstenite::Message,
            Error = tokio_tungstenite::tungstenite::Error,
        > + Unpin,
    Rx: StreamExt<
            Item = Result<
                tokio_tungstenite::tungstenite::Message,
                tokio_tungstenite::tungstenite::Error,
            >,
        > + Unpin,
{
    use tokio_tungstenite::tungstenite::Message;

    let id = next_id();
    let mut msg = serde_json::json!({ "id": id, "method": method, "params": params });
    if let Some(sid) = session_id {
        msg["sessionId"] = serde_json::Value::String(sid.to_string());
    }
    tx.send(Message::Text(msg.to_string().into()))
        .await
        .map_err(|e| format!("WS send failed for {method}: {e}"))?;

    let deadline = tokio::time::Instant::now() + timeout;
    // Safety bound on non-matching frames to prevent unbounded CPU drain if
    // Chrome sends excessive events (e.g. from an adversarial page).
    let mut skipped: u32 = 0;
    const MAX_SKIPPED_FRAMES: u32 = 10_000;
    loop {
        let frame = tokio::time::timeout_at(deadline, rx.next())
            .await
            .map_err(|_| format!("timeout waiting for CDP response to {method}"))?
            .ok_or_else(|| format!("WS closed waiting for {method}"))?
            .map_err(|e| format!("WS error waiting for {method}: {e}"))?;

        if let Message::Text(text) = frame {
            let v: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| format!("JSON parse error: {e}"))?;
            if v.get("id").and_then(|i| i.as_u64()) == Some(id) {
                if let Some(err) = v.get("error") {
                    return Err(format!("CDP error on {method}: {err}"));
                }
                return Ok(v.get("result").cloned().unwrap_or(serde_json::Value::Null));
            }
        }
        skipped += 1;
        if skipped > MAX_SKIPPED_FRAMES {
            return Err(format!(
                "CDP response loop exceeded {MAX_SKIPPED_FRAMES} non-matching frames for {method}"
            ));
        }
    }
}

/// Establish a Chrome CDP WebSocket connection and open a new tab.
///
/// Performs: TCP connect → WS handshake → `Target.createTarget` → `Target.attachToTarget`.
/// Returns `(ws_tx, ws_rx, target_id, session_id)` on success.
///
/// The caller is responsible for sending `Target.closeTarget` when done with the session.
async fn open_chrome_session(
    browser_ws_url: &str,
    page_url: &str,
    cmd_timeout: Duration,
) -> Result<
    (
        impl SinkExt<
            tokio_tungstenite::tungstenite::Message,
            Error = tokio_tungstenite::tungstenite::Error,
        > + Unpin,
        impl StreamExt<
            Item = Result<
                tokio_tungstenite::tungstenite::Message,
                tokio_tungstenite::tungstenite::Error,
            >,
        > + Unpin,
        String,
        String,
    ),
    String,
> {
    let parsed = reqwest::Url::parse(browser_ws_url)
        .map_err(|e| format!("invalid Chrome WS URL {browser_ws_url}: {e}"))?;
    let host = parsed.host_str().unwrap_or("127.0.0.1");
    let port = parsed.port().unwrap_or(9222);
    let addr = format!("{host}:{port}");

    // Normalize wss:// to ws:// for loopback connections — Chrome on localhost
    // never serves TLS on its CDP endpoint.
    let is_loopback = host == "127.0.0.1" || host == "localhost" || host == "::1";
    let effective_ws_url = if parsed.scheme() == "wss" && is_loopback {
        browser_ws_url.replacen("wss://", "ws://", 1)
    } else {
        browser_ws_url.to_string()
    };

    // connect_async handles both ws:// and wss:// (via rustls-tls-native-roots feature).
    let (stream, _resp) = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::connect_async(&effective_ws_url),
    )
    .await
    .map_err(|_| format!("timeout during WS handshake with Chrome at {addr}"))?
    .map_err(|e| format!("WS handshake failed with Chrome at {addr}: {e}"))?;

    let (mut ws_tx, mut ws_rx) = stream.split();

    let target_id = send_cdp_cmd(
        &mut ws_tx,
        &mut ws_rx,
        None,
        "Target.createTarget",
        serde_json::json!({ "url": "about:blank" }),
        cmd_timeout,
    )
    .await
    .and_then(|v| {
        v.get("targetId")
            .and_then(|t| t.as_str())
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("empty targetId for {page_url}"))
    })?;

    let session_id = send_cdp_cmd(
        &mut ws_tx,
        &mut ws_rx,
        None,
        "Target.attachToTarget",
        serde_json::json!({ "targetId": target_id, "flatten": true }),
        cmd_timeout,
    )
    .await
    .and_then(|v| {
        v.get("sessionId")
            .and_then(|s| s.as_str())
            .filter(|sid| !sid.is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("empty sessionId for {page_url}"))
    })?;

    Ok((ws_tx, ws_rx, target_id, session_id))
}

/// Enable Page events, inject HTML into the tab, and capture `outerHTML` after rendering.
///
/// Returns the rendered outer HTML string on success, or an error string on any CDP failure.
/// The caller owns tab cleanup (`Target.closeTarget`) regardless of this function's outcome.
async fn inject_and_render<Tx, Rx>(
    tx: &mut Tx,
    rx: &mut Rx,
    session_id: &str,
    html: &str,
    page_url: &str,
    cmd_timeout: Duration,
) -> Result<String, String>
where
    Tx: SinkExt<
            tokio_tungstenite::tungstenite::Message,
            Error = tokio_tungstenite::tungstenite::Error,
        > + Unpin,
    Rx: StreamExt<
            Item = Result<
                tokio_tungstenite::tungstenite::Message,
                tokio_tungstenite::tungstenite::Error,
            >,
        > + Unpin,
{
    // Enable Page events so we can wait for load.
    send_cdp_cmd(
        tx,
        rx,
        Some(session_id),
        "Page.enable",
        serde_json::json!({}),
        cmd_timeout,
    )
    .await
    .map_err(|e| format!("Page.enable failed for {page_url}: {e}"))?;

    // Inject the already-fetched HTML via Runtime.evaluate + document.write.
    // `Page.setContent` is not available in all Chrome CDP proxy configurations
    // (returns -32601 "method not found"). The document.write approach is
    // universally supported: open a blank document, write the HTML, then close.
    //
    // The HTML string is JSON-escaped by serde_json so it survives embedding
    // inside a JS template literal — no second encoding needed.
    let escaped_html = serde_json::to_string(html)
        .map_err(|e| format!("HTML JSON escaping failed for {page_url}: {e}"))?;
    let inject_expr = format!("document.open(); document.write({escaped_html}); document.close();");
    send_cdp_cmd(
        tx,
        rx,
        Some(session_id),
        "Runtime.evaluate",
        serde_json::json!({
            "expression": inject_expr,
            "returnByValue": false,
        }),
        cmd_timeout,
    )
    .await
    .map_err(|e| format!("HTML inject failed for {page_url}: {e}"))?;

    // Brief settle time for any synchronous JS in the injected HTML to run.
    // document.write is synchronous so no load event fires — wait a fixed interval.
    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = send_cdp_cmd(
        tx,
        rx,
        Some(session_id),
        "Runtime.evaluate",
        serde_json::json!({
            "expression": "document.documentElement.outerHTML",
            "returnByValue": true,
        }),
        cmd_timeout,
    )
    .await
    .map_err(|e| format!("outerHTML evaluate failed for {page_url}: {e}"))?;

    result
        .get("result")
        .and_then(|r| r.get("value"))
        .and_then(|val| val.as_str())
        .map(str::to_string)
        .ok_or_else(|| format!("outerHTML result missing value field for {page_url}"))
}

/// Render already-fetched HTML bytes via Chrome CDP and
/// return the resulting markdown.
///
/// This avoids a second network round-trip: the HTML we already received from
/// the HTTP crawl is injected directly into a Chrome tab, JavaScript executes,
/// and we capture the rendered DOM.
///
/// `chrome_ws_url` may be an HTTP management endpoint (e.g. `http://host:6000`)
/// or an already-resolved `ws://` WebSocket URL. This function resolves it to
/// a browser-level WebSocket URL before connecting.
///
/// Returns `None` when:
/// - Chrome URL cannot be resolved to a WebSocket endpoint
/// - Chrome is unreachable
/// - CDP commands fail (tab open, setContent, evaluate)
/// - Rendered markdown is still below `min_chars` (still thin)
pub(super) async fn render_html_with_chrome(
    chrome_ws_url: &str,
    html_bytes: Vec<u8>,
    page_url: &str,
    min_chars: usize,
    timeout_secs: u64,
    selector_config: Option<SelectorConfiguration>,
) -> Option<String> {
    let html = match String::from_utf8(html_bytes) {
        Ok(s) => s,
        Err(_) => {
            // Non-UTF-8 HTML — skip rather than risk garbage content.
            return None;
        }
    };

    let resolved_ws_url = resolve_inline_chrome_ws_url(chrome_ws_url).await?;

    // Use the caller-supplied timeout for CDP commands; cap at a sane maximum
    // so a misconfigured value cannot hang indefinitely.
    let cmd_timeout = Duration::from_secs(timeout_secs.clamp(5, 120));

    let (mut ws_tx, mut ws_rx, target_id, session_id) =
        match open_chrome_session(&resolved_ws_url, page_url, cmd_timeout).await {
            Ok(session) => session,
            Err(e) => {
                log_warn(&format!(
                    "thin_refetch: Chrome session failed for {page_url}: {e}"
                ));
                return None;
            }
        };

    let render_result = inject_and_render(
        &mut ws_tx,
        &mut ws_rx,
        &session_id,
        &html,
        page_url,
        cmd_timeout,
    )
    .await;

    close_cdp_target(&mut ws_tx, &mut ws_rx, &target_id).await;

    let rendered_html = match render_result {
        Ok(html_out) => html_out,
        Err(e) => {
            // Log the failure before falling back to the original HTTP HTML so
            // operators can see that Chrome rendering did not succeed.
            log_warn(&format!(
                "thin_refetch: Chrome render failed for {page_url}: {e}; falling back to HTTP HTML"
            ));
            html
        }
    };

    markdown_if_not_thin(&rendered_html, min_chars, selector_config.as_ref())
}

fn markdown_if_not_thin(
    rendered_html: &str,
    min_chars: usize,
    selector_config: Option<&SelectorConfiguration>,
) -> Option<String> {
    let trimmed = bytes_to_markdown(rendered_html.as_bytes(), selector_config);
    if trimmed.len() < min_chars {
        None
    } else {
        Some(trimmed)
    }
}

async fn resolve_inline_chrome_ws_url(chrome_ws_url: &str) -> Option<String> {
    match tokio::time::timeout(
        Duration::from_secs(5),
        super::runtime::resolve_cdp_ws_url(chrome_ws_url),
    )
    .await
    {
        Ok(Some(url)) => Some(url),
        Ok(None) => {
            log_warn(&format!(
                "thin_refetch: Chrome URL {chrome_ws_url} did not resolve to a ws:// endpoint (inside Docker?)"
            ));
            None
        }
        Err(_) => {
            log_warn(&format!(
                "thin_refetch: timeout resolving Chrome WS URL from {chrome_ws_url}"
            ));
            None
        }
    }
}

async fn close_cdp_target<Tx, Rx>(tx: &mut Tx, rx: &mut Rx, target_id: &str)
where
    Tx: SinkExt<
            tokio_tungstenite::tungstenite::Message,
            Error = tokio_tungstenite::tungstenite::Error,
        > + Unpin,
    Rx: StreamExt<
            Item = Result<
                tokio_tungstenite::tungstenite::Message,
                tokio_tungstenite::tungstenite::Error,
            >,
        > + Unpin,
{
    let _ = send_cdp_cmd(
        tx,
        rx,
        None,
        "Target.closeTarget",
        serde_json::json!({ "targetId": target_id }),
        Duration::from_secs(5),
    )
    .await;
}

#[cfg(test)]
#[path = "cdp_render_tests.rs"]
mod tests;
