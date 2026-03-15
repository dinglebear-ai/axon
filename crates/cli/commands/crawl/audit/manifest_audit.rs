use super::sitemap::discover_sitemap_urls_with_robots;
use crate::crates::core::config::Config;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, BufReader};

use super::sitemap::SitemapDiscoveryStats;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestAuditEntry {
    pub url: String,
    pub file_path: String,
    pub markdown_chars: usize,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct CrawlAuditSnapshot {
    pub(super) generated_at_epoch_ms: u128,
    pub(super) start_url: String,
    pub(super) output_dir: String,
    pub(super) exclude_path_prefix: Vec<String>,
    pub(super) sitemap: SitemapDiscoveryStats,
    pub(super) discovered_url_count: usize,
    /// Full list of URLs discovered via sitemap/robots — used for accurate set-based diff.
    #[serde(default)]
    pub(super) discovered_urls: Vec<String>,
    pub(super) manifest_entry_count: usize,
    pub(super) manifest_entries: Vec<ManifestAuditEntry>,
}

fn fnv1a64_hex(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

enum ManifestEntryPathResolution {
    Valid(PathBuf),
    Missing,
    Rejected,
}

async fn resolve_manifest_entry_path(
    canonical_output_dir: &Path,
    file_path: &str,
) -> ManifestEntryPathResolution {
    if file_path.contains('\0') {
        return ManifestEntryPathResolution::Rejected;
    }
    let raw_path = Path::new(file_path);
    let candidate = if raw_path.is_absolute() {
        raw_path.to_path_buf()
    } else {
        canonical_output_dir.join(raw_path)
    };
    match tokio::fs::canonicalize(&candidate).await {
        Ok(canonical) if canonical.starts_with(canonical_output_dir) => {
            ManifestEntryPathResolution::Valid(canonical)
        }
        Ok(_) => ManifestEntryPathResolution::Rejected,
        Err(_) => ManifestEntryPathResolution::Missing,
    }
}

/// Reads every manifest entry and computes a content fingerprint for each
/// referenced file. This performs one file read per entry, so large manifests
/// will incur significant I/O. A future improvement could make fingerprinting
/// opt-in (e.g. `--verify-checksums`) or cache fingerprints across runs.
async fn read_manifest_entries(
    output_dir: &Path,
) -> Result<Vec<ManifestAuditEntry>, Box<dyn Error>> {
    let manifest_path = output_dir.join("manifest.jsonl");
    if !tokio::fs::try_exists(&manifest_path).await? {
        return Ok(Vec::new());
    }
    let file = tokio::fs::File::open(&manifest_path).await?;
    let canonical_output_dir = tokio::fs::canonicalize(output_dir).await?;
    let mut reader = BufReader::new(file).lines();
    let mut entries = Vec::new();
    while let Some(line) = reader.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        let Some(url) = json.get("url").and_then(|v| v.as_str()) else {
            continue;
        };
        let file_path = json
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let markdown_chars = json
            .get("markdown_chars")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let fingerprint = if file_path.is_empty() {
            "no-file-path".to_string()
        } else {
            match resolve_manifest_entry_path(&canonical_output_dir, &file_path).await {
                ManifestEntryPathResolution::Valid(canonical_path) => {
                    match tokio::fs::read(&canonical_path).await {
                        Ok(bytes) => fnv1a64_hex(&bytes),
                        Err(_) => "file-not-found".to_string(),
                    }
                }
                ManifestEntryPathResolution::Missing => "file-not-found".to_string(),
                ManifestEntryPathResolution::Rejected => "path-outside-output-dir".to_string(),
            }
        };
        entries.push(ManifestAuditEntry {
            url: url.to_string(),
            file_path,
            markdown_chars,
            fingerprint,
        });
    }
    Ok(entries)
}

pub(super) async fn persist_audit_snapshot(
    cfg: &Config,
    start_url: &str,
) -> Result<(PathBuf, CrawlAuditSnapshot), Box<dyn Error>> {
    let now = super::now_epoch_ms();
    let discovery = discover_sitemap_urls_with_robots(cfg, start_url).await?;
    let manifest_entries = read_manifest_entries(&cfg.output_dir).await?;
    let snapshot = CrawlAuditSnapshot {
        generated_at_epoch_ms: now,
        start_url: start_url.to_string(),
        output_dir: cfg.output_dir.to_string_lossy().to_string(),
        exclude_path_prefix: cfg.exclude_path_prefix.clone(),
        sitemap: discovery.stats,
        discovered_url_count: discovery.urls.len(),
        discovered_urls: discovery.urls.into_iter().collect(),
        manifest_entry_count: manifest_entries.len(),
        manifest_entries,
    };
    let audit_dir = cfg.output_dir.join("reports").join("crawl-audit");
    tokio::fs::create_dir_all(&audit_dir).await?;
    let path = audit_dir.join(format!("audit-{now}.json"));
    tokio::fs::write(&path, serde_json::to_string_pretty(&snapshot)?).await?;
    Ok((path, snapshot))
}

#[cfg(test)]
mod tests {
    use super::{fnv1a64_hex, read_manifest_entries};

    #[tokio::test]
    async fn read_manifest_entries_rejects_out_of_bounds_file_paths() {
        let output_dir = tempfile::TempDir::new().expect("output tempdir");
        let outside_dir = tempfile::TempDir::new().expect("outside tempdir");
        let markdown_dir = output_dir.path().join("markdown");
        tokio::fs::create_dir_all(&markdown_dir)
            .await
            .expect("create markdown dir");

        let in_bounds = markdown_dir.join("page.md");
        let outside = outside_dir.path().join("secret.md");
        tokio::fs::write(&in_bounds, "inside")
            .await
            .expect("write in-bounds file");
        tokio::fs::write(&outside, "outside")
            .await
            .expect("write out-of-bounds file");

        let manifest = format!(
            "{}\n{}\n",
            serde_json::json!({
                "url": "https://example.com/in",
                "file_path": in_bounds.to_string_lossy(),
                "markdown_chars": 6
            }),
            serde_json::json!({
                "url": "https://example.com/out",
                "file_path": outside.to_string_lossy(),
                "markdown_chars": 7
            })
        );
        tokio::fs::write(output_dir.path().join("manifest.jsonl"), manifest)
            .await
            .expect("write manifest");

        let entries = read_manifest_entries(output_dir.path())
            .await
            .expect("read manifest entries");

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].fingerprint, fnv1a64_hex(b"inside"));
        assert_eq!(entries[1].fingerprint, "path-outside-output-dir");
    }
}
