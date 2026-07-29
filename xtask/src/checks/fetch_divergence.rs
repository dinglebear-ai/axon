//! Enforce that web acquisition goes through ONE fetch path.
//!
//! # Why
//!
//! "Pipeline unification" (#298) unified the job/ledger model but left the
//! acquisition layer fragmented: `scrape`, `map`, the sitemap/llms.txt probes,
//! the Spider crawl, and the extract verticals each built their own HTTP
//! client with their own user-agent, redirect policy, retry rules, and (non-)
//! handling of bot walls.
//!
//! The cost was measured on 2026-07-29: adding a TLS-impersonation retry to the
//! map path recovered four Akamai-fronted sites, while `axon scrape` on the
//! same hosts still fetched a 380-byte "Access Denied" page and dropped it as
//! thin content while reporting success. One fix, applied once, reached exactly
//! one surface.
//!
//! This check makes that class of drift a build failure rather than something
//! discovered months later by a user with zero mapped URLs.
//!
//! # The rule
//!
//! Inside the acquisition crates, constructing an HTTP client directly is a
//! violation. Use `axon_core::http::fetch_web` (the shared ladder: plain fetch
//! → wall classification → browser TLS impersonation → re-classification).
//!
//! Exceptions live in [`APPROVED_EXCEPTIONS`], each with a written reason.
//! Adding an entry is a deliberate, reviewable act — which is the point.

use anyhow::{Result, bail};
use std::path::Path;

/// Crates whose job is to pull bytes off the public web. Client construction
/// here must go through the shared ladder.
const ACQUISITION_ROOTS: &[&str] = &["crates/axon-adapters/src", "crates/axon-extract/src"];

/// Source patterns that construct an HTTP client.
const CLIENT_CONSTRUCTORS: &[&str] = &[
    "reqwest::Client::builder()",
    "reqwest::Client::new()",
    "build_ssrf_guarded_client_builder(",
    "build_client(",
    "build_client_no_redirect(",
    "wreq::Client::builder()",
];

/// Approved divergences: (repo-relative path, reason).
///
/// Every entry is a place where the shared ladder genuinely does not fit.
/// Adding one requires a reason a reviewer can evaluate — "it was easier" is
/// not one.
const APPROVED_EXCEPTIONS: &[(&str, &str)] = &[
    (
        "crates/axon-adapters/src/web_engine/engine/sitemap/discover.rs",
        "Builds the client handed to fetch_text_with_retry, a byte-capped \
         streaming reader (50 MB cap, mid-stream abort) that the String-returning \
         shared ladder cannot express. Passes axon_ua() and inherits the shared \
         10-hop SSRF-revalidating redirect policy.",
    ),
    (
        "crates/axon-adapters/src/web_engine/engine/sitemap/backfill.rs",
        "Builds the client handed to fetch_text_with_retry (byte-capped streaming \
         reader). Passes axon_ua() and inherits the shared redirect/SSRF policy.",
    ),
    (
        "crates/axon-adapters/src/web_engine/engine/llms_txt.rs",
        "Builds the client handed to fetch_text_with_retry (byte-capped streaming \
         reader). Passes axon_ua() and inherits the shared redirect/SSRF policy.",
    ),
    (
        "crates/axon-adapters/src/providers/http_fetch.rs",
        "FetchProvider: the acquire-lane provider boundary. Owns per-request \
         header/proxy configuration the shared ladder deliberately does not \
         expose. TRACKED for migration under axon_rust-w612x.",
    ),
    (
        "crates/axon-adapters/src/providers/searxng_search.rs",
        "Search-backend API client (SearXNG JSON), not page acquisition.",
    ),
    (
        "crates/axon-adapters/src/web_engine/scrape.rs",
        "Spider-based scrape plus a fallback reqwest client. TRACKED for \
         migration under axon_rust-w612x — this is the path that silently \
         captured an Akamai denial page.",
    ),
    (
        "crates/axon-adapters/src/reddit/acquire.rs",
        "Reddit OAuth API client. Reddit's API terms REQUIRE a descriptive, \
         bot-identifying User-Agent, which is the opposite of what the shared \
         browser-impersonating ladder sends. Talks to oauth.reddit.com JSON \
         endpoints, not arbitrary user-supplied hosts.",
    ),
    (
        "crates/axon-adapters/src/registry_sources/acquire.rs",
        "Package-registry API client (crates.io, npm, PyPI, …). crates.io policy \
         requires bot identification, and these are fixed JSON endpoints rather \
         than arbitrary web pages, so a bot wall is not a failure mode here.",
    ),
    (
        "crates/axon-adapters/src/feed/acquire.rs",
        "RSS/Atom fetch. NOT a settled exception: it hits arbitrary \
         user-supplied hosts with the bot-identifying UA \"axon-feed\" and has \
         no wall handling, so a Cloudflare-fronted feed fails the same silent \
         way scrape did. TRACKED for migration under axon_rust-w612x.",
    ),
];

