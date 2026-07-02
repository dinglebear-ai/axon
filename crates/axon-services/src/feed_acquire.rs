//! Feed acquisition (network fetch) for `axon source <feed-url>`.
//!
//! Mirrors [`crate::git_acquire`]: SSRF-validate the target, fetch the raw feed
//! bytes through the SSRF-guarded HTTP client (streaming, size-capped so a
//! hostile endpoint can't OOM us), and write them to a **deterministic**,
//! feed-URL-derived cache path. The feed bridge
//! ([`crate::index_feed_source_with_job`]) then reads that prepared path — this
//! helper does NOT parse the feed; the `axon_adapters::feed` adapter does that
//! from `feed_path`. The path must be stable per URL because the bridge derives
//! the source id from it.
//!
//! Kept dependency-free of the legacy `axon-ingest` crate. Fetch-URL derivation
//! is a pure function ([`normalize_feed_target`]) so callers can assert the
//! resolved target without spawning a request.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use axon_core::content::redact_url;
use axon_core::http::{build_client, validate_url};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};

use crate::feed_target::normalize_feed_target;

/// Wall-clock cap for a single feed fetch before it is aborted.
const FETCH_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum feed document size. Feeds are link/entry indexes, not content dumps,
/// so 16 MiB is generous; the cap defends against a hostile or misconfigured
/// endpoint streaming a multi-GB body.
const MAX_FEED_BYTES: usize = 16 * 1024 * 1024;

/// SSRF-validate `feed_input`, fetch the raw feed document, and write it to a
/// **deterministic**, feed-URL-derived cache path.
///
/// `feed_input` may carry an `rss:`/`feed:`/`atom:` prefix; it is normalized to
/// the real https URL before validation and fetch. The URL is SSRF-validated
/// before any request is sent, and the fetch itself goes through the
/// SSRF-guarded HTTP client so a redirect or DNS-rebind to a private/metadata
/// address is blocked at connect time.
///
/// The returned path is a stable function of the feed URL (not a random temp
/// name): the feed bridge derives the source id from this path, so the same
/// feed URL must map to the same path across runs for generation/manifest-diff
/// refresh to work. The file content is overwritten with fresh bytes each run.
/// Fetch/write errors are URL-redacted before being surfaced.
pub async fn fetch_feed_to_file(feed_input: &str) -> Result<PathBuf> {
    let feed_url = normalize_feed_target(feed_input);
    validate_url(&feed_url)
        .map_err(|err| anyhow::anyhow!("refusing to fetch {}: {err}", redact_url(&feed_url)))?;

    let bytes = fetch_feed_bytes(&feed_url).await?;

    let path = feed_cache_path(&feed_url);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("failed to create feed cache directory")?;
    }
    tokio::fs::write(&path, &bytes).await.with_context(|| {
        format!(
            "failed to write feed document for {}",
            redact_url(&feed_url)
        )
    })?;
    Ok(path)
}

/// Deterministic on-disk path for a feed URL: `<tmp>/axon-feeds/<sha256>.xml`.
///
/// Stability (same URL -> same path) is load-bearing: the feed bridge hashes
/// the canonicalized `feed_path` to form the source id, so a random temp name
/// would make every run a brand-new source and defeat refresh/dedup.
fn feed_cache_path(feed_url: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(feed_url.as_bytes());
    let digest = hasher.finalize();
    let hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    std::env::temp_dir()
        .join("axon-feeds")
        .join(format!("{hash}.xml"))
}

/// Fetch the feed document, enforcing the size cap while streaming so a hostile
/// or misconfigured endpoint can't OOM us by returning a multi-GB body.
async fn fetch_feed_bytes(feed_url: &str) -> Result<Vec<u8>> {
    // Use the shared SSRF-guarded client: its DNS resolver re-validates the
    // resolved IP on every hop, so a redirect or DNS-rebind from a public host
    // to a private/metadata address (169.254.169.254, loopback, RFC1918) is
    // refused at connect time — the parse-time `validate_url` above only checks
    // the literal input URL.
    let client = build_client(FETCH_TIMEOUT.as_secs(), Some("axon-feed"))
        .context("failed to build feed http client")?;

    let resp = client
        .get(feed_url)
        .send()
        .await
        .map_err(|err| anyhow::anyhow!("feed fetch failed for {}: {err}", redact_url(feed_url)))?
        .error_for_status()
        .map_err(|err| {
            anyhow::anyhow!(
                "feed endpoint returned error for {}: {err}",
                redact_url(feed_url)
            )
        })?;

    // Reject early when the server advertises an over-cap body.
    if let Some(len) = resp.content_length()
        && len > MAX_FEED_BYTES as u64
    {
        bail!(
            "feed at {} advertises {len} bytes, exceeds {MAX_FEED_BYTES} byte cap",
            redact_url(feed_url)
        );
    }

    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| {
            anyhow::anyhow!(
                "feed fetch stream failed for {}: {err}",
                redact_url(feed_url)
            )
        })?;
        if buf.len() + chunk.len() > MAX_FEED_BYTES {
            bail!(
                "feed at {} exceeds {MAX_FEED_BYTES} byte cap while streaming",
                redact_url(feed_url)
            );
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

#[cfg(test)]
#[path = "feed_acquire_tests.rs"]
mod tests;
