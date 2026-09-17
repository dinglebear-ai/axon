//! Chrome DevTools Protocol discovery URL construction.

use futures_util::{SinkExt, StreamExt};
use spider::url::Url;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};

const SPIDER_CDP_RELAY_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const SPIDER_CDP_RELAY_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const SPIDER_CDP_RELAY_MAX_CONNECTIONS: usize = 16;

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// Return whether a discovered CDP socket is an authorized target for the
/// configured Chrome endpoint.
///
/// Exact origins are always accepted. A non-TLS loopback endpoint may rewrite
/// between loopback names or ports because containerized Chrome commonly
/// advertises its internal DevTools listener. Remote endpoints may never
/// redirect the WebSocket handshake to another origin.
pub fn cdp_websocket_origin_is_authorized(remote_url: &str, websocket_url: &str) -> bool {
    let Ok(remote) = Url::parse(remote_url) else {
        return false;
    };
    let Ok(websocket) = Url::parse(websocket_url) else {
        return false;
    };
    if !remote.username().is_empty()
        || remote.password().is_some()
        || !websocket.username().is_empty()
        || websocket.password().is_some()
    {
        return false;
    }
    let Some(remote_host) = remote.host_str() else {
        return false;
    };
    let Some(websocket_host) = websocket.host_str() else {
        return false;
    };
    let expected_ws_scheme = match remote.scheme() {
        "http" | "ws" => "ws",
        "https" | "wss" => "wss",
        _ => return false,
    };
    if websocket.scheme() == expected_ws_scheme
        && remote_host == websocket_host
        && remote.port_or_known_default() == websocket.port_or_known_default()
    {
        return true;
    }
    expected_ws_scheme == "ws"
        && websocket.scheme() == "ws"
        && is_loopback_host(remote_host)
        && is_loopback_host(websocket_host)
}

/// Build a bearer authorization header for a CDP WebSocket handshake only when
/// the discovered socket has the same origin as the configured Chrome URL.
///
/// HTTP and WS are equivalent transport pairs, as are HTTPS and WSS. Host and
/// effective port must match exactly so discovery cannot redirect credentials
/// to another host, port, downgraded connection, or a rewritten loopback URL.
pub fn cdp_websocket_bearer_header(
    remote_url: &str,
    websocket_url: &str,
    token: &str,
) -> Option<reqwest::header::HeaderValue> {
    if token.is_empty() {
        return None;
    }
    if !cdp_websocket_origin_is_authorized(remote_url, websocket_url) {
        return None;
    }
    let remote = Url::parse(remote_url).ok()?;
    let websocket = Url::parse(websocket_url).ok()?;
    if !remote.username().is_empty()
        || remote.password().is_some()
        || !websocket.username().is_empty()
        || websocket.password().is_some()
    {
        return None;
    }
    let expected_ws_scheme = match remote.scheme() {
        "http" | "ws" => "ws",
        "https" | "wss" => "wss",
        _ => return None,
    };
    if websocket.scheme() != expected_ws_scheme
        || remote.host_str() != websocket.host_str()
        || remote.port_or_known_default() != websocket.port_or_known_default()
    {
        return None;
    }
    let mut header = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}")).ok()?;
    header.set_sensitive(true);
    Some(header)
}

