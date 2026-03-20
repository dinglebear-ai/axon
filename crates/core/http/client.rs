//! HTTP client construction and shared singleton.

#[cfg(not(test))]
use std::sync::LazyLock;
use std::time::Duration;

use super::error::HttpError;
use super::normalize::normalize_url;
use super::ssrf::validate_url;

#[cfg(not(test))]
pub(crate) static HTTP_CLIENT: LazyLock<Result<reqwest::Client, String>> = LazyLock::new(|| {
    let ua = std::env::var("AXON_CHROME_USER_AGENT").ok();
    build_client(30, ua.as_deref()).map_err(|e| e.to_string())
});

#[cfg(not(test))]
pub fn http_client() -> anyhow::Result<&'static reqwest::Client> {
    HTTP_CLIENT
        .as_ref()
        .map_err(|err| anyhow::Error::msg(format!("failed to initialize HTTP client: {err}")))
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

pub fn build_client(
    timeout_secs: u64,
    user_agent: Option<&str>,
) -> Result<reqwest::Client, HttpError> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let url_string = attempt.url().as_str().to_owned();
            match validate_url(&url_string) {
                Ok(()) => attempt.follow(),
                Err(_) => attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("SSRF: redirect to blocked URL {url_string}"),
                )),
            }
        }));
    if let Some(ua) = user_agent {
        builder = builder.user_agent(ua);
    }
    Ok(builder.build()?)
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
