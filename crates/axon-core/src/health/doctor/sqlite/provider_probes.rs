//! Authenticated HTTP probes used by the SQLite-runtime doctor report.

use super::Config;
use crate::health::doctor::{probe_tei_info, timed_probe};
use crate::http::{internal_service_no_redirect_http_client, with_path};
use reqwest::header::{HeaderName, HeaderValue};
use serde_json::Value;
use std::time::Duration;

pub(super) struct ServiceProbes {
    pub(super) tei: (bool, Option<String>),
    pub(super) tei_latency_ms: u64,
    pub(super) tei_info: (Option<Value>, Option<String>),
    pub(super) qdrant: (bool, Option<String>),
    pub(super) chrome: (bool, Option<String>),
    pub(super) client_ok: bool,
}

#[derive(Clone)]
pub(super) struct ProbeAuth {
    pub(super) name: HeaderName,
    pub(super) value: HeaderValue,
}

impl ProbeAuth {
    fn from_env(env_name: &str, header_name: HeaderName, prefix: &str) -> Option<Self> {
        let token = std::env::var(env_name)
            .ok()
            .filter(|value| !value.trim().is_empty())?;
        let mut value = HeaderValue::from_str(&format!("{prefix}{token}")).ok()?;
        value.set_sensitive(true);
        Some(Self {
            name: header_name,
            value,
        })
    }
}

pub(super) async fn collect_service_probes(cfg: &Config) -> ServiceProbes {
    let probe_client_result = internal_service_no_redirect_http_client();
    let client_err_detail = probe_client_result
        .as_ref()
        .err()
        .map(|e| format!("http client init failed: {e}"));

    match probe_client_result {
        Ok(client) => {
            let chrome_url = cfg.chrome_remote_url.as_deref();
            let tei_auth = ProbeAuth::from_env(
                "AXON_TEI_BEARER_TOKEN",
                reqwest::header::AUTHORIZATION,
                "Bearer ",
            );
            let qdrant_auth =
                ProbeAuth::from_env("QDRANT_API_KEY", HeaderName::from_static("api-key"), "");
            let chrome_auth = ProbeAuth::from_env(
                "AXON_CHROME_BEARER_TOKEN",
                reqwest::header::AUTHORIZATION,
                "Bearer ",
            );
            let ((tei, tei_latency_ms), (qdrant, _), (chrome, _)) = spider::tokio::join!(
                timed_probe(probe_internal_http(
                    client,
                    &cfg.tei_url,
                    &["/health", "/"],
                    tei_auth.as_ref(),
                )),
                timed_probe(probe_internal_http(
                    client,
                    &cfg.qdrant_url,
                    &["/healthz", "/"],
                    qdrant_auth.as_ref(),
                )),
                timed_probe(probe_internal_chrome(
                    client,
                    chrome_url,
                    chrome_auth.as_ref()
                )),
            );
            let (tei_info, _) = match tei_auth.as_ref() {
                Some(auth) => {
                    timed_probe(probe_authenticated_tei_info(&cfg.tei_url, client, auth)).await
                }
                None => timed_probe(probe_tei_info(&cfg.tei_url, client)).await,
            };

            ServiceProbes {
                tei,
                tei_latency_ms,
                tei_info,
                qdrant,
                chrome,
                client_ok: true,
            }
        }
        Err(_) => failed_service_probes(client_err_detail),
    }
}

fn failed_service_probes(detail: Option<String>) -> ServiceProbes {
    let failed = (false, detail.clone());
    let tei_info = (None, detail);

    ServiceProbes {
        tei: failed.clone(),
        tei_latency_ms: 0,
        tei_info,
        qdrant: failed.clone(),
        chrome: failed,
        client_ok: false,
    }
}

async fn probe_internal_chrome(
    client: &reqwest::Client,
    chrome_url: Option<&str>,
    auth: Option<&ProbeAuth>,
) -> (bool, Option<String>) {
    match chrome_url {
        Some(url) if !url.trim().is_empty() => {
            probe_internal_http(client, url, &["/json/version", "/json"], auth).await
        }
        _ => (false, None),
    }
}

pub(super) async fn probe_internal_http(
    client: &reqwest::Client,
    url: &str,
    paths: &[&str],
    auth: Option<&ProbeAuth>,
) -> (bool, Option<String>) {
    if url.trim().is_empty() {
        return (false, Some("not configured".to_string()));
    }

    let mut last_error = None;
    for path in paths {
        let endpoint = with_path(url, path);
        match authenticated_probe_request(client, url, &endpoint, auth)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() || status.is_redirection() {
                    return (true, Some(format!("http {}", status.as_u16())));
                }
                last_error = Some(format!("http {}", status.as_u16()));
            }
            Err(err) => last_error = Some(err.to_string()),
        }
    }

    (false, last_error)
}