fn is_exception(rel: &str) -> Option<&'static str> {
    APPROVED_EXCEPTIONS
        .iter()
        .find(|(path, _)| *path == rel)
        .map(|(_, reason)| *reason)
}

/// True for paths whose client construction is not web acquisition at all.
fn is_ignored(rel: &str) -> bool {
    rel.contains("/tests/") || rel.ends_with("_tests.rs") || rel.ends_with("/testing.rs")
}

fn collect_rs(dir: &Path, root: &Path, out: &mut Vec<String>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_rs(&path, root, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
    Ok(())
}

pub fn check(root: &Path) -> Result<()> {
    let mut files = Vec::new();
    for acq_root in ACQUISITION_ROOTS {
        collect_rs(&root.join(acq_root), root, &mut files)?;
    }
    files.sort();

    let mut violations: Vec<String> = Vec::new();
    let mut used_exceptions: Vec<&str> = Vec::new();

    for rel in &files {
        if is_ignored(rel) {
            continue;
        }
        let body = std::fs::read_to_string(root.join(rel))?;
        let mut hits: Vec<(usize, &str)> = Vec::new();
        for (idx, line) in body.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") {
                continue;
            }
            for pat in CLIENT_CONSTRUCTORS {
                if line.contains(pat) {
                    hits.push((idx + 1, pat));
                }
            }
        }
        if hits.is_empty() {
            continue;
        }
        if is_exception(rel).is_some() {
            used_exceptions.push(rel);
            continue;
        }
        for (line_no, pat) in hits {
            violations.push(format!("  {rel}:{line_no} constructs a client via `{pat}`"));
        }
    }

    // A stale exception is drift too: it advertises a divergence that no longer
    // exists, which makes the list untrustworthy as documentation.
    let stale: Vec<&str> = APPROVED_EXCEPTIONS
        .iter()
        .map(|(p, _)| *p)
        .filter(|p| !used_exceptions.contains(p))
        .filter(|p| root.join(p).exists())
        .collect();

    if violations.is_empty() && stale.is_empty() {
        println!(
            "OK: web acquisition is unified — {} approved exception(s), no unsanctioned clients.",
            APPROVED_EXCEPTIONS.len()
        );
        return Ok(());
    }

    let mut msg = String::new();
    if !violations.is_empty() {
        msg.push_str(
            "Unsanctioned HTTP client construction in an acquisition crate.\n\n\
             Web acquisition must go through `axon_core::http::fetch_web`, which owns the\n\
             shared ladder: plain fetch -> bot-wall classification -> browser TLS\n\
             impersonation -> re-classification. Building a client directly means any future\n\
             acquisition fix silently skips this surface.\n\n",
        );
        msg.push_str(&violations.join("\n"));
        msg.push_str(
            "\n\nEither route the call through fetch_web, or add the file to\n\
             APPROVED_EXCEPTIONS in xtask/src/checks/fetch_divergence.rs with a reason.\n",
        );
    }
    if !stale.is_empty() {
        msg.push_str(&format!(
            "\nStale APPROVED_EXCEPTIONS entries (file no longer constructs a client):\n  {}\n\
             Remove them so the list keeps documenting reality.\n",
            stale.join("\n  ")
        ));
    }
    bail!(msg)
}

#[cfg(test)]
#[path = "fetch_divergence_tests.rs"]
mod tests;
