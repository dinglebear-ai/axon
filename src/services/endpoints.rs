use crate::core::config::Config;
use crate::core::content::{
    EndpointExtractOptions, PrefetchedBundle, discover_script_sources, extract_endpoints,
};
use crate::core::http::{
    axon_ua, build_client, build_client_no_redirect, normalize_url, validate_url_with_dns,
};
use crate::services::types::{
    DiscoveredEndpoint, EndpointKind, EndpointOptions, EndpointReport, EndpointSourceKind,
    EndpointVerification,
};
use futures_util::{StreamExt, stream};
use std::collections::BTreeMap;
use std::error::Error;
use std::future::Future;
use std::time::{Duration, Instant};
use url::Url;

mod capture;
use capture::capture_requests_with_chrome;

type EndpointError = Box<dyn Error + Send + Sync>;

const BUNDLE_TIMEOUT_SECS: u64 = 8;
const VERIFY_TIMEOUT_SECS: u64 = 4;
const MAX_BUNDLE_BYTES: usize = 2 * 1024 * 1024;
const MAX_VERIFY_PROBES: usize = 40;
const VERIFY_CONCURRENCY: usize = 5;
const CAPTURE_MAX_REQUESTS: usize = 500;

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
) -> Result<EndpointReport, EndpointError> {
    discover_with_capture_provider(cfg, url, options, &ChromeNetworkCapture).await
}

pub async fn discover_with_capture_provider<P: NetworkCaptureProvider + Sync>(
    cfg: &Config,
    url: &str,
    mut options: EndpointOptions,
    capture_provider: &P,
) -> Result<EndpointReport, EndpointError> {
    normalize_options(&mut options);
    let started = Instant::now();
    let normalized = normalize_url(url).into_owned();
    validate_url_with_dns_timeout(&normalized).await?;

    let client = build_client(timeout_secs(cfg, BUNDLE_TIMEOUT_SECS), Some(axon_ua()))?;
    let (html, html_truncated) =
        fetch_bounded_text(&client, &normalized, options.max_scan_bytes, true).await?;
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
        verify_endpoints(cfg, &normalized, &mut report).await;
    }
    report.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(report)
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
    if let Some(content_length) = response.content_length()
        && content_length > max_bytes as u64
    {
        return Err(format!("response body exceeds cap {max_bytes} bytes").into());
    }
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
    let mut validation_cache: BTreeMap<String, Option<String>> = BTreeMap::new();
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
        let validation_err = match validation_cache.get(&validation_url) {
            Some(cached) => cached.clone(),
            None => {
                let result = validate_url_with_dns_timeout(&validation_url)
                    .await
                    .err()
                    .map(|err| err.to_string());
                validation_cache.insert(validation_url, result.clone());
                result
            }
        };
        if let Some(err) = validation_err {
            report
                .warnings
                .push(format!("network capture skipped {}: {err}", request.url));
            continue;
        }
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
            continue;
        }
        report.endpoints.push(DiscoveredEndpoint {
            value: request.url.clone(),
            normalized_url: Some(request.url.clone()),
            kind,
            first_party,
            source: EndpointSourceKind::NetworkCapture,
            source_url: Some(url.to_string()),
            verified: None,
        });
    }
    recompute_hosts(report);
    Ok(())
}

async fn verify_endpoints(cfg: &Config, page_url: &str, report: &mut EndpointReport) {
    let client =
        match build_client_no_redirect(timeout_secs(cfg, VERIFY_TIMEOUT_SECS), Some(axon_ua())) {
            Ok(client) => client,
            Err(err) => {
                report
                    .warnings
                    .push(format!("verification client unavailable: {err}"));
                return;
            }
        };
    let targets: Vec<(usize, String)> = report
        .endpoints
        .iter()
        .enumerate()
        .filter_map(|(idx, endpoint)| verification_url(page_url, endpoint).map(|url| (idx, url)))
        .take(MAX_VERIFY_PROBES)
        .collect();

    let results: Vec<_> = stream::iter(targets)
        .map(|(idx, url)| {
            let client = client.clone();
            async move { (idx, verify_one(&client, &url).await) }
        })
        .buffer_unordered(VERIFY_CONCURRENCY)
        .collect()
        .await;

    for (idx, verification) in results {
        if let Some(endpoint) = report.endpoints.get_mut(idx) {
            endpoint.verified = Some(verification);
        }
    }
}

fn verification_url(page_url: &str, endpoint: &DiscoveredEndpoint) -> Option<String> {
    if matches!(endpoint.kind, EndpointKind::Websocket) {
        return None;
    }
    endpoint.normalized_url.clone().or_else(|| {
        let base = Url::parse(page_url).ok()?;
        base.join(&endpoint.value).ok().map(|url| url.to_string())
    })
}

async fn verify_one(client: &reqwest::Client, url: &str) -> EndpointVerification {
    if let Err(err) = validate_url_with_dns_timeout(url).await {
        return verification_error(url, "HEAD", "ssrf_rejected", err.to_string());
    }
    match probe(client, reqwest::Method::HEAD, url).await {
        Ok(result) if result.status == Some(405) || result.status == Some(501) => {
            match probe(client, reqwest::Method::OPTIONS, url).await {
                Ok(result) => result,
                Err(err) => verification_error(url, "OPTIONS", "probe_error", err.to_string()),
            }
        }
        Ok(result) => result,
        Err(err) => verification_error(url, "HEAD", "probe_error", err.to_string()),
    }
}

async fn probe(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
) -> Result<EndpointVerification, EndpointError> {
    let response = client.request(method.clone(), url).send().await?;
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string);
    let final_url = response.url().to_string();
    Ok(EndpointVerification {
        attempted_url: url.to_string(),
        method: method.as_str().to_string(),
        status: Some(status),
        content_type,
        final_url: Some(final_url),
        redirect_count: 0,
        reachable: status < 500,
        error: None,
    })
}

fn verification_error(
    url: &str,
    method: &str,
    class: &str,
    detail: String,
) -> EndpointVerification {
    EndpointVerification {
        attempted_url: url.to_string(),
        method: method.to_string(),
        status: None,
        content_type: None,
        final_url: None,
        redirect_count: 0,
        reachable: false,
        error: Some(format!("{class}: {detail}")),
    }
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
