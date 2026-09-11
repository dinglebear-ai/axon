//! HTTP client construction and shared singleton.

use encoding_rs::{Encoding, UTF_8};
use mime::Mime;
#[cfg(not(test))]
use std::sync::LazyLock;
use std::time::Duration;

pub const DEFAULT_MAX_RESPONSE_BODY_BYTES: usize = 64 * 1024 * 1024;

use super::error::HttpError;
use super::normalize::normalize_url;
use super::ssrf::validate_url;

#[cfg(not(test))]
pub(crate) static HTTP_CLIENT: LazyLock<Result<reqwest::Client, String>> =
    LazyLock::new(|| build_client(30, Some(super::ua::axon_ua())).map_err(|e| e.to_string()));

#[cfg(not(test))]
pub(crate) static INTERNAL_SERVICE_HTTP_CLIENT: LazyLock<Result<reqwest::Client, String>> =
    LazyLock::new(|| {
        build_client_with_options(30, Some(super::ua::axon_ua()), false, true, true)
            .map_err(|e| e.to_string())
    });

#[cfg(not(test))]
pub(crate) static INTERNAL_SERVICE_NO_REDIRECT_HTTP_CLIENT: LazyLock<
    Result<reqwest::Client, String>,
> = LazyLock::new(|| {
    build_client_with_options(30, Some(super::ua::axon_ua()), false, false, true)
        .map_err(|e| e.to_string())
});

#[cfg(not(test))]
pub fn http_client() -> anyhow::Result<&'static reqwest::Client> {
    HTTP_CLIENT
        .as_ref()
        .map_err(|err| anyhow::Error::msg(format!("failed to initialize HTTP client: {err}")))
}

#[cfg(not(test))]
pub fn internal_service_http_client() -> anyhow::Result<&'static reqwest::Client> {
    INTERNAL_SERVICE_HTTP_CLIENT.as_ref().map_err(|err| {
        anyhow::Error::msg(format!("failed to initialize internal HTTP client: {err}"))
    })
}

#[cfg(test)]
pub fn http_client() -> anyhow::Result<&'static reqwest::Client> {
    // In tests, each #[tokio::test] runs on its own runtime. A process-wide
    // reqwest::Client can hold a handle to a dropped runtime and intermittently
    // fail with "dispatch task is gone". Use a fresh client per call.
    //
    // The `Box::leak` is intentional and bounded: each test leaks one
    // reqwest::Client (~200 bytes). For a typical test suite this is negligible
    // and avoids lifetime issues with static references to runtime-scoped data.
    let client = build_client(30, None)
        .map_err(|err| anyhow::Error::msg(format!("failed to initialize HTTP client: {err}")))?;
    Ok(Box::leak(Box::new(client)))
}

#[cfg(test)]
pub fn internal_service_http_client() -> anyhow::Result<&'static reqwest::Client> {
    let client = build_client_with_options(30, None, false, true, true).map_err(|err| {
        anyhow::Error::msg(format!("failed to initialize internal HTTP client: {err}"))
    })?;
    Ok(Box::leak(Box::new(client)))
}

#[cfg(not(test))]
pub fn internal_service_no_redirect_http_client() -> anyhow::Result<&'static reqwest::Client> {
    INTERNAL_SERVICE_NO_REDIRECT_HTTP_CLIENT
        .as_ref()
        .map_err(|err| {
            anyhow::Error::msg(format!(
                "failed to initialize internal no-redirect HTTP client: {err}"
            ))
        })
}

#[cfg(test)]
pub fn internal_service_no_redirect_http_client() -> anyhow::Result<&'static reqwest::Client> {
    let client = build_client_with_options(30, None, false, false, true).map_err(|err| {
        anyhow::Error::msg(format!(
            "failed to initialize internal no-redirect HTTP client: {err}"
        ))
    })?;
    Ok(Box::leak(Box::new(client)))
}

pub fn build_client(
    timeout_secs: u64,
    user_agent: Option<&str>,
) -> Result<reqwest::Client, HttpError> {
    build_client_with_options(timeout_secs, user_agent, true, true, false)
}

pub fn build_client_no_redirect(
    timeout_secs: u64,
    user_agent: Option<&str>,
) -> Result<reqwest::Client, HttpError> {
    build_client_with_options(timeout_secs, user_agent, true, false, false)
}

pub fn build_ssrf_guarded_client_builder(timeout: Option<Duration>) -> reqwest::ClientBuilder {
    base_client_builder(timeout, true)
}

