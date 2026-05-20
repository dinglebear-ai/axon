use super::{EndpointError, timeout_secs, validate_url_with_dns_timeout};
use crate::core::config::Config;
use crate::core::http::{axon_ua, build_client_no_redirect};
use crate::services::types::{
    DiscoveredEndpoint, EndpointKind, EndpointReport, EndpointVerification,
};
use futures_util::{StreamExt, stream};
use url::Url;

const VERIFY_TIMEOUT_SECS: u64 = 4;
const MAX_VERIFY_PROBES: usize = 40;
const VERIFY_CONCURRENCY: usize = 5;

pub(super) async fn verify_endpoints(cfg: &Config, page_url: &str, report: &mut EndpointReport) {
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
