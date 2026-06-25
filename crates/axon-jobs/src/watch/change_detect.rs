//! Detect whether one watched URL changed and persist its new snapshot.
//!
//! Flow: conditional probe (304 = unchanged) → scrape → normalize + ignore
//! filter → fast-equal hash skip → reuse `axon_api::diff::compute_diff` →
//! threshold. First-seen is forced Changed (seed). DB read errors preserve the
//! existing row (no phantom upsert); transient probe/scrape errors stamp
//! `last_checked_at` only.

use crate::store::now_ms;
use crate::watch::filter::{apply_ignore, content_hash, normalize_markdown};
use crate::watch::url_state::{UrlState, get_url_state, upsert_url_state};
use async_trait::async_trait;
use axon_api::diff::{DiffResult, DiffStatus, LinkEntry};
use axon_api::diff::{compute_diff, extract_links_from_payload};
use axon_api::job_dto::ScrapeResult;
use axon_core::config::Config;
use axon_core::http::{Probe, conditional_probe};
use regex::Regex;
use sqlx::SqlitePool;
use uuid::Uuid;

/// Probe/scrape injection seam — enables offline deterministic testing of
/// [`detect_url_change`] without live HTTP access.
///
/// The production path uses [`LiveFetcher`]. Tests inject a stub.
#[async_trait]
pub(crate) trait WatchFetcher: Send + Sync {
    async fn probe(&self, url: &str, etag: Option<&str>, last_modified: Option<&str>) -> Probe;
    /// Returns `Ok(result)` on success or `Err(message)` on failure (the error
    /// message is already prefixed with "scrape failed: " by convention).
    async fn scrape_url(&self, cfg: &Config, url: &str) -> Result<ScrapeResult, String>;
}

/// Live production fetcher — delegates to the real HTTP stack.
pub(crate) struct LiveFetcher;

#[async_trait]
impl WatchFetcher for LiveFetcher {
    async fn probe(&self, url: &str, etag: Option<&str>, lm: Option<&str>) -> Probe {
        conditional_probe(url, etag, lm).await
    }
    async fn scrape_url(&self, cfg: &Config, url: &str) -> Result<ScrapeResult, String> {
        axon_extract::scrape::scrape(cfg, url, None)
            .await
            .map_err(|e| format!("scrape failed: {e}"))
    }
}

/// Outcome of detecting a change for one URL. Enum so illegal states (e.g.
/// "meaningful but no diff") are unrepresentable.
#[derive(Debug, Clone)]
pub enum UrlOutcome {
    /// No meaningful change (304, sub-threshold, or hash-equal).
    Unchanged,
    /// A meaningful change was detected; carries the diff for summary/artifact.
    Changed { diff: DiffResult },
    /// The probe/scrape/persist failed; carries the error message.
    Failed { error: String },
}

/// Hash the fast-equal snapshot input: filtered markdown joined with the
/// serialized links snapshot. Including links means a link-only change (identical
/// visible markdown) still changes the hash and is not short-circuited as
/// Unchanged before `compute_diff` runs.
fn snapshot_hash(filtered: &str, links_json: &str) -> String {
    content_hash(&format!("{filtered}\n{links_json}"))
}

/// A change is meaningful if content changed AND (links changed OR the word-count
/// delta clears the threshold). Links always count.
pub fn is_meaningful(diff: &DiffResult, threshold_words: i64) -> bool {
    if !matches!(diff.status, DiffStatus::Changed) {
        return false;
    }
    if !diff.links_added.is_empty() || !diff.links_removed.is_empty() {
        return true;
    }
    diff.word_count_delta.abs() >= threshold_words.max(0)
}

/// Persist a "checked but unchanged" snapshot: clone the prior state (or a fresh
/// default when first-seen with no prior row), stamp `last_checked_at`, and
/// upsert. Best-effort — a write failure is logged but does not change the
/// returned outcome. Folds the repeated NotModified/Failed/scrape-fail paths.
async fn persist_unchanged(
    pool: &SqlitePool,
    watch_id: Uuid,
    url: &str,
    prior: Option<&UrlState>,
    now: i64,
) {
    let mut s = prior.cloned().unwrap_or_default();
    s.last_checked_at = Some(now);
    if let Err(e) = upsert_url_state(pool, watch_id, url, &s).await {
        tracing::warn!(%watch_id, url, error = %e, "watch: upsert_url_state (unchanged) failed");
    }
}

