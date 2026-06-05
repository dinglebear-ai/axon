use crate::core::config::Config;
use crate::core::http::parse_custom_headers;
use crate::core::http::{axon_ua, cdp_discovery_url, ssrf_blacklist_compact_strings, validate_url};
use crate::crawl::engine::resolve_cdp_ws_url;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use futures_util::{SinkExt, StreamExt};
use spider::configuration::Viewport;
use spider::features::chrome_common::RequestInterceptConfiguration;
use spider::features::chrome_common::{ScreenShotConfig, ScreenshotParams};
use spider::website::Website;
use std::error::Error;
use std::sync::atomic::{AtomicU64, Ordering};

static SCREENSHOT_CDP_ID: AtomicU64 = AtomicU64::new(2_000_000);

/// Capture a screenshot using Spider's Chrome screenshot support with explicit
/// viewport and full_page parameters.
///
/// Called by both the CLI handler and the services layer so capture logic stays
/// in one place.
pub(crate) async fn spider_screenshot_with_options(
    cfg: &Config,
    url: &str,
    width: u32,
    height: u32,
    full_page: bool,
) -> Result<Vec<u8>, Box<dyn Error>> {
    // Validate the URL through the SSRF guard before passing it to Chrome.
    // Without this, an attacker-controlled URL could reach internal services
    // via the Chrome rendering path.
    validate_url(url).map_err(|e| format!("screenshot blocked — SSRF guard: {e}"))?;

    let remote_url = cfg
        .chrome_remote_url
        .as_deref()
        .ok_or("screenshot requires Chrome — set AXON_CHROME_REMOTE_URL")?;

    // Resolve the Chrome connection URL using the same logic as the crawl
    // engine: try the CDP WS discovery first, fall back to the discovery
    // URL or the raw remote URL.
    let chrome_url = match resolve_cdp_ws_url(remote_url).await {
        Some(ws_url) => ws_url,
        None => cdp_discovery_url(remote_url).unwrap_or_else(|| remote_url.to_string()),
    };

    match capture_screenshot_via_cdp(cfg, url, &chrome_url, width, height, full_page).await {
        Ok(bytes) => return Ok(bytes),
        Err(err) => {
            crate::core::logging::log_warn(&format!(
                "screenshot CDP capture failed for {url}: {err}; falling back to Spider screenshot"
            ));
        }
    }

    let params = ScreenshotParams {
        full_page: Some(full_page),
        ..Default::default()
    };

    let screenshot_config = ScreenShotConfig::new(
        params, true,  // bytes — return PNG bytes on page.screenshot_bytes
        false, // save — we handle file writing ourselves
        None,  // output_dir — not needed since save=false
    );

    let mut website = Website::new(url);
    website
        .with_chrome_connection(Some(chrome_url))
        .with_chrome_intercept(RequestInterceptConfiguration::new(true))
        .with_stealth(true)
        .with_fingerprint(true)
        .with_screenshot(Some(screenshot_config))
        .with_viewport(Some(Viewport::new(width, height)))
        .with_blacklist_url(Some(ssrf_blacklist_compact_strings().to_vec()))
        // Single page only — no crawling beyond the target URL.
        .with_limit(1)
        .with_depth(0)
        .with_subdomains(false);

    website.with_user_agent(Some(
        cfg.chrome_user_agent
            .as_deref()
            .unwrap_or_else(|| axon_ua()),
    ));
    if let Some(proxy) = cfg.chrome_proxy.as_deref() {
        website.with_proxies(Some(vec![proxy.to_string()]));
    }
    if let Some(timeout_ms) = cfg.request_timeout_ms {
        website.with_request_timeout(Some(std::time::Duration::from_millis(timeout_ms)));
    }
    if cfg.accept_invalid_certs {
        website.with_danger_accept_invalid_certs(true);
    }
    if cfg.bypass_csp {
        website.with_csp_bypass(true);
    }
    if !cfg.custom_headers.is_empty() {
        let map = parse_custom_headers(&cfg.custom_headers);
        if !map.is_empty() {
            website.with_headers(Some(map));
        }
    }
    let retries = cfg.fetch_retries.min(u8::MAX as usize) as u8;
    website.with_retry(retries);

    // Wait for network idle so JS-rendered pages finish loading before capture.
    website.with_wait_for_idle_network0(Some(spider::configuration::WaitForIdleNetwork::new(
        Some(std::time::Duration::from_secs(
            cfg.chrome_network_idle_timeout_secs,
        )),
    )));

    // Dismiss browser dialogs that would otherwise block capture indefinitely.
    website.with_dismiss_dialogs(true);
    website.configuration.disable_log = true;

    // Build the website config (required after Chrome settings).
    let mut website = website
        .build()
        .map_err(|_| format!("failed to build Spider website config for screenshot of {url}"))?;

    let mut rx = website.subscribe(16);
    let collect = tokio::spawn(async move { rx.recv().await.ok() });

    website.crawl().await;
    website.unsubscribe();

    let screenshot_bytes = match collect.await {
        Ok(Some(page)) => page.screenshot_bytes.clone(),
        Ok(None) | Err(_) => Some(
            website
                .get_pages()
                .and_then(|pages| pages.first().and_then(|page| page.screenshot_bytes.clone()))
                .ok_or_else(|| format!("no pages returned from screenshot crawl of {url}"))?,
        ),
    };

    screenshot_bytes.ok_or_else(|| {
        format!("screenshot bytes not captured for {url} — Chrome may not be reachable").into()
    })
}