pub(super) fn authenticated_probe_request(
    client: &reqwest::Client,
    configured_url: &str,
    endpoint: &str,
    auth: Option<&ProbeAuth>,
) -> reqwest::RequestBuilder {
    let request = client.get(endpoint);
    match auth.filter(|_| same_origin(configured_url, endpoint)) {
        Some(auth) => request.header(auth.name.clone(), auth.value.clone()),
        None => request,
    }
}

pub(super) fn same_origin(configured_url: &str, endpoint: &str) -> bool {
    let Ok(configured) = url::Url::parse(configured_url) else {
        return false;
    };
    let Ok(endpoint) = url::Url::parse(endpoint) else {
        return false;
    };
    configured.username().is_empty()
        && configured.password().is_none()
        && endpoint.username().is_empty()
        && endpoint.password().is_none()
        && configured.scheme() == endpoint.scheme()
        && configured.host_str() == endpoint.host_str()
        && configured.port_or_known_default() == endpoint.port_or_known_default()
}

pub(super) async fn probe_authenticated_tei_info(
    url: &str,
    client: &reqwest::Client,
    auth: &ProbeAuth,
) -> (Option<Value>, Option<String>) {
    if url.trim().is_empty() {
        return (None, Some("not configured".to_string()));
    }

    let mut last_error = None;
    for path in ["/info", "/v1/info"] {
        let endpoint = with_path(url, path);
        match authenticated_probe_request(client, url, &endpoint, Some(auth))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                let status = resp.status();
                match crate::http::read_response_json_bounded::<Value>(
                    resp,
                    crate::http::DEFAULT_MAX_RESPONSE_BODY_BYTES,
                )
                .await
                {
                    Ok(json) => return (Some(json), Some(format!("{path} {status}"))),
                    Err(err) => last_error = Some(format!("{path} invalid json: {err}")),
                }
            }
            Ok(resp) => last_error = Some(format!("{path} {}", resp.status())),
            Err(err) => last_error = Some(err.to_string()),
        }
    }

    (None, last_error)
}

pub(super) async fn probe_collection_info_if_reachable(
    cfg: &Config,
    qdrant_ok: bool,
    client_ok: bool,
) -> (Option<String>, Option<u64>) {
    if !qdrant_ok || !client_ok {
        return (None, None);
    }

    let Ok(client) = internal_service_no_redirect_http_client() else {
        return (None, None);
    };
    let auth = ProbeAuth::from_env("QDRANT_API_KEY", HeaderName::from_static("api-key"), "");
    probe_collection_info(&client, &cfg.qdrant_url, &cfg.collection, auth.as_ref()).await
}

/// GET `/collections/{name}`, classify the vectors block, and extract the dense vector size.
///
/// Returns `(mode, dense_size)` where `mode` identifies named or unnamed vectors
/// and `dense_size` is the configured dense-vector dimension. Best-effort: an
/// unreachable or malformed endpoint returns `(None, None)`.
pub(super) async fn probe_collection_info(
    client: &reqwest::Client,
    qdrant_url: &str,
    collection: &str,
    auth: Option<&ProbeAuth>,
) -> (Option<String>, Option<u64>) {
    let url = format!(
        "{}/collections/{}",
        qdrant_url.trim_end_matches('/'),
        collection
    );
    let resp = match authenticated_probe_request(client, qdrant_url, &url, auth)
        .timeout(Duration::from_secs(5))
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => return (None, None),
    };
    if !resp.status().is_success() {
        return (None, None);
    }
    let body: Value = match crate::http::read_response_json_bounded(
        resp,
        crate::http::DEFAULT_MAX_RESPONSE_BODY_BYTES,
    )
    .await
    {
        Ok(value) => value,
        Err(_) => return (None, None),
    };
    let Some(vectors) = body
        .get("result")
        .and_then(|result| result.get("config"))
        .and_then(|config| config.get("params"))
        .and_then(|params| params.get("vectors"))
    else {
        return (None, None);
    };

    if let Some(size) = vectors.get("size").and_then(Value::as_u64) {
        (Some("unnamed".to_string()), Some(size))
    } else if vectors.is_object() {
        let dense_size = vectors
            .get("dense")
            .and_then(|dense| dense.get("size"))
            .and_then(Value::as_u64);
        (Some("named".to_string()), dense_size)
    } else {
        (None, None)
    }
}
