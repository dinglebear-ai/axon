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

/// Source patterns that obtain an HTTP client for a direct fetch.
///
/// `http_client()` is included deliberately. An earlier version of this check
/// matched only *construction*, which made it blind to the ~25 call sites that
/// take the shared singleton and then do their own `.get().send()` with no wall
/// handling — the single largest class of real divergence in the tree, sitting
/// inside the roots this check already scanned.
const CLIENT_CONSTRUCTORS: &[&str] = &[
    "http_client()",
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
        "crates/axon-adapters/src/artifact_candidates/depot.rs",
        "Authenticated Depot write API client on one configured provider origin. \
         It posts typed candidate JSON with bearer auth, disables redirects, and \
         must not use the browser-impersonating public-page acquisition ladder.",
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
        "crates/axon-adapters/src/registry_sources/skills_sh/fetch.rs",
        "Authenticated skills.sh structured JSON API client on a fixed provider \
         origin. Requires a short-lived Vercel OIDC bearer token plus provider- \
         specific 429/auth/response-bound handling; browser-style web acquisition \
         and impersonation are deliberately not appropriate for this surface.",
    ),
    (
        "crates/axon-adapters/src/registry_sources/skills_sh/audit.rs",
        "Authenticated skills.sh audit-metadata JSON API on the same fixed \
         provider origin. Requires the Vercel OIDC bearer token and bounded, \
         provider-specific status handling rather than arbitrary page fetching.",
    ),
    (
        "crates/axon-adapters/src/feed/acquire.rs",
        "RSS/Atom fetch. NOT a settled exception: it hits arbitrary \
         user-supplied hosts with the bot-identifying UA \"axon-feed\" and has \
         no wall handling, so a Cloudflare-fronted feed fails the same silent \
         way scrape did. TRACKED for migration under axon_rust-w612x.",
    ),
];

/// Vertical extractors that take the shared `http_client()` singleton and do
/// their own `.get().send()`.
///
/// All target FIXED, known hosts (not arbitrary user-supplied URLs), which is
/// why they are tolerable today. But several scrape HTML from hosts that
/// actively bot-wall, and none of them classify a wall: `ebay.rs` already
/// reimplements a narrower, worse check (`403 | 503` status only, no body
/// fingerprint, no escalation), which is direct evidence the need is real and
/// that leaving these unmigrated invites more one-off copies.
///
/// TRACKED for migration under `axon_rust-w612x`. Listed individually rather
/// than by glob so the inventory stays explicit and a NEW vertical fails the
/// check until someone decides which side it belongs on.
const TRACKED_SHARED_CLIENT_FETCHERS: &[&str] = &[
    "crates/axon-adapters/src/web_engine/engine/runtime.rs",
    "crates/axon-extract/src/verticals/amazon.rs",
    "crates/axon-extract/src/verticals/arxiv.rs",
    "crates/axon-extract/src/verticals/crates_io.rs",
    "crates/axon-extract/src/verticals/dev_to.rs",
    "crates/axon-extract/src/verticals/docker_hub.rs",
    "crates/axon-extract/src/verticals/docs_rs.rs",
    "crates/axon-extract/src/verticals/ebay.rs",
    "crates/axon-extract/src/verticals/github_issue.rs",
    "crates/axon-extract/src/verticals/github_pr.rs",
    "crates/axon-extract/src/verticals/github_release.rs",
    "crates/axon-extract/src/verticals/github_repo.rs",
    "crates/axon-extract/src/verticals/hackernews.rs",
    "crates/axon-extract/src/verticals/huggingface_model.rs",
    "crates/axon-extract/src/verticals/npm.rs",
    "crates/axon-extract/src/verticals/pypi.rs",
    "crates/axon-extract/src/verticals/reddit.rs",
    "crates/axon-extract/src/verticals/shopify.rs",
    "crates/axon-extract/src/verticals/stackoverflow.rs",
];

const TRACKED_REASON: &str = "Fixed-host vertical/engine fetch on the shared client with no wall \
     classification. TRACKED for migration under axon_rust-w612x.";

fn is_exception(rel: &str) -> Option<&'static str> {
    if TRACKED_SHARED_CLIENT_FETCHERS.contains(&rel) {
        return Some(TRACKED_REASON);
    }
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
        .chain(TRACKED_SHARED_CLIENT_FETCHERS.iter().copied())
        .filter(|p| !used_exceptions.contains(p))
        .filter(|p| root.join(p).exists())
        .collect();

    if violations.is_empty() && stale.is_empty() {
        println!(
            "OK: {} settled exception(s), {} tracked-for-migration fetcher(s), \
             no unlisted acquisition clients.",
            APPROVED_EXCEPTIONS.len(),
            TRACKED_SHARED_CLIENT_FETCHERS.len()
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
