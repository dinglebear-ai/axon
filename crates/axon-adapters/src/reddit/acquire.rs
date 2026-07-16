//! Reddit acquisition (OAuth + fetch -> prepared dump) for `axon source
//! r/<name>` / `axon source <thread-url>`.
//!
//! Mirrors [`crate::feed_acquire`]: authenticate, fetch through the SSRF-guarded
//! HTTP client (size-capped so a hostile endpoint can't OOM us), map the raw
//! Reddit API JSON into the prepared dump shape the `axon_adapters::reddit`
//! adapter reads, and write it to a **deterministic**, target-derived cache
//! path. The shared source pipeline then reads
//! that `reddit_dump_path` — this helper does NOT parse the dump; the adapter
//! does.
//!
//! Kept dependency-free of the legacy `axon-ingest` crate: the minimal OAuth +
//! listing/thread fetch logic is ported here, and the raw-JSON → dump mapping is
//! a pure function ([`map::map_subreddit_listing`] / [`map::map_thread`]) so it
//! is unit-testable with fixtures, no network.
//!
//! Tokens and credentials never appear in errors — auth failures are reported
//! generically and fetch errors are URL-redacted.

mod map;

use std::path::PathBuf;
use std::time::Duration;

use super::{RedditTarget, parse_reddit_target};
use anyhow::{Context, Result, bail};
use axon_core::content::redact_url;
use axon_core::http::build_client;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};

use self::map::{DumpItem, map_subreddit_listing, map_thread};

/// Wall-clock cap for a single Reddit API request before it is aborted.
const FETCH_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum Reddit API response size. Listings/threads are bounded content, so
/// 16 MiB is generous; the cap defends against a hostile or misconfigured
/// endpoint streaming a multi-GB body.
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

/// Reddit requires a descriptive User-Agent for API access.
const REDDIT_USER_AGENT: &str = "axon-source/1.0 by /u/axon_bot";

/// Number of posts to fetch from a subreddit listing (single page, hot sort).
const SUBREDDIT_POST_LIMIT: u32 = 100;

/// Comment-tree depth requested for thread fetches.
const THREAD_COMMENT_DEPTH: u32 = 10;

/// Classify `target`, OAuth-authenticate against Reddit, fetch the subreddit
/// listing (hot) or thread, map the response into the prepared dump shape, and
/// write it to a **deterministic**, target-derived cache path.
///
/// The returned path is a stable function of the target string (not a random
/// temp name). `REDDIT_CLIENT_ID` / `REDDIT_CLIENT_SECRET` must be set in the
/// environment; a missing pair is a clear, actionable error surfaced *before*
/// any network call. Auth and fetch errors are credential-/URL-redacted.
pub async fn fetch_reddit_dump(target: &str) -> Result<PathBuf> {
    let parsed = parse_reddit_target(target)
        .map_err(|err| anyhow::anyhow!("invalid reddit target '{target}': {err}"))?;

    let (client_id, client_secret) = reddit_credentials()?;
    let client = build_client(FETCH_TIMEOUT.as_secs(), Some(REDDIT_USER_AGENT))
        .context("failed to build reddit http client")?;

    let token = get_access_token(&client, &client_id, &client_secret).await?;

    let items = match &parsed {
        RedditTarget::Subreddit(name) => fetch_subreddit(&client, &token, name).await?,
        RedditTarget::Thread(permalink) => fetch_thread(&client, &token, permalink).await?,
    };

    let bytes = serde_json::to_vec(&items).context("failed to serialize reddit dump")?;
    let path = reddit_cache_path(target);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("failed to create reddit cache directory")?;
    }
    tokio::fs::write(&path, &bytes)
        .await
        .with_context(|| format!("failed to write reddit dump for target '{target}'"))?;
    Ok(path)
}

/// Read `REDDIT_CLIENT_ID` / `REDDIT_CLIENT_SECRET`, returning a clear,
/// actionable error when either is unset or blank. Secret values are never
/// echoed.
fn reddit_credentials() -> Result<(String, String)> {
    resolve_reddit_credentials(
        non_empty_env("REDDIT_CLIENT_ID"),
        non_empty_env("REDDIT_CLIENT_SECRET"),
    )
}