async fn capture_screenshot_via_cdp(
    cfg: &Config,
    url: &str,
    browser_ws_url: &str,
    width: u32,
    height: u32,
    full_page: bool,
) -> Result<Vec<u8>, Box<dyn Error>> {
    use tokio_tungstenite::tungstenite::Message;

    let (stream, _) = tokio::time::timeout(
        std::time::Duration::from_secs(8),
        tokio_tungstenite::connect_async(browser_ws_url),
    )
    .await
    .map_err(|_| format!("timeout connecting to Chrome at {browser_ws_url}"))?
    .map_err(|err| format!("failed to connect to Chrome at {browser_ws_url}: {err}"))?;
    let (mut tx, mut rx) = stream.split();
    let timeout = std::time::Duration::from_secs(cfg.request_timeout_ms.unwrap_or(30_000) / 1000)
        .clamp(std::time::Duration::from_secs(5), std::time::Duration::from_secs(120));

    let target = send_cdp_cmd(
        &mut tx,
        &mut rx,
        None,
        "Target.createTarget",
        serde_json::json!({ "url": "about:blank" }),
        timeout,
    )
    .await?;
    let target_id = target
        .get("targetId")
        .and_then(|value| value.as_str())
        .ok_or("Chrome did not return targetId")?
        .to_string();

    let result = async {
        let attached = send_cdp_cmd(
            &mut tx,
            &mut rx,
            None,
            "Target.attachToTarget",
            serde_json::json!({ "targetId": target_id, "flatten": true }),
            timeout,
        )
        .await?;
        let session_id = attached
            .get("sessionId")
            .and_then(|value| value.as_str())
            .ok_or("Chrome did not return sessionId")?
            .to_string();

        send_cdp_cmd(
            &mut tx,
            &mut rx,
            Some(&session_id),
            "Page.enable",
            serde_json::json!({}),
            timeout,
        )
        .await?;
        send_cdp_cmd(
            &mut tx,
            &mut rx,
            Some(&session_id),
            "Emulation.setDeviceMetricsOverride",
            serde_json::json!({
                "width": width,
                "height": height,
                "deviceScaleFactor": 1,
                "mobile": false
            }),
            timeout,
        )
        .await?;

        let mut headers = serde_json::Map::new();
        headers.insert(
            "User-Agent".to_string(),
            serde_json::Value::String(
                cfg.chrome_user_agent
                    .as_deref()
                    .unwrap_or_else(|| axon_ua())
                    .to_string(),
            ),
        );
        for (key, value) in parse_custom_headers(&cfg.custom_headers) {
            if let Ok(value) = value.to_str() {
                if let Some(key) = key {
                    headers.insert(
                        key.as_str().to_string(),
                        serde_json::Value::String(value.to_string()),
                    );
                }
            }
        }
        send_cdp_cmd(
            &mut tx,
            &mut rx,
            Some(&session_id),
            "Network.setExtraHTTPHeaders",
            serde_json::json!({ "headers": headers }),
            timeout,
        )
        .await
        .ok();
        send_cdp_cmd(
            &mut tx,
            &mut rx,
            Some(&session_id),
            "Page.navigate",
            serde_json::json!({ "url": url }),
            timeout,
        )
        .await?;
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

        let result = send_cdp_cmd(
            &mut tx,
            &mut rx,
            Some(&session_id),
            "Page.captureScreenshot",
            serde_json::json!({
                "format": "png",
                "fromSurface": true,
                "captureBeyondViewport": full_page
            }),
            timeout,
        )
        .await?;
        let data = result
            .get("data")
            .and_then(|value| value.as_str())
            .ok_or("Chrome screenshot response missing data")?;
        BASE64_STANDARD
            .decode(data)
            .map_err(|err| format!("Chrome screenshot data was not valid base64: {err}").into())
    }
    .await;

    let _ = send_cdp_cmd(
        &mut tx,
        &mut rx,
        None,
        "Target.closeTarget",
        serde_json::json!({ "targetId": target_id }),
        std::time::Duration::from_secs(5),
    )
    .await;
    let _ = tx.send(Message::Close(None)).await;

    result
}