/// Return a CDP URL that Spider can connect to while preserving bearer
/// authentication on the upstream WebSocket handshake.
///
/// Spider's Chrome client accepts only a URL, so it cannot attach the bearer
/// header required by authenticated CDP gateways. For those endpoints this
/// starts a bounded loopback WebSocket relay with an unguessable path. The
/// relay validates that path and injects the bearer header only into the
/// configured endpoint's exact-origin upstream handshake. The token is never
/// placed in a URL, child-process environment, or log message.
#[allow(clippy::result_large_err)] // tungstenite's handshake callback fixes this Result error type.
pub async fn cdp_spider_connection_url(
    remote_url: &str,
    websocket_url: &str,
    bearer_token: Option<&str>,
) -> Result<String, String> {
    if !cdp_websocket_origin_is_authorized(remote_url, websocket_url) {
        return Err("Chrome discovery returned an unauthorized WebSocket origin".to_string());
    }
    let Some(token) = bearer_token.filter(|token| !token.is_empty()) else {
        return Ok(websocket_url.to_string());
    };
    let authorization = cdp_websocket_bearer_header(remote_url, websocket_url, token)
        .ok_or_else(|| "authenticated Chrome discovery returned a different origin".to_string())?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|error| format!("failed to bind authenticated Chrome relay: {error}"))?;
    let address = listener
        .local_addr()
        .map_err(|error| format!("failed to inspect authenticated Chrome relay: {error}"))?;
    let relay_path = format!("/{}", uuid::Uuid::new_v4());
    let expected_path = relay_path.clone();
    let upstream_url = websocket_url.to_string();

    tokio::spawn(async move {
        let expires_at = tokio::time::Instant::now() + SPIDER_CDP_RELAY_IDLE_TIMEOUT;
        for _ in 0..SPIDER_CDP_RELAY_MAX_CONNECTIONS {
            let accepted = tokio::time::timeout_at(expires_at, listener.accept()).await;
            let Ok(Ok((stream, _))) = accepted else {
                break;
            };
            let expected_path = expected_path.clone();
            let callback = move |request: &Request, response: Response| {
                if request.uri().path() == expected_path {
                    Ok(response)
                } else {
                    let mut rejection = ErrorResponse::new(Some("forbidden".to_string()));
                    *rejection.status_mut() = reqwest::StatusCode::FORBIDDEN;
                    Err(rejection)
                }
            };
            let handshake_deadline = std::cmp::min(
                expires_at,
                tokio::time::Instant::now() + SPIDER_CDP_RELAY_HANDSHAKE_TIMEOUT,
            );
            let downstream = tokio::time::timeout_at(
                handshake_deadline,
                tokio_tungstenite::accept_hdr_async(stream, callback),
            )
            .await;
            let Ok(Ok(downstream)) = downstream else {
                continue;
            };
            let Ok(mut upstream_request) = upstream_url.as_str().into_client_request() else {
                break;
            };
            upstream_request
                .headers_mut()
                .insert(reqwest::header::AUTHORIZATION, authorization.clone());
            let connect_deadline = std::cmp::min(
                expires_at,
                tokio::time::Instant::now() + SPIDER_CDP_RELAY_HANDSHAKE_TIMEOUT,
            );
            let upstream = tokio::time::timeout_at(
                connect_deadline,
                tokio_tungstenite::connect_async(upstream_request),
            )
            .await;
            let Ok(Ok((upstream, _))) = upstream else {
                continue;
            };

            // One Spider browser session owns this relay. Stop accepting new
            // connections as soon as that session is established so the
            // bearer credential and listener do not outlive it.
            drop(listener);
            relay_websocket_messages(downstream, upstream).await;
            return;
        }
    });

    Ok(format!("ws://{address}{relay_path}"))
}

async fn relay_websocket_messages<S>(
    downstream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    upstream: tokio_tungstenite::WebSocketStream<S>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut downstream_tx, mut downstream_rx) = downstream.split();
    let (mut upstream_tx, mut upstream_rx) = upstream.split();
    tokio::select! {
        _ = async {
            while let Some(Ok(message)) = downstream_rx.next().await {
                if upstream_tx.send(message).await.is_err() {
                    break;
                }
            }
        } => {}
        _ = async {
            while let Some(Ok(message)) = upstream_rx.next().await {
                if downstream_tx.send(message).await.is_err() {
                    break;
                }
            }
        } => {}
    }
}

/// Build the CDP `/json/version` discovery URL from a Chrome remote URL.
///
/// Handles `ws://` / `wss://` -> `http://` / `https://` conversion (reqwest cannot
/// make requests to `ws://` scheme URLs) and appends `/json/version` when the path
/// is absent or root.  Returns `None` if the URL cannot be parsed or uses an
/// unsupported scheme (`ftp://`, `file://`, etc.).
///
/// # Safety (SSRF)
///
/// This function performs **no SSRF validation** on the input URL. It trusts
/// that `remote_url` originates from a trusted configuration source (e.g.
/// `AXON_CHROME_REMOTE_URL` environment variable). Do **not** pass
/// user-controlled or untrusted URLs without first validating them through
/// [`super::ssrf::validate_url`].
pub fn cdp_discovery_url(remote_url: &str) -> Option<String> {
    let parsed = Url::parse(remote_url).ok()?;
    let http_scheme = match parsed.scheme() {
        "ws" | "http" => "http",
        "wss" | "https" => "https",
        _ => return None,
    };
    let host = parsed.host_str()?;
    let port = parsed.port_or_known_default()?;
    let path = parsed.path();
    let path = if path == "/" || path.is_empty() {
        "/json/version"
    } else {
        path
    };
    Some(format!("{http_scheme}://{host}:{port}{path}"))
}
