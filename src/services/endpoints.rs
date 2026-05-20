use crate::core::config::Config;
use crate::core::content::{
    EndpointExtractOptions, PrefetchedBundle, discover_script_sources, extract_endpoints,
};
use crate::core::http::{axon_ua, build_client, normalize_url, validate_url_with_dns};
use crate::services::events::{LogLevel, ServiceEvent, emit};
use crate::services::types::{
    DiscoveredEndpoint, EndpointKind, EndpointOptions, EndpointReport, EndpointSourceKind,
};
use futures_util::{StreamExt, stream};
use std::collections::BTreeMap;
use std::error::Error;
use std::future::Future;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use url::Url;

mod capture;
use capture::capture_requests_with_chrome;
mod verify;
use verify::verify_endpoints;

type EndpointError = Box<dyn Error + Send + Sync>;

const BUNDLE_TIMEOUT_SECS: u64 = 8;
const MAX_BUNDLE_BYTES: usize = 2 * 1024 * 1024;
const CAPTURE_MAX_REQUESTS: usize = 500;
const CAPTURE_VALIDATION_CONCURRENCY: usize = 32;

#[derive(Debug, Clone)]
pub struct CapturedRequest {
    pub url: String,
    pub method: Option<String>,
}

pub trait NetworkCaptureProvider {
    fn capture(
        &self,
        cfg: &Config,
        url: &str,
        max_requests: usize,
    ) -> impl Future<Output = Result<Vec<CapturedRequest>, EndpointError>> + Send;
}

#[derive(Debug, Clone, Copy)]
pub struct ChromeNetworkCapture;

impl NetworkCaptureProvider for ChromeNetworkCapture {
    async fn capture(
        &self,
        cfg: &Config,
        url: &str,
        max_requests: usize,
    ) -> Result<Vec<CapturedRequest>, EndpointError> {
        let Some(remote_url) = cfg
            .chrome_remote_url
            .as_deref()
            .filter(|url| !url.is_empty())
        else {
            return Err(
                "capture_network requires AXON_CHROME_REMOTE_URL or chrome.remote-url".into(),
            );
        };
        capture_requests_with_chrome(
            remote_url,
            url,
            max_requests,
            cfg.chrome_network_idle_timeout_secs,
        )
        .await
        .map_err(Into::into)
    }
}