async fn send_cdp_cmd<Tx, Rx>(
    tx: &mut Tx,
    rx: &mut Rx,
    session_id: Option<&str>,
    method: &str,
    params: serde_json::Value,
    timeout: std::time::Duration,
) -> Result<serde_json::Value, Box<dyn Error>>
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

    let id = SCREENSHOT_CDP_ID.fetch_add(1, Ordering::Relaxed);
    let mut msg = serde_json::json!({ "id": id, "method": method, "params": params });
    if let Some(session_id) = session_id {
        msg["sessionId"] = serde_json::Value::String(session_id.to_string());
    }
    tx.send(Message::Text(msg.to_string().into()))
        .await
        .map_err(|err| format!("CDP send failed for {method}: {err}"))?;

    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let frame = tokio::time::timeout_at(deadline, rx.next())
            .await
            .map_err(|_| format!("timeout waiting for CDP response to {method}"))?
            .ok_or_else(|| format!("Chrome WebSocket closed waiting for {method}"))?
            .map_err(|err| format!("Chrome WebSocket error waiting for {method}: {err}"))?;
        let Message::Text(text) = frame else {
            continue;
        };
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|err| format!("CDP response JSON parse failed for {method}: {err}"))?;
        if value.get("id").and_then(|value| value.as_u64()) != Some(id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            return Err(format!("CDP error for {method}: {error}").into());
        }
        return Ok(value
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null));
    }
}

/// Sanitize a URL into a safe screenshot filename.
///
/// Strips the scheme, replaces non-alphanumeric chars with hyphens,
/// collapses runs of hyphens, trims edges, and truncates to 120 chars.
pub(crate) fn url_to_screenshot_filename(url: &str, idx: usize) -> String {
    let stripped = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    let sanitized: String = stripped
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();

    // Collapse consecutive hyphens and trim leading/trailing hyphens.
    let mut collapsed = String::with_capacity(sanitized.len());
    let mut prev_hyphen = true; // Start true to trim leading hyphens.
    for c in sanitized.chars() {
        if c == '-' {
            if !prev_hyphen {
                collapsed.push('-');
            }
            prev_hyphen = true;
        } else {
            collapsed.push(c);
            prev_hyphen = false;
        }
    }
    let collapsed = collapsed.trim_end_matches('-');

    // Truncate to a reasonable filename length.
    let max_name = 120;
    let name = if collapsed.len() > max_name {
        &collapsed[..max_name]
    } else {
        collapsed
    };

    format!("{idx:04}-{name}.png")
}