/// Maximum redirect hops followed by the shared clients.
///
/// Matches reqwest's own default so capping restores the behaviour that
/// `Policy::custom` silently removed, rather than tightening it.
///
/// The `>` comparison is deliberate and matches reqwest's own
/// (`redirect.rs`: `if attempt.previous.len() > max`): `previous` includes the
/// initial URL, so `> 10` permits 10 actual redirects. Using `>=` here would
/// allow only 9 and would silently disagree with the impersonating client.
const MAX_REDIRECT_HOPS: usize = 10;

fn build_client_with_options(
    timeout_secs: u64,
    user_agent: Option<&str>,
    ssrf_dns_guard: bool,
    follow_redirects: bool,
    disable_proxy: bool,
) -> Result<reqwest::Client, HttpError> {
    let mut builder = base_client_builder(Some(Duration::from_secs(timeout_secs)), ssrf_dns_guard);
    builder = if follow_redirects {
        builder.redirect(reqwest::redirect::Policy::custom(|attempt| {
            // `Policy::custom` REPLACES reqwest's default hop cap — it does not
            // layer on top of it. Without this check a redirect loop is followed
            // forever, pinning a connection and a task. Cap first, then revalidate
            // SSRF on every surviving hop.
            if attempt.previous().len() > MAX_REDIRECT_HOPS {
                return attempt.error(std::io::Error::other(format!(
                    "too many redirects (>{MAX_REDIRECT_HOPS})"
                )));
            }
            let url_string = attempt.url().as_str().to_owned();
            match validate_url(&url_string) {
                Ok(()) => attempt.follow(),
                Err(_) => attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("SSRF: redirect to blocked URL {url_string}"),
                )),
            }
        }))
    } else {
        builder.redirect(reqwest::redirect::Policy::none())
    };
    if let Some(ua) = user_agent {
        builder = builder.user_agent(ua);
    }
    if disable_proxy {
        builder = builder.no_proxy();
    }
    Ok(builder.build()?)
}

fn base_client_builder(timeout: Option<Duration>, ssrf_dns_guard: bool) -> reqwest::ClientBuilder {
    // Explicit connection pool sizing. reqwest defaults `pool_max_idle_per_host`
    // to `usize::MAX`, which under sustained dual-Qdrant + TEI load on a single
    // host can drift toward ephemeral-port pressure (Linux default range ~28K).
    // Cap idle reuse and the idle TTL so the pool actively recycles connections
    // instead of growing unbounded. (bd axon_rust-wo1)
    let mut builder = reqwest::Client::builder()
        .pool_max_idle_per_host(50)
        .pool_idle_timeout(Some(Duration::from_secs(60)));
    if let Some(timeout) = timeout {
        builder = builder.timeout(timeout);
    }
    // Wire the SSRF-blocking DNS resolver in every build so connect-time
    // enforcement is exercised by tests too. Explicit test loopback allowance
    // is captured by the resolver when this client is constructed.
    if ssrf_dns_guard {
        builder = builder.dns_resolver(super::ssrf::SsrfBlockingResolver::for_current_policy());
    }
    builder
}

pub async fn read_response_bytes_bounded(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, HttpError> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(HttpError::ResponseTooLarge { max_bytes });
    }
    let initial_capacity = response
        .content_length()
        .unwrap_or_default()
        .min(max_bytes as u64)
        .min(1024 * 1024) as usize;
    let mut bytes = Vec::with_capacity(initial_capacity);
    while let Some(chunk) = response.chunk().await? {
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            return Err(HttpError::ResponseTooLarge { max_bytes });
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub async fn read_response_text_bounded(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<String, HttpError> {
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = read_response_bytes_bounded(response, max_bytes).await?;
    Ok(decode_response_text(&bytes, content_type.as_deref()))
}

pub(super) fn decode_response_text(bytes: &[u8], content_type: Option<&str>) -> String {
    let content_type = content_type.and_then(|value| value.parse::<Mime>().ok());
    let encoding_name = content_type
        .as_ref()
        .and_then(|mime| mime.get_param("charset").map(|charset| charset.as_str()))
        .unwrap_or("utf-8");
    let encoding = Encoding::for_label(encoding_name.as_bytes()).unwrap_or(UTF_8);
    let (text, _, _) = encoding.decode(bytes);
    text.into_owned()
}

pub async fn read_response_json_bounded<T>(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<T, anyhow::Error>
where
    T: serde::de::DeserializeOwned,
{
    let bytes = read_response_bytes_bounded(response, max_bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

pub async fn fetch_html(client: &reqwest::Client, url: &str) -> Result<String, anyhow::Error> {
    let normalized = normalize_url(url);
    validate_url(&normalized)?;
    let response = client
        .get(normalized.as_ref())
        .send()
        .await?
        .error_for_status()?;
    Ok(read_response_text_bounded(response, DEFAULT_MAX_RESPONSE_BODY_BYTES).await?)
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
