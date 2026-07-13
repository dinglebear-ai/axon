//! Registry acquisition (fetch package metadata -> prepared dump) for
//! `axon source pkg:<registry>/<package>`.
//!
//! Mirrors [`crate::reddit_acquire`] / [`crate::youtube_acquire`]: classify the
//! target, fetch package metadata through the SSRF-guarded HTTP client
//! (size-capped so a hostile endpoint can't OOM us), map the raw registry API
//! JSON into the [`axon_adapters::registry_sources::dump::RegistryDump`] shape
//! the registry adapter reads, and write it to a **deterministic**,
//! target-derived cache path. The registry bridge
//! ([`crate::index_registry_source_with_job`]) then reads that
//! `registry_dump_path` — this helper does NOT parse the dump; the adapter does.
//!
//! This is a net-new acquisition path (there is no legacy npm/pypi ingest to
//! port): the raw-JSON → dump mapping is a pure function
//! ([`map::map_npm`] / [`map::map_pypi`] / [`map::map_crates`]) so it is
//! unit-testable with fixtures — with a round-trip test that reads the written
//! dump back through the *real* adapter reader
//! ([`axon_adapters::registry_sources::dump::RegistryDump::load`]).
//!
//! Target URLs never appear verbatim in errors — they are URL-redacted so a
//! hostile registry response can't leak an internal endpoint through the
//! surfaced message.

mod map;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use axon_adapters::registry_sources::dump::RegistryDump;
use axon_core::content::redact_url;
use axon_core::http::build_client;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};

/// Wall-clock cap for a single registry API request before it is aborted.
const FETCH_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum registry API response size (16 MiB). Package metadata (including a
/// bundled README) is bounded content; the cap defends against a hostile or
/// misconfigured endpoint streaming a multi-GB body.
const MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

/// crates.io requires a descriptive User-Agent (it 403s the default reqwest UA).
const REGISTRY_USER_AGENT: &str = "axon-source/1.0 (+https://github.com/jmagar/axon)";

/// The legacy `pkg:` prefix that marks a registry target.
const REGISTRY_PREFIX: &str = "pkg:";
const REGISTRY_URI_PREFIX: &str = "pkg://";

/// Registries whose package metadata this acquirer can fetch.
const REGISTRIES: &[&str] = &["npm", "pypi", "crates"];

/// True when `input` should route to the registry acquisition path.
///
/// Pure — string parsing only, no I/O. A registry target is a
/// `pkg:<registry>/<package>` string with a known registry and a non-empty
/// package name. Checked *before* the web branch (a `pkg:` string is not a
/// URL, so the web catch-all never matches it) alongside the other prefix
/// checks.
pub fn is_registry_target(input: &str) -> bool {
    parse_registry_target(input).is_ok()
}

/// Parse a registry target into `(registry, package)`.
///
/// Accepts the old CLI form (`pkg:<registry>/<package>`), the router
/// canonical URI (`pkg://<registry>/<package>`), and resolver shorthands
/// (`npm:<package>`, `pypi:<package>`, `crates:<package>`). Pure and I/O-free.
/// Errors name the bad component without echoing anything sensitive. The
/// `<package>` may itself contain `/` (npm scoped packages like
/// `@scope/name`), so only the FIRST `/` (separating registry from package) is
/// split.
pub fn parse_registry_target(input: &str) -> Result<(String, String), String> {
    let trimmed = input.trim();
    let (registry, package) = if let Some(rest) = trimmed.strip_prefix(REGISTRY_URI_PREFIX) {
        rest.split_once('/').ok_or_else(|| {
            "registry target is missing a `/<package>` after the registry".to_string()
        })?
    } else if let Some(rest) = trimmed.strip_prefix(REGISTRY_PREFIX) {
        rest.split_once('/').ok_or_else(|| {
            "registry target is missing a `/<package>` after the registry".to_string()
        })?
    } else if let Some((registry, package)) = trimmed.split_once(':') {
        (registry, package)
    } else {
        return Err(format!(
            "not a registry target (expected `{REGISTRY_PREFIX}<registry>/<package>`)"
        ));
    };
    let registry = registry.trim().to_ascii_lowercase();
    if !REGISTRIES.contains(&registry.as_str()) {
        return Err(format!(
            "unknown registry '{registry}' (expected one of npm/pypi/crates)"
        ));
    }
    let package = package.trim().to_string();
    if package.is_empty() {
        return Err("registry target has an empty package name".to_string());
    }
    Ok((registry, package))
}

/// Fetch `registry`/`package` metadata, map it into the prepared dump shape,
/// and write it to a **deterministic**, target-derived cache path.
///
/// The returned path is a stable function of the `(registry, package)` pair
/// (not a random temp name), mirroring the reddit/feed/youtube caches. Errors
/// are URL-redacted.
pub async fn fetch_registry_dump(registry: &str, package: &str) -> Result<PathBuf> {
    let registry = registry.trim().to_ascii_lowercase();
    let package = package.trim();
    if package.is_empty() {
        bail!("registry acquisition requires a non-empty package name");
    }

    let client = build_client(FETCH_TIMEOUT.as_secs(), Some(REGISTRY_USER_AGENT))
        .context("failed to build registry http client")?;

    let url = metadata_url(&registry, package)?;
    let value = fetch_registry_json(&client, &url).await?;
    let dump = map_dump(&registry, package, &value)?;

    let bytes = serde_json::to_vec(&dump).context("failed to serialize registry dump")?;
    let path = registry_cache_path(&registry, package);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("failed to create registry cache directory")?;
    }
    tokio::fs::write(&path, &bytes)
        .await
        .with_context(|| format!("failed to write registry dump for '{registry}/{package}'"))?;
    Ok(path)
}