/// Public entry point — uses the live HTTP stack.
pub async fn detect_url_change(
    cfg: &Config,
    pool: &SqlitePool,
    watch_id: Uuid,
    url: &str,
    ignore: &[Regex],
    threshold_words: i64,
) -> UrlOutcome {
    detect_url_change_with(
        &LiveFetcher,
        cfg,
        pool,
        watch_id,
        url,
        ignore,
        threshold_words,
    )
    .await
}

/// Testable core — accepts any [`WatchFetcher`] so stubs can be injected.
pub(crate) async fn detect_url_change_with(
    fetcher: &impl WatchFetcher,
    cfg: &Config,
    pool: &SqlitePool,
    watch_id: Uuid,
    url: &str,
    ignore: &[Regex],
    threshold_words: i64,
) -> UrlOutcome {
    // A DB read error must NOT be collapsed into a blank prior: doing so would
    // upsert a full blank row over a just-saved snapshot and re-trigger crawls.
    // Skip persistence entirely and surface the error.
    let prior = match get_url_state(pool, watch_id, url).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(%watch_id, url, error = %e, "watch: get_url_state failed; skipping URL");
            return UrlOutcome::Failed {
                error: format!("read prior state failed: {e}"),
            };
        }
    };
    let now = now_ms();

    // 1) Conditional probe.
    let (etag, last_modified) = match fetcher
        .probe(
            url,
            prior.as_ref().and_then(|p| p.etag.as_deref()),
            prior.as_ref().and_then(|p| p.last_modified.as_deref()),
        )
        .await
    {
        Probe::NotModified => {
            persist_unchanged(pool, watch_id, url, prior.as_ref(), now).await;
            return UrlOutcome::Unchanged;
        }
        Probe::Failed(msg) => {
            // The probe uses the shared `http_client()`, which does NOT carry the
            // watch's configured custom headers — a header-gated page (401/403)
            // or a transient probe-only failure would otherwise permanently block
            // seeding/detection. Fall through to the scrape path (which DOES use
            // configured headers via `services::scrape`); if that scrape also fails
            // we return Failed below. Preserve the prior validators rather than
            // wiping them — a transient probe failure must not discard the stored
            // ETag/Last-Modified that the next conditional probe will reuse.
            tracing::warn!(%watch_id, url, error = %msg, "watch: conditional probe failed; falling back to scrape");
            (
                prior.as_ref().and_then(|p| p.etag.clone()),
                prior.as_ref().and_then(|p| p.last_modified.clone()),
            )
        }
        Probe::Modified {
            etag,
            last_modified,
        } => (etag, last_modified),
    };

    // 2) Scrape + 3) filter.
    let scraped = match fetcher.scrape_url(cfg, url).await {
        Ok(r) => r,
        Err(msg) => {
            persist_unchanged(pool, watch_id, url, prior.as_ref(), now).await;
            return UrlOutcome::Failed { error: msg };
        }
    };
    let filtered = apply_ignore(&normalize_markdown(&scraped.markdown), ignore);
    let fresh_links = extract_links_from_payload(&scraped.payload);
    let fresh_links_json = serde_json::to_string(&fresh_links).unwrap_or_else(|_| "[]".into());
    // Hash filtered markdown AND the serialized links so a link-only change (same
    // visible markdown, different anchors) changes the hash and proceeds to
    // `compute_diff` — where the "links always count" rule applies. Hashing the
    // markdown alone would short-circuit such changes as Unchanged. Prior
    // snapshots stored a markdown-only hash, so they re-seed once (acceptable).
    let fresh_hash = snapshot_hash(&filtered, &fresh_links_json);

    let prior_hash = prior.as_ref().and_then(|p| p.content_hash.clone());
    let prior_changed_at = prior.as_ref().and_then(|p| p.last_changed_at);

    // 4) Fast-equal skip.
    if prior_hash.as_deref() == Some(fresh_hash.as_str()) {
        let s = UrlState {
            etag,
            last_modified,
            content_hash: Some(fresh_hash),
            last_markdown: Some(filtered),
            last_links_json: Some(fresh_links_json),
            last_checked_at: Some(now),
            last_changed_at: prior_changed_at,
            last_crawl_job_id: prior.as_ref().and_then(|p| p.last_crawl_job_id),
        };
        if let Err(e) = upsert_url_state(pool, watch_id, url, &s).await {
            tracing::warn!(%watch_id, url, error = %e, "watch: upsert_url_state (hash-equal) failed");
        }
        return UrlOutcome::Unchanged;
    }

    // 5–7) Diff vs prior snapshot, apply threshold, persist, and classify.
    let fresh = FreshSnapshot {
        etag,
        last_modified,
        filtered,
        fresh_links,
        fresh_links_json,
        fresh_hash,
    };
    finalize_changed(
        pool,
        watch_id,
        url,
        prior.as_ref(),
        now,
        threshold_words,
        fresh,
    )
    .await
}

