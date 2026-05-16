use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use spider_transformations::transformation::content::SelectorConfiguration;
use tokio::sync::mpsc::Sender;

use super::super::is_excluded_url_path;
use super::super::{
    CrawlSummary, MapScope, canonicalize_url_for_dedupe, normalize_map_candidate_url,
};
use crate::core::content::{
    LadderThresholds, LadderTier, extract_with_ladder, url_to_stable_filename,
};
use crate::crawl::manifest::ManifestEntry;

pub struct CollectorConfig {
    pub markdown_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub min_chars: usize,
    pub drop_thin: bool,
    pub exclude_path_prefix: Vec<String>,
    pub scope: Option<MapScope>,
    pub progress_tx: Option<Sender<CrawlSummary>>,
    pub previous_manifest: Arc<HashMap<String, ManifestEntry>>,
    pub selector_config: Option<SelectorConfiguration>,
    pub chrome_ws_url: Option<String>,
    pub chrome_timeout_secs: u64,
    pub output_dir: PathBuf,
    pub ladder_thresholds: LadderThresholds,
    /// Maximum bytes scanned for antibot challenge patterns.
    /// Passed from `cfg.antibot_max_body_scan_bytes` (default 150 KiB).
    pub antibot_max_scan_bytes: usize,
}

pub enum PageOutcome {
    Thin {
        trimmed: String,
        content_hash: String,
    },
    Empty,
    /// Antibot challenge page detected (CF/Akamai/DataDome/etc).
    /// Page is skipped — not embedded, not saved to disk.
    /// Cookie-warmup retry is a follow-up (TODO: thread CookieJar through collector).
    Challenged {
        vendor: String,
    },
    Reused {
        filename: String,
        trimmed: String,
        entry: ManifestEntry,
    },
    Write {
        filename: String,
        trimmed: String,
        entry: ManifestEntry,
    },
}

pub fn process_page(html_bytes: &[u8], url: &str, col: &CollectorConfig) -> PageOutcome {
    let ladder = extract_with_ladder(
        html_bytes,
        col.selector_config.as_ref(),
        col.ladder_thresholds,
    );
    if ladder.tier != LadderTier::Scored {
        tracing::debug!(
            url = %url,
            tier = ladder.tier.as_str(),
            words = ladder.word_count,
            "ladder.tier_used"
        );
    }
    let trimmed = ladder.markdown;
    let chars = trimmed.len();

    // Challenge detection — MUST run before thin-page filter so CF/Akamai pages
    // are not silently dropped as empty content.
    // Headers are unavailable in the collector path; body-based detection catches
    // the most important cases. Cookie-warmup retry is deferred (TODO: thread
    // CookieJar through the collector pipeline).
    let html_str = String::from_utf8_lossy(html_bytes);
    if let Some(cd) = crate::core::http::detect_challenge(
        &html_str,
        |_| None,
        col.antibot_max_scan_bytes,
    ) {
        tracing::warn!(
            url = %url,
            vendor = %cd.vendor.as_str(),
            akamai_recoverable = cd.akamai_warmup_recoverable,
            "antibot.detected: challenge page, skipping"
        );
        return PageOutcome::Challenged {
            vendor: cd.vendor.as_str().to_string(),
        };
    }

    if trimmed.is_empty() {
        return PageOutcome::Empty;
    }

    let mut hasher = Sha256::new();
    hasher.update(trimmed.as_bytes());
    let content_hash = hex::encode(hasher.finalize());

    if chars < col.min_chars {
        crate::core::logging::log_debug(&format!(
            "content thin_page url={url} chars={chars} min={}",
            col.min_chars
        ));
        return PageOutcome::Thin {
            trimmed,
            content_hash,
        };
    }

    if let Some(prev) = col.previous_manifest.get(url)
        && prev.content_hash.as_deref() == Some(&content_hash)
    {
        let filename = url_to_stable_filename(url);
        let entry = ManifestEntry {
            url: url.to_string(),
            relative_path: format!("markdown/{filename}"),
            markdown_chars: chars,
            content_hash: Some(content_hash),
            changed: false,
        };
        return PageOutcome::Reused {
            filename,
            trimmed,
            entry,
        };
    }

    let filename = url_to_stable_filename(url);
    let entry = ManifestEntry {
        url: url.to_string(),
        relative_path: format!("markdown/{filename}"),
        markdown_chars: chars,
        content_hash: Some(content_hash),
        changed: true,
    };
    PageOutcome::Write {
        filename,
        trimmed,
        entry,
    }
}

pub fn canonicalize_and_track_page(
    raw_url: &str,
    col: &CollectorConfig,
    summary: &mut CrawlSummary,
    urls: &mut HashSet<String>,
    seen_canonical: &mut HashSet<String>,
) -> Option<String> {
    if is_excluded_url_path(raw_url, &col.exclude_path_prefix) {
        return None;
    }
    let url = match col.scope.as_ref() {
        Some(scope) => normalize_map_candidate_url(raw_url, scope, false)?,
        None => canonicalize_url_for_dedupe(raw_url)?,
    };
    if !seen_canonical.insert(url.clone()) {
        return None;
    }
    summary.pages_seen += 1;
    urls.insert(url.clone());
    Some(url)
}