/// Build the metadata endpoint URL for a `(registry, package)` pair.
///
/// The only path-segment character these registries permit that is not already
/// URL-safe is the `/` in an npm scoped package (`@scope/name`), which must be
/// percent-encoded to `%2F` so the whole scoped name stays a single path
/// segment. Every other legal package-name character (alphanumerics, `-`, `_`,
/// `.`, `@`) is URL-safe, so [`encode_package_segment`] only rewrites `/`.
fn metadata_url(registry: &str, package: &str) -> Result<String> {
    let encoded = encode_package_segment(package);
    let url = match registry {
        "npm" => format!("https://registry.npmjs.org/{encoded}"),
        "pypi" => format!("https://pypi.org/pypi/{encoded}/json"),
        "crates" => format!("https://crates.io/api/v1/crates/{encoded}"),
        other => bail!("unknown registry '{other}' (expected one of npm/pypi/crates)"),
    };
    Ok(url)
}

/// Percent-encode the one non-URL-safe character legal in a package name: the
/// `/` in an npm scoped package (`@scope/name` -> `@scope%2Fname`).
fn encode_package_segment(package: &str) -> String {
    package.replace('/', "%2F")
}

/// Dispatch the raw registry JSON to the matching pure mapper.
fn map_dump(registry: &str, package: &str, value: &serde_json::Value) -> Result<RegistryDump> {
    match registry {
        "npm" => map::map_npm(package, value),
        "pypi" => map::map_pypi(package, value),
        "crates" => map::map_crates(package, value),
        other => bail!("unknown registry '{other}' (expected one of npm/pypi/crates)"),
    }
}

/// Deterministic on-disk path for a registry target:
/// `<tmp>/axon-registry/<sha256(registry/pkg)>.json`.
///
/// Stability (same target -> same path) mirrors the reddit/feed/youtube caches.
/// The registry bridge derives the source id from the *dump path* (see
/// `registry_source_id`), so a stable path is what keeps re-fetches of the same
/// package mapping onto the same source id.
fn registry_cache_path(registry: &str, package: &str) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(format!("{registry}/{package}").as_bytes());
    let digest = hasher.finalize();
    let hash = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    std::env::temp_dir()
        .join("axon-registry")
        .join(format!("{hash}.json"))
}

/// GET a registry API URL, enforcing the size cap while streaming so a hostile
/// or misconfigured endpoint can't OOM us, and parse the JSON body.
async fn fetch_registry_json(client: &reqwest::Client, url: &str) -> Result<serde_json::Value> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|err| anyhow::anyhow!("registry fetch failed for {}: {err}", redact_url(url)))?
        .error_for_status()
        .map_err(|err| {
            anyhow::anyhow!(
                "registry endpoint returned error for {}: {err}",
                redact_url(url)
            )
        })?;

    if let Some(len) = resp.content_length()
        && len > MAX_RESPONSE_BYTES as u64
    {
        bail!(
            "registry response for {} advertises {len} bytes, exceeds {MAX_RESPONSE_BYTES} byte cap",
            redact_url(url)
        );
    }

    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| {
            anyhow::anyhow!(
                "registry fetch stream failed for {}: {err}",
                redact_url(url)
            )
        })?;
        if buf.len() + chunk.len() > MAX_RESPONSE_BYTES {
            bail!(
                "registry response for {} exceeds {MAX_RESPONSE_BYTES} byte cap while streaming",
                redact_url(url)
            );
        }
        buf.extend_from_slice(&chunk);
    }

    serde_json::from_slice(&buf).map_err(|err| {
        anyhow::anyhow!(
            "registry response for {} was not valid JSON: {err}",
            redact_url(url)
        )
    })
}

#[cfg(test)]
#[path = "registry_acquire_tests.rs"]
mod tests;
