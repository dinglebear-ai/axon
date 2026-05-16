use serde::{Deserialize, Serialize};
use spider::url::Url;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub url: String,
    pub relative_path: String,
    pub markdown_chars: usize,
    #[serde(default)]
    pub content_hash: Option<String>,
    #[serde(default = "default_true")]
    pub changed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CrawlAuditDiff {
    pub start_url: String,
    pub previous_count: usize,
    pub current_count: usize,
    pub added_count: usize,
    pub removed_count: usize,
    pub unchanged_count: usize,
    pub cache_hit: bool,
    pub cache_source: Option<String>,
}

fn default_true() -> bool {
    true
}

use std::time::SystemTime;

pub async fn manifest_cache_is_stale(manifest_path: &Path, ttl_secs: u64) -> bool {
    let Ok(metadata) = tokio::fs::metadata(manifest_path).await else {
        return true; // Missing or unreadable file = stale
    };
    let Ok(mtime) = metadata.modified() else {
        return true; // Platform doesn't support mtime = treat as stale
    };
    match SystemTime::now().duration_since(mtime) {
        Ok(age) => age.as_secs() > ttl_secs,
        Err(_) => true, // Clock skew (mtime in future) = treat as stale
    }
}

pub async fn write_audit_diff(
    output_dir: &Path,
    start_url: &str,
    previous: &HashSet<String>,
    current: &HashSet<String>,
    cache_hit: bool,
    cache_source: Option<String>,
) -> Result<(PathBuf, CrawlAuditDiff), std::io::Error> {
    let unchanged_count = previous.intersection(current).count();
    let added_count = current.difference(previous).count();
    let removed_count = previous.difference(current).count();
    let report = CrawlAuditDiff {
        start_url: start_url.to_string(),
        previous_count: previous.len(),
        current_count: current.len(),
        added_count,
        removed_count,
        unchanged_count,
        cache_hit,
        cache_source,
    };

    let audit_dir = output_dir.join("audit");
    tokio::fs::create_dir_all(&audit_dir).await?;
    let slug = Url::parse(start_url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    let report_path = audit_dir.join(format!("{slug}-diff-report.json"));
    let payload = serde_json::to_string_pretty(&report).map_err(std::io::Error::other)?;
    tokio::fs::write(&report_path, payload).await?;
    Ok((report_path, report))
}

pub async fn read_manifest_urls(path: &Path) -> Result<HashSet<String>, std::io::Error> {
    if !tokio::fs::try_exists(path).await? {
        return Ok(HashSet::new());
    }
    let file = tokio::fs::File::open(path).await?;
    let reader = tokio::io::BufReader::new(file);
    let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
    let mut out = HashSet::new();
    while let Some(raw) = lines.next_line().await? {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(url) = json.get("url").and_then(|value| value.as_str()) else {
            continue;
        };
        out.insert(url.to_string());
    }
    Ok(out)
}

/// Reads manifest JSONL into a HashMap keyed by URL. Duplicate URLs use last-write-wins
/// (the latest entry in the file supersedes earlier ones for the same URL).
pub async fn read_manifest_data(
    path: &Path,
) -> Result<HashMap<String, ManifestEntry>, std::io::Error> {
    if !tokio::fs::try_exists(path).await? {
        return Ok(HashMap::new());
    }
    let file = tokio::fs::File::open(path).await?;
    let reader = tokio::io::BufReader::new(file);
    let mut lines = tokio::io::AsyncBufReadExt::lines(reader);
    let mut out = HashMap::new();
    while let Some(raw) = lines.next_line().await? {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<ManifestEntry>(line) else {
            continue;
        };
        out.insert(entry.url.clone(), entry);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
