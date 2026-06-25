use super::{CrawlAuditSnapshot, CrawlAuditSnapshotDiff, now_epoch_ms};
use axon_core::config::Config;
use axon_core::ui::{muted, primary};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::{Path, PathBuf};

async fn list_audit_reports(output_dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let audit_dir = output_dir.join("reports").join("crawl-audit");
    if !tokio::fs::try_exists(&audit_dir).await.unwrap_or(false) {
        return Ok(Vec::new());
    }
    let mut entries = tokio::fs::read_dir(audit_dir).await?;
    let mut out = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name.starts_with("audit-") && name.ends_with(".json"))
        {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

async fn read_audit_snapshot(path: &Path) -> Result<CrawlAuditSnapshot, Box<dyn Error>> {
    let bytes = tokio::fs::read(path).await?;
    Ok(serde_json::from_slice::<CrawlAuditSnapshot>(&bytes)?)
}

fn build_snapshot_diff(
    previous_report: &Path,
    current_report: &Path,
    previous: &CrawlAuditSnapshot,
    current: &CrawlAuditSnapshot,
) -> CrawlAuditSnapshotDiff {
    let manifest_prev_urls: HashSet<&String> =
        previous.manifest_entries.iter().map(|e| &e.url).collect();
    let manifest_curr_urls: HashSet<&String> =
        current.manifest_entries.iter().map(|e| &e.url).collect();
    let manifest_added = manifest_curr_urls.difference(&manifest_prev_urls).count();
    let manifest_removed = manifest_prev_urls.difference(&manifest_curr_urls).count();

    let prev_map: HashMap<&str, &str> = previous
        .manifest_entries
        .iter()
        .map(|entry| (entry.url.as_str(), entry.fingerprint.as_str()))
        .collect();
    let mut manifest_changed = 0usize;
    for entry in &current.manifest_entries {
        if let Some(prev_fp) = prev_map.get(entry.url.as_str())
            && *prev_fp != entry.fingerprint.as_str()
        {
            manifest_changed += 1;
        }
    }

    let prev_discovered: HashSet<&str> = previous
        .discovered_urls
        .iter()
        .map(|s| s.as_str())
        .collect();
    let curr_discovered: HashSet<&str> =
        current.discovered_urls.iter().map(|s| s.as_str()).collect();
    CrawlAuditSnapshotDiff {
        generated_at_epoch_ms: now_epoch_ms(),
        previous_report: previous_report.to_string_lossy().to_string(),
        current_report: current_report.to_string_lossy().to_string(),
        discovered_added: curr_discovered.difference(&prev_discovered).count(),
        discovered_removed: prev_discovered.difference(&curr_discovered).count(),
        manifest_added,
        manifest_removed,
        manifest_changed,
    }
}

pub(super) async fn run_crawl_audit_diff(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let reports = list_audit_reports(&cfg.output_dir).await?;
    if reports.len() < 2 {
        return Err(anyhow::anyhow!(
            "crawl diff requires at least two persisted crawl audit reports"
        )
        .into());
    }
    let previous_report = reports[reports.len() - 2].clone();
    let current_report = reports[reports.len() - 1].clone();
    let previous = read_audit_snapshot(&previous_report).await?;
    let current = read_audit_snapshot(&current_report).await?;
    let diff = build_snapshot_diff(&previous_report, &current_report, &previous, &current);
    let diff_path = cfg
        .output_dir
        .join("reports")
        .join("crawl-audit")
        .join(format!("diff-{}.json", now_epoch_ms()));
    tokio::fs::write(&diff_path, serde_json::to_string_pretty(&diff)?).await?;

    if cfg.json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "diff_report_path": diff_path.to_string_lossy(),
                "diff": diff,
            }))?
        );
    } else {
        println!("{}", primary("Crawl Audit Diff"));
        println!("  {} {}", muted("Report:"), diff_path.to_string_lossy());
        println!("  {} {}", muted("Manifest added:"), diff.manifest_added);
        println!("  {} {}", muted("Manifest removed:"), diff.manifest_removed);
        println!("  {} {}", muted("Manifest changed:"), diff.manifest_changed);
    }
    Ok(())
}
