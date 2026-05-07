use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use spider_transformations::transformation::content::{
    SelectorConfiguration, TransformConfig, TransformInput, transform_content_input,
};
use tokio::sync::mpsc::Sender;

use super::super::is_excluded_url_path;
use super::super::{
    CrawlSummary, MapScope, canonicalize_url_for_dedupe, normalize_map_candidate_url,
};
use crate::core::content::{BOILERPLATE_SELECTORS, clean_markdown_whitespace, url_to_filename};
use crate::crawl::manifest::ManifestEntry;

pub struct CollectorConfig {
    pub markdown_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub min_chars: usize,
    pub drop_thin: bool,
    pub exclude_path_prefix: Vec<String>,
    pub scope: Option<MapScope>,
    pub transform_cfg: &'static TransformConfig,
    pub progress_tx: Option<Sender<CrawlSummary>>,
    pub previous_manifest: Arc<HashMap<String, ManifestEntry>>,
    pub selector_config: Option<SelectorConfiguration>,
    pub chrome_ws_url: Option<String>,
    pub chrome_timeout_secs: u64,
    pub output_dir: PathBuf,
}

pub enum PageOutcome {
    Thin {
        trimmed: String,
        content_hash: String,
    },
    Empty,
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

pub fn process_page(
    html_bytes: &[u8],
    url: &str,
    col: &CollectorConfig,
    next_file_count: u32,
) -> PageOutcome {
    let input = TransformInput {
        url: None,
        content: html_bytes,
        screenshot_bytes: None,
        encoding: None,
        selector_config: col.selector_config.as_ref(),
        ignore_tags: Some(BOILERPLATE_SELECTORS),
    };
    let markdown = transform_content_input(input, col.transform_cfg);
    let trimmed = clean_markdown_whitespace(markdown.trim());
    let chars = trimmed.len();

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
        let filename = url_to_filename(url, next_file_count);
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

    let filename = url_to_filename(url, next_file_count);
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
