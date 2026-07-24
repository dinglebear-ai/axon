use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::path::Path;

/// Verify `.cargo/audit.toml` and `deny.toml` advisory ignore lists are in sync.
///
/// `deny.toml` is canonical (the source of truth for `cargo deny`). The
/// `.cargo/audit.toml` ignore list must match it as a set so `cargo audit`
/// and `cargo deny` agree on which advisories are suppressed. The two files
/// are kept in sync by hand today; this check guards against silent drift.
pub fn check(root: &Path) -> Result<()> {
    let audit_path = root.join(".cargo").join("audit.toml");
    let deny_path = root.join("deny.toml");

    let audit_ignores = parse_ignore_list(&audit_path)
        .with_context(|| format!("failed to parse {}", audit_path.display()))?;
    let deny_ignores = parse_ignore_list(&deny_path)
        .with_context(|| format!("failed to parse {}", deny_path.display()))?;

    let audit_set: BTreeSet<&str> = audit_ignores.iter().map(String::as_str).collect();
    let deny_set: BTreeSet<&str> = deny_ignores.iter().map(String::as_str).collect();

    if audit_set == deny_set {
        return Ok(());
    }

    let only_audit: Vec<_> = audit_set.difference(&deny_set).copied().collect();
    let only_deny: Vec<_> = deny_set.difference(&audit_set).copied().collect();

    eprintln!(
        "[audit-ignore-sync] advisory ignore lists drifted between \
         .cargo/audit.toml and deny.toml"
    );
    eprintln!("[audit-ignore-sync] deny.toml is canonical; .cargo/audit.toml must match.");
    if !only_audit.is_empty() {
        eprintln!(
            "[audit-ignore-sync] in .cargo/audit.toml but not deny.toml: {}",
            only_audit.join(", ")
        );
    }
    if !only_deny.is_empty() {
        eprintln!(
            "[audit-ignore-sync] in deny.toml but not .cargo/audit.toml: {}",
            only_deny.join(", ")
        );
    }
    bail!("advisory ignore list drift detected");
}

/// Extract `RUSTSEC-xxx` IDs from the `ignore = [...]` array under `[advisories]`.
///
/// Minimal line-based parser: no `toml` dependency on the xtask crate. Scans
/// for quoted IDs starting with `RUSTSEC-`, tolerating inline-array and
/// multi-line-array layouts, and comment-prefixed lines (the canonical
/// `deny.toml` annotates each ID with a leading `#` comment).
fn parse_ignore_list(path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let mut ids = Vec::new();
    for raw in content.lines() {
        // Strip leading `#` comments so annotated entries still parse, but
        // keep the rest of the line (the ID may live inline with a comment).
        let line = raw.split('#').next().unwrap_or(raw).trim();
        if line.is_empty() {
            continue;
        }
        // Split by double quotes to cleanly extract quoted string literals
        for part in line.split('"') {
            let id = part.trim();
            if id.starts_with("RUSTSEC-") {
                ids.push(id.to_string());
            }
        }
    }
    Ok(ids)
}

#[cfg(test)]
#[path = "audit_ignore_sync_tests.rs"]
mod tests;
