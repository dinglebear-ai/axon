//! Conditional re-crawl (ETag / If-Modified-Since) plumbing — bead axon_rust-hiyf.
//!
//! ## The correctness crux
//!
//! spider 2.51 implements conditional requests *internally*: when its per-`Website`
//! [`spider::utils::etag_cache::ETagCache`] holds validators for a URL, it sends
//! `If-None-Match` / `If-Modified-Since`, and on a `304 Not Modified` it returns
//! `Default::default()` — the page **never enters the broadcast stream**. There is
//! no bodyless page for axon's collector to intercept; the URL simply vanishes.
//!
//! Left alone, that means an unchanged page would be silently dropped from the
//! manifest on every re-crawl — losing content while appearing to save bandwidth.
//! This module closes that gap by *reconciling* the drop: after the crawl we
//! re-emit the previous manifest entry for every URL that (a) we seeded validators
//! for and (b) did not arrive in this crawl. That set is exactly spider's 304
//! skips (a URL with no validators yields empty conditional headers, is fetched
//! normally, and therefore *arrives*).
//!
//! ## Why the reconciliation set is gated on spider's *visited* set
//!
//! A page that is no longer discovered (deleted, or unlinked from the crawl
//! graph) must NOT be reconciled — re-emitting its old manifest entry would
//! resurrect stale content as `changed=false`. Spider's 304 short-circuit runs
//! *inside* the per-URL page-fetch task, so a 304-skipped URL is recorded in
//! spider's `links_visited` set, whereas a URL that was never scheduled this run
//! is absent from it. The reconciliation set is therefore
//! `{ url ∈ previous_manifest : url ∈ seeded_sidecar
//!      AND url ∉ arrived_urls AND url ∈ visited_urls }`,
//! where `visited_urls` is `Website::get_links()` canonicalized into the manifest
//! key space. That is exactly spider's set of silent 304 skips, and it excludes
//! no-longer-discovered pages.
//!
//! Only the *safe* residual remains: a URL spider actually visited and got a 304
//! for is genuinely unchanged, so reuse is correct. Spider caps `links_visited`
//! at `LINKS_VISITED_MEMORY_LIMIT`; on an enormous crawl an overflowed 304 URL
//! falls out of `visited_urls` and is simply re-fetched next run (lost bandwidth
//! benefit, never zombie content). With no working seed the set is empty → no
//! reconciliation.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use spider::website::Website;
use tokio::io::AsyncWriteExt;

use crate::crawl::manifest::ManifestEntry;

fn is_zero(v: &u8) -> bool {
    *v == 0
}

/// Maximum consecutive reconcile-only runs before a sidecar entry is aged out.
/// An entry whose `miss_count` reaches this value is removed from the next sidecar
/// and excluded from reconciliation. Override with `AXON_ETAG_MAX_MISS_RUNS`
/// (must be ≥1; clamped to `u8`). Default 3.
fn max_miss_runs() -> u8 {
    std::env::var("AXON_ETAG_MAX_MISS_RUNS")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(3)
}

/// One URL's cached conditional-request validators.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EtagEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    /// Consecutive runs in which this URL was reconciled (spider 304) but never
    /// arrived fresh. When this reaches `max_miss_runs()` the entry is dropped
    /// from the sidecar so genuinely-deleted pages are not reused indefinitely.
    /// Not serialized when 0 to keep sidecar files compact.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub miss_count: u8,
}

impl EtagEntry {
    fn is_empty(&self) -> bool {
        self.etag.is_none() && self.last_modified.is_none()
    }
}

/// Sidecar file name written next to `manifest.jsonl` in the crawl output dir.
pub const ETAG_SIDECAR_FILENAME: &str = "etag.json";

/// Resolve the sidecar path for a crawl output directory.
pub fn sidecar_path(output_dir: &Path) -> std::path::PathBuf {
    output_dir.join(ETAG_SIDECAR_FILENAME)
}