pub async fn discover(
    cfg: &Config,
    url: &str,
    options: EndpointOptions,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<EndpointReport, EndpointError> {
    discover_with_capture_provider(cfg, url, options, &ChromeNetworkCapture, tx).await
}

pub async fn discover_with_capture_provider<P: NetworkCaptureProvider + Sync>(
    cfg: &Config,
    url: &str,
    mut options: EndpointOptions,
    capture_provider: &P,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<EndpointReport, EndpointError> {
    normalize_options(&mut options);
    let started = Instant::now();
    let normalized = normalize_url(url).into_owned();
    validate_url_with_dns_timeout(&normalized).await?;
    emit_endpoint_log(&tx, format!("starting endpoint discovery: {normalized}")).await;

    let client = build_client(timeout_secs(cfg, BUNDLE_TIMEOUT_SECS), Some(axon_ua()))?;
    let (html, html_truncated) =
        fetch_bounded_text(&client, &normalized, options.max_scan_bytes, true).await?;
    emit_endpoint_log(&tx, "endpoint discovery fetched target page").await;
    let (script_sources, script_truncated) =
        discover_script_sources(&html, &normalized, options.max_scripts);
    let bundle_sources: Vec<_> = if options.include_bundles {
        script_sources
            .iter()
            .filter(|script| script.first_party)
            .take(options.max_scripts)
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    let bundles = fetch_bundles(&client, &bundle_sources, options.max_scan_bytes).await;
    let mut warnings = Vec::new();
    let mut prefetched = Vec::new();
    for item in bundles {
        match item {
            Ok(bundle) => prefetched.push(bundle),
            Err(message) => warnings.push(message),
        }
    }
    if options.include_bundles {
        emit_endpoint_log(
            &tx,
            format!(
                "endpoint discovery fetched {} of {} bundles",
                prefetched.len(),
                bundle_sources.len()
            ),
        )
        .await;
    }

    let mut report = extract_endpoints(
        &html,
        &normalized,
        &prefetched,
        &EndpointExtractOptions {
            max_scripts: options.max_scripts,
            max_scan_bytes: options.max_scan_bytes,
            unique_only: options.unique_only,
            ..EndpointExtractOptions::default()
        },
    );
    report.truncated |= script_truncated;
    report.truncated |= html_truncated;
    report.warnings.extend(warnings);

    if options.capture_network {
        emit_endpoint_log(&tx, "endpoint discovery starting network capture").await;
        merge_network_capture(
            cfg,
            &normalized,
            &mut report,
            capture_provider,
            options.first_party_only,
        )
        .await?;
    }
    if options.first_party_only {
        report.endpoints.retain(|endpoint| endpoint.first_party);
        recompute_hosts(&mut report);
    }
    if options.verify {
        emit_endpoint_log(&tx, "endpoint discovery verifying endpoints").await;
        verify_endpoints(cfg, &normalized, &mut report).await;
    }
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    emit_endpoint_log(
        &tx,
        format!(
            "endpoint discovery complete: {} endpoints",
            report.endpoints.len()
        ),
    )
    .await;
    Ok(report)
}

async fn emit_endpoint_log(tx: &Option<mpsc::Sender<ServiceEvent>>, message: impl Into<String>) {
    emit(
        tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: message.into(),
        },
    )
    .await;
}

pub fn options_from_config(cfg: &Config) -> EndpointOptions {
    EndpointOptions {
        include_bundles: cfg.endpoints_include_bundles,
        first_party_only: cfg.endpoints_first_party_only,
        unique_only: cfg.endpoints_unique_only,
        max_scripts: cfg.endpoints_max_scripts,
        max_scan_bytes: cfg.endpoints_max_scan_bytes,
        verify: cfg.endpoints_verify,
        capture_network: cfg.endpoints_capture_network,
    }
}

fn normalize_options(options: &mut EndpointOptions) {
    options.max_scripts = options.max_scripts.clamp(1, 200);
    options.max_scan_bytes = options.max_scan_bytes.clamp(1024, 64 * 1024 * 1024);
}

pub(super) async fn validate_url_with_dns_timeout(url: &str) -> Result<(), EndpointError> {
    tokio::time::timeout(Duration::from_millis(2_000), validate_url_with_dns(url))
        .await
        .map_err(|_| format!("invalid endpoint discovery url {url}: DNS validation timed out"))?
        .map_err(|e| format!("invalid endpoint discovery url {url}: {e}").into())
}

fn timeout_secs(cfg: &Config, fallback: u64) -> u64 {
    cfg.request_timeout_ms
        .map(|ms| ms.div_ceil(1000).max(1))
        .unwrap_or(fallback)
}

async fn fetch_bundles(
    client: &reqwest::Client,
    sources: &[crate::core::content::ScriptSource],
    max_scan_bytes: usize,
) -> Vec<Result<PrefetchedBundle, String>> {
    stream::iter(sources.iter().cloned())
        .map(|source| async move {
            fetch_script_bundle(client, &source.url, max_scan_bytes)
                .await
                .map_err(|err| format!("bundle fetch skipped {}: {err}", source.url))
        })
        .buffer_unordered(8)
        .collect()
        .await
}

async fn fetch_script_bundle(
    client: &reqwest::Client,
    url: &str,
    max_scan_bytes: usize,
) -> Result<PrefetchedBundle, EndpointError> {
    validate_url_with_dns_timeout(url).await?;
    let response = client.get(url).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {status}").into());
    }
    if let Some(content_type) = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    {
        let lower = content_type.to_ascii_lowercase();
        if !(lower.contains("javascript")
            || lower.contains("ecmascript")
            || lower.contains("text/plain")
            || lower.contains("application/octet-stream"))
        {
            return Err(format!("content-type {content_type} is not JavaScript-like").into());
        }
    }
    fetch_response_text(response, max_scan_bytes.min(MAX_BUNDLE_BYTES))
        .await
        .map(|(text, truncated)| PrefetchedBundle {
            url: url.to_string(),
            text,
            truncated,
        })
}

async fn fetch_bounded_text(
    client: &reqwest::Client,
    url: &str,
    max_bytes: usize,
    require_success: bool,
) -> Result<(String, bool), EndpointError> {
    let response = client.get(url).send().await?;
    if require_success {
        response.error_for_status_ref()?;
    }
    fetch_response_text(response, max_bytes).await
}

async fn fetch_response_text(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<(String, bool), EndpointError> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    let mut truncated = false;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            let remaining = max_bytes.saturating_sub(bytes.len());
            bytes.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}

async fn merge_network_capture<P: NetworkCaptureProvider + Sync>(
    cfg: &Config,
    url: &str,
    report: &mut EndpointReport,
    capture_provider: &P,
    first_party_only: bool,
) -> Result<(), EndpointError> {
    let captured = capture_provider
        .capture(cfg, url, CAPTURE_MAX_REQUESTS)
        .await?;
    let mut pending = Vec::with_capacity(CAPTURE_VALIDATION_CONCURRENCY);
    for request in captured {
        if report.endpoints.len() >= crate::core::content::DEFAULT_MAX_ENDPOINTS {
            report.truncated = true;
            break;
        }
        let first_party = first_party_for_url(url, &request.url);
        if first_party_only && !first_party {
            continue;
        }
        let Some(validation_url) = capture_validation_url(&request.url) else {
            report.warnings.push(format!(
                "network capture skipped {}: unsupported URL",
                request.url
            ));
            continue;
        };
        pending.push((request, first_party, validation_url));
        if pending.len() >= CAPTURE_VALIDATION_CONCURRENCY {
            merge_validated_capture_batch(url, report, std::mem::take(&mut pending)).await;
        }
    }
    if !pending.is_empty() {
        merge_validated_capture_batch(url, report, pending).await;
    }
    recompute_hosts(report);
    Ok(())
}

fn recompute_hosts(report: &mut EndpointReport) {
    let mut hosts = std::collections::BTreeSet::new();
    for endpoint in &report.endpoints {
        if let Some(host) = endpoint
            .normalized_url
            .as_deref()
            .and_then(|value| Url::parse(value).ok())
            .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
        {
            hosts.insert(host);
        }
    }
    report.hosts = hosts.into_iter().collect();
}

fn first_party_for_url(page_url: &str, candidate: &str) -> bool {
    let page_host = Url::parse(page_url)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
        .unwrap_or_default();
    Url::parse(candidate)
        .ok()
        .and_then(|url| url.host_str().map(|host| host.to_ascii_lowercase()))
        .map(|host| host == page_host || host.ends_with(&format!(".{page_host}")))
        .unwrap_or(true)
}

async fn validate_captured_request_origins(
    validation_urls: std::collections::BTreeSet<String>,
) -> BTreeMap<String, Option<String>> {
    stream::iter(validation_urls)
        .map(|validation_url| async move {
            let result = validate_url_with_dns_timeout(&validation_url)
                .await
                .err()
                .map(|err| err.to_string());
            (validation_url, result)
        })
        .buffer_unordered(CAPTURE_VALIDATION_CONCURRENCY)
        .collect()
        .await
}

async fn merge_validated_capture_batch(
    page_url: &str,
    report: &mut EndpointReport,
    batch: Vec<(CapturedRequest, bool, String)>,
) {
    let validation_urls = batch
        .iter()
        .map(|(_, _, validation_url)| validation_url.clone())
        .collect();
    let validation_cache = validate_captured_request_origins(validation_urls).await;

    for (request, first_party, validation_url) in batch {
        if report.endpoints.len() >= crate::core::content::DEFAULT_MAX_ENDPOINTS {
            report.truncated = true;
            break;
        }
        if let Some(Some(err)) = validation_cache.get(&validation_url) {
            report
                .warnings
                .push(format!("network capture skipped {}: {err}", request.url));
            continue;
        }
        merge_validated_capture_request(page_url, report, request, first_party);
    }
}

fn merge_validated_capture_request(
    page_url: &str,
    report: &mut EndpointReport,
    request: CapturedRequest,
    first_party: bool,
) {
    let kind = if request.url.starts_with("ws://") || request.url.starts_with("wss://") {
        EndpointKind::Websocket
    } else if request.url.to_ascii_lowercase().contains("graphql") {
        EndpointKind::Graphql
    } else {
        EndpointKind::AbsoluteUrl
    };
    if report
        .endpoints
        .iter()
        .any(|endpoint| endpoint.normalized_url.as_deref() == Some(request.url.as_str()))
    {
        return;
    }
    report.endpoints.push(DiscoveredEndpoint {
        value: request.url.clone(),
        normalized_url: Some(request.url),
        kind,
        first_party,
        source: EndpointSourceKind::NetworkCapture,
        source_url: Some(page_url.to_string()),
        verified: None,
    });
}

fn capture_validation_url(value: &str) -> Option<String> {
    let mut url = Url::parse(value).ok()?;
    match url.scheme() {
        "http" | "https" => {}
        "ws" => url.set_scheme("http").ok()?,
        "wss" => url.set_scheme("https").ok()?,
        _ => return None,
    }
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string())
}

#[cfg(test)]
#[path = "endpoints_tests.rs"]
mod tests;