/// Freshly-scraped snapshot inputs threaded from [`detect_url_change`] into
/// [`finalize_changed`]. Bundling keeps the helper signature small.
struct FreshSnapshot {
    etag: Option<String>,
    last_modified: Option<String>,
    filtered: String,
    fresh_links: Vec<LinkEntry>,
    fresh_links_json: String,
    fresh_hash: String,
}

/// Steps 5–7: diff the fresh snapshot against the prior one (first-seen forces
/// Changed to seed), apply the meaningfulness threshold, persist the new
/// snapshot, and return the classified outcome.
async fn finalize_changed(
    pool: &SqlitePool,
    watch_id: Uuid,
    url: &str,
    prior: Option<&UrlState>,
    now: i64,
    threshold_words: i64,
    fresh: FreshSnapshot,
) -> UrlOutcome {
    let prior_hash = prior.and_then(|p| p.content_hash.clone());
    let prior_changed_at = prior.and_then(|p| p.last_changed_at);

    // 5) Diff: prior snapshot vs fresh. First-seen → force Changed (seed).
    let prior_md = prior
        .and_then(|p| p.last_markdown.clone())
        .unwrap_or_default();
    let prior_links: Vec<LinkEntry> = prior
        .and_then(|p| p.last_links_json.as_deref())
        .and_then(|j| serde_json::from_str(j).ok())
        .unwrap_or_default();
    let empty = serde_json::json!({});
    let mut diff = compute_diff(
        url,
        &prior_md,
        &prior_links,
        &empty,
        url,
        &fresh.filtered,
        &fresh.fresh_links,
        &empty,
    );
    let first_seen = prior_hash.is_none();
    if first_seen {
        diff.status = DiffStatus::Changed;
    }

    // 6) Threshold.
    let meaningful = first_seen || is_meaningful(&diff, threshold_words);

    // 7) Persist snapshot.
    let s = UrlState {
        etag: fresh.etag,
        last_modified: fresh.last_modified,
        content_hash: Some(fresh.fresh_hash),
        last_markdown: Some(fresh.filtered),
        last_links_json: Some(fresh.fresh_links_json),
        last_checked_at: Some(now),
        last_changed_at: if meaningful {
            Some(now)
        } else {
            prior_changed_at
        },
        last_crawl_job_id: prior.and_then(|p| p.last_crawl_job_id),
    };
    if let Err(e) = upsert_url_state(pool, watch_id, url, &s).await {
        tracing::warn!(%watch_id, url, error = %e, "watch: upsert_url_state (changed) failed");
    }

    if meaningful {
        UrlOutcome::Changed { diff }
    } else {
        UrlOutcome::Unchanged
    }
}

#[cfg(test)]
#[path = "change_detect_tests.rs"]
mod tests;