/// Load the persisted validator sidecar. Returns an empty map when absent or
/// unparseable — a missing/corrupt sidecar must never fail a crawl.
pub async fn load_sidecar(output_dir: &Path) -> HashMap<String, EtagEntry> {
    let path = sidecar_path(output_dir);
    let Ok(bytes) = tokio::fs::read(&path).await else {
        return HashMap::new();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

/// Persist the validator sidecar atomically (temp file + rename).
async fn write_sidecar(
    output_dir: &Path,
    data: &HashMap<String, EtagEntry>,
) -> Result<(), std::io::Error> {
    let path = sidecar_path(output_dir);
    let tmp = path.with_extension("json.tmp");
    let payload = serde_json::to_vec(data).map_err(std::io::Error::other)?;
    tokio::fs::write(&tmp, payload).await?;
    // On Windows, `rename` over an existing destination fails — remove it first.
    // On Unix, `rename` replaces atomically and this block is compiled out.
    #[cfg(windows)]
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    tokio::fs::rename(&tmp, &path).await
}

/// Materialize spider's ETag cache and seed it from the loaded sidecar.
///
/// The cache is lazily built inside spider's crawl setup (`setup_base`), so at
/// configure time `get_etag_cache()` returns `None`. `configure_setup_norobots()`
/// runs that setup synchronously *without* spawning a control thread, letting us
/// `store()` validators before the crawl begins. The subsequent `setup_base`
/// inside `crawl_raw()` is guarded by an `is_none()` check upstream, so it
/// preserves the seeded cache rather than clearing it.
///
/// Returns the set of URLs that were seeded (non-empty validators stored). When
/// the feature is unavailable or no entries seed, the returned set is empty and
/// the caller performs no reconciliation — safe by construction.
pub fn seed_website_etag_cache(
    website: &mut Website,
    sidecar: &HashMap<String, EtagEntry>,
) -> HashSet<String> {
    let mut seeded = HashSet::new();
    if sidecar.is_empty() {
        return seeded;
    }

    // Force the per-Website cache into existence without a control thread.
    let prior_control = website.configuration.no_control_thread;
    website.with_no_control_thread(true);
    website.configure_setup_norobots();
    website.with_no_control_thread(prior_control);

    let Some(cache) = website.get_etag_cache() else {
        crate::core::logging::log_warn(
            "etag: cache did not materialize after setup; conditional re-crawl inactive this run",
        );
        return seeded;
    };

    for (url, entry) in sidecar {
        if entry.is_empty() {
            continue;
        }
        cache.store(url, entry.etag.as_deref(), entry.last_modified.as_deref());
        seeded.insert(url.clone());
    }
    crate::core::logging::log_info(&format!(
        "etag: seeded {} conditional validator(s) from sidecar",
        seeded.len()
    ));
    seeded
}

/// Load the validator sidecar and seed the website cache when conditional
/// re-crawl is enabled. Returns `(previous_sidecar, seeded_urls)`. When the
/// feature is off, both are empty and the caller performs no reconciliation.
pub async fn load_and_seed(
    cfg: &crate::core::config::Config,
    website: &mut Website,
    output_dir: &Path,
) -> (HashMap<String, EtagEntry>, HashSet<String>) {
    if !cfg.etag_conditional {
        return (HashMap::new(), HashSet::new());
    }
    let previous = load_sidecar(output_dir).await;
    let seeded = seed_website_etag_cache(website, &previous);
    (previous, seeded)
}

/// Build the next sidecar by overlaying freshly-stored validators (read back from
/// the live cache for URLs that arrived this run) on top of the previous sidecar.
///
/// Carry-forward matters because spider's 304 path returns *before* it re-stores
/// validators, so a URL that 304'd this run has no entry in the live cache — its
/// validators would vanish after one hop without carrying the old sidecar forward.
///
/// Age-out: for every URL NOT in `arrived_urls`, `miss_count` is incremented.
/// Once `miss_count` reaches `max_miss_runs()` the entry is removed from the
/// next sidecar entirely, preventing indefinitely-reconciled deleted pages.
pub fn build_next_sidecar(
    website: &Website,
    previous: &HashMap<String, EtagEntry>,
    arrived_urls: &HashSet<String>,
) -> HashMap<String, EtagEntry> {
    let max_miss = max_miss_runs();
    // Start from the previous sidecar so 304'd URLs keep their validators.
    let mut next = previous.clone();

    // Refresh validators and reset miss_count for URLs that arrived this run.
    if let Some(cache) = website.get_etag_cache() {
        for url in arrived_urls {
            if let Some((etag, last_modified)) = cache.get(url) {
                let entry = EtagEntry {
                    etag: etag.map(|s| s.to_string()),
                    last_modified: last_modified.map(|s| s.to_string()),
                    miss_count: 0,
                };
                if entry.is_empty() {
                    next.remove(url);
                } else {
                    next.insert(url.clone(), entry);
                }
            } else if let Some(e) = next.get_mut(url) {
                // Arrived but no live-cache entry — keep validators, reset counter.
                e.miss_count = 0;
            }
        }
    } else {
        // No live cache — still reset miss_count for arrived URLs.
        for url in arrived_urls {
            if let Some(e) = next.get_mut(url) {
                e.miss_count = 0;
            }
        }
    }

    // Age-out: increment miss_count for non-arrived URLs; drop at the limit.
    let mut to_drop = Vec::new();
    for (url, entry) in next.iter_mut() {
        if !arrived_urls.contains(url) {
            entry.miss_count = entry.miss_count.saturating_add(1);
            if entry.miss_count >= max_miss {
                to_drop.push(url.clone());
            }
        }
    }
    if !to_drop.is_empty() {
        crate::core::logging::log_info(&format!(
            "etag: aged out {} validator(s) after {} consecutive non-arriving run(s)",
            to_drop.len(),
            max_miss,
        ));
        for url in to_drop {
            next.remove(&url);
        }
    }

    next
}

/// Persist the next sidecar. Logs and swallows IO errors — a failed sidecar write
/// degrades cross-run benefit but must not fail an otherwise-successful crawl.
pub async fn persist_next_sidecar(
    output_dir: &Path,
    website: &Website,
    previous: &HashMap<String, EtagEntry>,
    arrived_urls: &HashSet<String>,
) {
    let next = build_next_sidecar(website, previous, arrived_urls);
    if let Err(e) = write_sidecar(output_dir, &next).await {
        crate::core::logging::log_warn(&format!("etag: failed to persist validator sidecar: {e}"));
    }
}

/// The set of URLs to reconcile: previously-indexed, seeded with validators,
/// absent from this crawl's arrivals, **and** present in spider's visited set.
/// The visited-set gate is what distinguishes a genuine 304 skip (spider fetched
/// it and got Not-Modified) from a page that is no longer discovered at all
/// (never scheduled this run) — only the former is reused.
///
/// URLs whose `miss_count` in `sidecar` has reached `max_miss_runs()` are excluded:
/// they will be dropped from the next sidecar by `build_next_sidecar` and should not
/// be reconciled again to prevent indefinite reuse of deleted pages.
pub fn reconcile_targets(
    previous_manifest: &HashMap<String, ManifestEntry>,
    seeded_urls: &HashSet<String>,
    arrived_urls: &HashSet<String>,
    visited_urls: &HashSet<String>,
    sidecar: &HashMap<String, EtagEntry>,
) -> Vec<String> {
    let max_miss = max_miss_runs();
    let mut targets: Vec<String> = previous_manifest
        .keys()
        .filter(|url| {
            seeded_urls.contains(*url)
                && !arrived_urls.contains(*url)
                && visited_urls.contains(*url)
                && sidecar
                    .get(*url)
                    .map(|e| e.miss_count < max_miss)
                    .unwrap_or(true)
        })
        .cloned()
        .collect();
    targets.sort();
    targets
}

/// Re-emit previous manifest entries for 304-skipped pages, relinking their
/// markdown from the recycling bin (`markdown.old`) and appending reused entries
/// to the (now-closed) manifest, which is reopened in append mode.
///
/// `sidecar` is the PREVIOUS run's sidecar — used to skip URLs whose `miss_count`
/// has reached `max_miss_runs()` so aged-out entries are not reconciled.
///
/// Returns the number of pages reconciled. Failures to relink an individual page
/// are logged and skipped — a partial reconcile is better than a failed crawl.
pub async fn reconcile_unmodified(
    output_dir: &Path,
    previous_manifest: &HashMap<String, ManifestEntry>,
    seeded_urls: &HashSet<String>,
    arrived_urls: &HashSet<String>,
    visited_urls: &HashSet<String>,
    sidecar: &HashMap<String, EtagEntry>,
) -> usize {
    let targets = reconcile_targets(
        previous_manifest,
        seeded_urls,
        arrived_urls,
        visited_urls,
        sidecar,
    );
    if targets.is_empty() {
        return 0;
    }

    let markdown_dir = output_dir.join("markdown");
    let recycling_bin = output_dir.join("markdown.old");
    let manifest_path = output_dir.join("manifest.jsonl");

    let mut manifest = match tokio::fs::OpenOptions::new()
        .append(true)
        .open(&manifest_path)
        .await
    {
        Ok(file) => tokio::io::BufWriter::new(file),
        Err(e) => {
            crate::core::logging::log_warn(&format!(
                "etag: cannot reopen manifest for reconciliation: {e}"
            ));
            return 0;
        }
    };

    let mut reconciled = 0usize;
    for url in &targets {
        let Some(prev) = previous_manifest.get(url) else {
            continue;
        };
        if relink_reused_page(&markdown_dir, &recycling_bin, prev).await {
            let mut entry = prev.clone();
            entry.changed = false;
            if append_entry(&mut manifest, &entry).await {
                reconciled += 1;
            }
        }
    }

    if let Err(e) = manifest.flush().await {
        crate::core::logging::log_warn(&format!(
            "etag: manifest flush after reconcile failed: {e}"
        ));
    }
    if reconciled > 0 {
        crate::core::logging::log_info(&format!(
            "etag: reconciled {reconciled} unchanged page(s) from previous crawl (304 reuse)"
        ));
    }
    reconciled
}

/// Relink a single reused page's markdown from the recycling bin into the live
/// markdown dir. Returns `true` when the file is present in the live dir
/// afterward. The archived path is derived from the previous manifest's
/// `relative_path` and constrained to `markdown.old/` to prevent traversal.
async fn relink_reused_page(
    markdown_dir: &Path,
    recycling_bin: &Path,
    prev: &ManifestEntry,
) -> bool {
    let Some(filename) = Path::new(&prev.relative_path).file_name() else {
        return false;
    };
    let archived = recycling_bin.join(filename);
    let dest = markdown_dir.join(filename);

    if tokio::fs::try_exists(&dest).await.unwrap_or(false) {
        return true; // Already present (e.g. arrived via another path).
    }
    // Refuse a symlinked archive entry: a symlink planted in `markdown.old` could
    // otherwise make the reflink/copy below read content from outside the bin.
    if let Ok(meta) = tokio::fs::symlink_metadata(&archived).await
        && meta.file_type().is_symlink()
    {
        crate::core::logging::log_warn(&format!(
            "etag: archived markdown for {} is a symlink; refusing to relink",
            prev.url
        ));
        return false;
    }
    if !tokio::fs::try_exists(&archived).await.unwrap_or(false) {
        crate::core::logging::log_warn(&format!(
            "etag: archived markdown missing for reused page {}; skipping",
            prev.url
        ));
        return false;
    }
    if reflink_copy::reflink_or_copy(&archived, &dest).is_ok() {
        return true;
    }
    tokio::fs::hard_link(&archived, &dest).await.is_ok()
}

async fn append_entry(
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
    entry: &ManifestEntry,
) -> bool {
    let Ok(mut line) = serde_json::to_string(entry) else {
        return false;
    };
    line.push('\n');
    manifest.write_all(line.as_bytes()).await.is_ok()
}

#[cfg(test)]
#[path = "etag_tests.rs"]
mod tests;