/// Pure credential resolution — decoupled from the environment so the
/// missing-credentials guard is testable without mutating process env (this
/// crate denies `unsafe`, which `std::env::set_var` now requires). Blank/absent
/// values are already collapsed to `None` by the caller.
fn resolve_reddit_credentials(
    client_id: Option<String>,
    client_secret: Option<String>,
) -> Result<(String, String)> {
    match (client_id, client_secret) {
        (Some(id), Some(secret)) => Ok((id, secret)),
        _ => bail!(
            "reddit acquisition requires REDDIT_CLIENT_ID and REDDIT_CLIENT_SECRET \
             (set them in ~/.axon/.env or the environment)"
        ),
    }
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Deterministic on-disk path for a reddit target:
/// `<tmp>/axon-reddit/<sha256(target)>.json`.
///
/// Stability (same target -> same path) mirrors the feed cache. The reddit
/// bridge derives the source id from the *target* (not the path), but a stable
/// path still avoids leaking a fresh temp file per run and keeps the dump
/// inspectable/reproducible.
fn reddit_cache_path(target: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(target.trim().as_bytes());
    let digest = hasher.finalize();
    let hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    std::env::temp_dir()
        .join("axon-reddit")
        .join(format!("{hash}.json"))
}

/// OAuth2 client-credentials grant against `www.reddit.com`. The token is never
/// logged; auth failures report a generic message with no credential material.
async fn get_access_token(
    client: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
) -> Result<String> {
    let resp = client
        .post("https://www.reddit.com/api/v1/access_token")
        .basic_auth(client_id, Some(client_secret))
        .form(&[("grant_type", "client_credentials")])
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("reddit oauth request failed"))?;
    let status = resp.status();
    let value: serde_json::Value = resp
        .json()
        .await
        .map_err(|_| anyhow::anyhow!("reddit oauth returned an unparseable response"))?;
    value["access_token"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| {
            let reason = value["error"].as_str().unwrap_or("unknown");
            anyhow::anyhow!("reddit oauth failed (status {status}): {reason}")
        })
}

/// Fetch a single hot-listing page for `r/<name>` and map it to dump items.
async fn fetch_subreddit(
    client: &reqwest::Client,
    token: &str,
    name: &str,
) -> Result<Vec<DumpItem>> {
    let url =
        format!("https://oauth.reddit.com/r/{name}/hot?limit={SUBREDDIT_POST_LIMIT}&raw_json=1");
    let value = fetch_reddit_json(client, token, &url).await?;
    Ok(map_subreddit_listing(&value))
}

/// Fetch a thread's `.json` (post + comment tree) and map it to a single dump
/// item with flattened comments.
async fn fetch_thread(
    client: &reqwest::Client,
    token: &str,
    permalink: &str,
) -> Result<Vec<DumpItem>> {
    let clean = permalink.trim_end_matches('/');
    let url = format!(
        "https://oauth.reddit.com{clean}.json?limit=100&depth={THREAD_COMMENT_DEPTH}&raw_json=1"
    );
    let value = fetch_reddit_json(client, token, &url).await?;
    match map_thread(&value) {
        Some(item) => Ok(vec![item]),
        None => bail!("reddit thread {} returned no post data", redact_url(&url)),
    }
}

/// GET a Reddit API URL with bearer auth, enforcing the size cap while streaming
/// so a hostile or misconfigured endpoint can't OOM us, and parse the JSON body.
async fn fetch_reddit_json(
    client: &reqwest::Client,
    token: &str,
    url: &str,
) -> Result<serde_json::Value> {
    let resp = client
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|err| anyhow::anyhow!("reddit fetch failed for {}: {err}", redact_url(url)))?
        .error_for_status()
        .map_err(|err| {
            anyhow::anyhow!(
                "reddit endpoint returned error for {}: {err}",
                redact_url(url)
            )
        })?;

    if let Some(len) = resp.content_length()
        && len > MAX_RESPONSE_BYTES as u64
    {
        bail!(
            "reddit response for {} advertises {len} bytes, exceeds {MAX_RESPONSE_BYTES} byte cap",
            redact_url(url)
        );
    }

    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| {
            anyhow::anyhow!("reddit fetch stream failed for {}: {err}", redact_url(url))
        })?;
        if buf.len() + chunk.len() > MAX_RESPONSE_BYTES {
            bail!(
                "reddit response for {} exceeds {MAX_RESPONSE_BYTES} byte cap while streaming",
                redact_url(url)
            );
        }
        buf.extend_from_slice(&chunk);
    }

    serde_json::from_slice(&buf).map_err(|err| {
        anyhow::anyhow!(
            "reddit response for {} was not valid JSON: {err}",
            redact_url(url)
        )
    })
}

#[cfg(test)]
#[path = "acquire_tests.rs"]
mod tests;
