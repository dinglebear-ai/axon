use anyhow::{Result, bail};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, Url};
use serde::Deserialize;

use crate::core::config::Config;
use crate::core::http::{build_ssrf_guarded_client_builder, validate_url};

/// Maximum response body size for success responses (10 MiB).
const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

/// Maximum error body size echoed in error messages (4 KiB).
const MAX_ERROR_BODY_BYTES: usize = 4 * 1024;

#[derive(Debug, Deserialize)]
pub(crate) struct GiteaRepo {
    pub(crate) full_name: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) html_url: Option<String>,
    pub(crate) clone_url: Option<String>,
    pub(crate) default_branch: Option<String>,
    pub(crate) private: Option<bool>,
    pub(crate) stars_count: Option<u64>,
    pub(crate) forks_count: Option<u64>,
    pub(crate) open_issues_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GiteaUser {
    pub(crate) login: Option<String>,
    pub(crate) full_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GiteaIssue {
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) body: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) html_url: Option<String>,
    pub(crate) user: Option<GiteaUser>,
    pub(crate) labels: Option<Vec<GiteaLabel>>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) comments: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GiteaLabel {
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GiteaPullRequest {
    pub(crate) number: u64,
    pub(crate) title: String,
    pub(crate) body: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) html_url: Option<String>,
    pub(crate) user: Option<GiteaUser>,
    pub(crate) labels: Option<Vec<GiteaLabel>>,
    pub(crate) created_at: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) comments: Option<u64>,
    pub(crate) merged: Option<bool>,
}

/// Build an SSRF-guarded reqwest client for Gitea API calls.
///
/// S-H1/B-M2: Uses `build_ssrf_guarded_client_builder` (SsrfBlockingResolver
/// at connect time) plus a custom redirect policy that rejects cross-host
/// redirects to prevent auth token exfiltration. Applies to both gitea.com/
/// codeberg.org and self-hosted Gitea/Forgejo instances.
pub(crate) fn build_client(cfg: &Config) -> Result<Client> {
    let mut headers = HeaderMap::new();
    if let Some(token) = cfg.gitea_token.as_deref().filter(|token| !token.is_empty()) {
        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("token {token}"))?,
        );
    }
    let client = build_ssrf_guarded_client_builder(Some(std::time::Duration::from_secs(60)))
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let prev_host = attempt
                .previous()
                .last()
                .and_then(|u| u.host_str().map(str::to_string));
            let next_host = attempt.url().host_str().map(str::to_string);
            if prev_host != next_host {
                return attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!(
                        "SSRF: cross-host redirect from {:?} to {:?}",
                        prev_host, next_host
                    ),
                ));
            }
            let url_str = attempt.url().as_str().to_owned();
            match validate_url(&url_str) {
                Ok(()) => attempt.follow(),
                Err(_) => attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("SSRF: redirect to blocked URL {url_str}"),
                )),
            }
        }))
        .build()?;
    Ok(client)
}

pub(crate) async fn fetch_repo(client: &Client, target: &super::GiteaTarget) -> Result<GiteaRepo> {
    let resp = client.get(target.repo_api_url("")).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = bounded_error_body(resp).await;
        bail!("Gitea repo fetch failed ({status}): {body}");
    }
    let bytes = resp.bytes().await?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        bail!(
            "Gitea repo response too large: {} bytes (max {MAX_RESPONSE_BYTES})",
            bytes.len()
        );
    }
    Ok(serde_json::from_slice(&bytes)?)
}

pub(crate) async fn fetch_paginated<T: for<'de> Deserialize<'de>>(
    client: &Client,
    url: &str,
    query: &[(&str, &str)],
    max_items: usize,
) -> Result<Vec<T>> {
    let mut out = Vec::new();
    let mut page = 1usize;
    loop {
        let mut request_url = Url::parse(url)?;
        {
            let mut pairs = request_url.query_pairs_mut();
            for (key, value) in query {
                pairs.append_pair(key, value);
            }
            pairs.append_pair("limit", "100");
            pairs.append_pair("page", &page.to_string());
        }
        let response = client.get(request_url).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let body = bounded_error_body(response).await;
            bail!("Gitea API error ({status}): {body}");
        }
        let bytes = response.bytes().await?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            bail!(
                "Gitea paginated response too large: {} bytes (max {MAX_RESPONSE_BYTES})",
                bytes.len()
            );
        }
        let mut page_items: Vec<T> = serde_json::from_slice(&bytes)?;
        let page_len = page_items.len();
        if max_items > 0 {
            let remaining = max_items.saturating_sub(out.len());
            page_items.truncate(remaining);
        }
        out.append(&mut page_items);
        if page_len < 100 || (max_items > 0 && out.len() >= max_items) {
            break;
        }
        page += 1;
    }
    Ok(out)
}

/// Read and sanitize an error response body, bounded to `MAX_ERROR_BODY_BYTES`.
///
/// S-L4: Prevents arbitrarily large upstream error bodies from being echoed
/// verbatim in error messages. Control characters are stripped so crafted
/// responses can't inject escape sequences into logs.
async fn bounded_error_body(resp: reqwest::Response) -> String {
    let bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(_) => return "(failed to read error body)".to_string(),
    };
    let truncated = &bytes[..bytes.len().min(MAX_ERROR_BODY_BYTES)];
    String::from_utf8_lossy(truncated)
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect()
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
