//! HTTP client construction and shared singleton.

#[cfg(not(test))]
use std::sync::LazyLock;
use std::time::Duration;

use super::error::HttpError;
use super::normalize::normalize_url;
use super::ssrf::validate_url;

#[cfg(not(test))]
pub(crate) static HTTP_CLIENT: LazyLock<Result<reqwest::Client, String>> =
    LazyLock::new(|| build_client(30, Some(super::ua::axon_ua())).map_err(|e| e.to_string()));

#[cfg(not(test))]
pub(crate) static INTERNAL_SERVICE_HTTP_CLIENT: LazyLock<Result<reqwest::Client, String>> =
    LazyLock::new(|| {
        build_client_without_ssrf_resolver(30, Some(super::ua::axon_ua()))
            .map_err(|e| e.to_string())
    });

#[cfg(not(test))]
pub fn http_client() -> anyhow::Result<&'static reqwest::Client> {
    HTTP_CLIENT
        .as_ref()
        .map_err(|err| anyhow::Error::msg(format!("failed to initialize HTTP client: {err}")))
}

#[cfg(not(test))]
pub(crate) fn internal_service_http_client() -> anyhow::Result<&'static reqwest::Client> {
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
pub(crate) fn internal_service_http_client() -> anyhow::Result<&'static reqwest::Client> {
    http_client()
}

pub fn build_client(
    timeout_secs: u64,
    user_agent: Option<&str>,
) -> Result<reqwest::Client, HttpError> {
    build_client_with_options(timeout_secs, user_agent, true, true, false)
}

pub(crate) fn build_client_no_redirect(
    timeout_secs: u64,
    user_agent: Option<&str>,
) -> Result<reqwest::Client, HttpError> {
    build_client_with_options(timeout_secs, user_agent, true, false, false)
}

pub(crate) fn build_ssrf_guarded_client_builder(
    timeout: Option<Duration>,
) -> reqwest::ClientBuilder {
    base_client_builder(timeout, true)
}

#[cfg(not(test))]
pub(crate) fn build_client_without_ssrf_resolver(
    timeout_secs: u64,
    user_agent: Option<&str>,
) -> Result<reqwest::Client, HttpError> {
    build_client_with_options(timeout_secs, user_agent, false, true, true)
}

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
    #[cfg(test)]
    let _ = ssrf_dns_guard;

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
    // Wire the SSRF-blocking DNS resolver in production builds to close the
    // DNS rebinding TOCTOU window at connect time. Test builds skip only the
    // custom resolver so httpmock servers on 127.0.0.1 remain reachable;
    // validate_url() still guards parse-time SSRF checks in tests.
    #[cfg(not(test))]
    {
        if ssrf_dns_guard {
            builder = builder.dns_resolver(super::ssrf::SsrfBlockingResolver);
        }
    }
    builder
}

pub async fn fetch_html(client: &reqwest::Client, url: &str) -> Result<String, anyhow::Error> {
    let normalized = normalize_url(url);
    validate_url(&normalized)?;
    let body = client
        .get(normalized.as_ref())
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(body)
}
