use anyhow::{Result, bail};
use std::path::Path;

/// Files that must all carry the same version string.
///
/// Cargo.toml is the single source of truth — all other files are checked
/// against the version declared there.
const VERSION_FILES: &[(&str, VersionKind)] = &[
    ("Cargo.toml", VersionKind::Cargo),
    ("README.md", VersionKind::ReadmeHeader),
    ("CHANGELOG.md", VersionKind::Changelog),
    (
        "plugins/axon/.claude-plugin/plugin.json",
        VersionKind::PluginJson,
    ),
];

#[derive(Debug, Clone, Copy)]
enum VersionKind {
    /// `version = "X.Y.Z"` in [package] section
    Cargo,
    /// `Version: X.Y.Z` anywhere in the file
    ReadmeHeader,
    /// `## [X.Y.Z]` heading in CHANGELOG
    Changelog,
    /// `"version": "X.Y.Z"` JSON field — must NOT be present
    PluginJson,
}

/// Extract the axon package version from Cargo.toml.
fn read_cargo_version(content: &str) -> Option<String> {
    let mut in_package = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            continue;
        }
        if in_package && trimmed.starts_with('[') {
            // Left the [package] section
            break;
        }
        if in_package && let Some(rest) = trimmed.strip_prefix("version") {
            let rest = rest.trim_start_matches([' ', '\t', '=']);
            if let Some(ver) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                return Some(ver.to_owned());
            }
        }
    }
    None
}

fn check_readme(content: &str, expected: &str) -> Result<()> {
    let pattern = format!("Version: {expected}");
    if content.contains(&pattern) {
        return Ok(());
    }
    bail!(
        "README.md does not contain 'Version: {expected}'\n\
         Hint: update the 'Version: X.Y.Z' line in README.md to match Cargo.toml"
    );
}

fn check_changelog(content: &str, expected: &str) -> Result<()> {
    let heading = format!("## [{expected}]");
    if content.contains(&heading) {
        return Ok(());
    }
    bail!(
        "CHANGELOG.md does not contain a '## [{expected}]' heading\n\
         Hint: add a changelog entry for version {expected}"
    );
}

fn check_plugin_json(content: &str, path: &str) -> Result<()> {
    // plugin.json must NOT carry a version field — it is versioned by the
    // marketplace, not the manifest. `just validate-plugin` hard-fails on it.
    if content.contains("\"version\"") {
        bail!(
            "{path} must NOT contain a \"version\" key\n\
             The plugin is versioned by the marketplace. Remove the field."
        );
    }
    Ok(())
}

pub fn check(root: &Path) -> Result<()> {
    // Read Cargo.toml first to extract the canonical version
    let cargo_path = root.join("Cargo.toml");
    let cargo_content = std::fs::read_to_string(&cargo_path)
        .map_err(|e| anyhow::anyhow!("Failed to read Cargo.toml: {e}"))?;

    let version = read_cargo_version(&cargo_content).ok_or_else(|| {
        anyhow::anyhow!("Could not extract version from [package] section of Cargo.toml")
    })?;

    println!("Checking version parity: {version}");

    let mut errors: Vec<String> = Vec::new();

    for (rel_path, kind) in VERSION_FILES {
        if matches!(kind, VersionKind::Cargo) {
            // Already read above — just validate semver shape
            let parts: Vec<&str> = version.split('.').collect();
            if parts.len() != 3 || parts.iter().any(|p| p.parse::<u32>().is_err()) {
                errors.push(format!(
                    "Cargo.toml version '{version}' is not valid semver (expected X.Y.Z)"
                ));
            }
            continue;
        }

        let full_path = root.join(rel_path);
        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("Failed to read {rel_path}: {e}"));
                continue;
            }
        };

        let result = match kind {
            VersionKind::Cargo => unreachable!(),
            VersionKind::ReadmeHeader => check_readme(&content, &version),
            VersionKind::Changelog => check_changelog(&content, &version),
            VersionKind::PluginJson => check_plugin_json(&content, rel_path),
        };

        if let Err(e) = result {
            errors.push(format!("{rel_path}: {e}"));
        } else {
            println!("  OK  {rel_path}");
        }
    }

    if !errors.is_empty() {
        eprintln!("\nVersion sync errors:");
        for err in &errors {
            eprintln!("  ✗  {err}");
        }
        eprintln!();
        eprintln!("All version-bearing files must carry version {version}.");
        eprintln!("See the 'Version Bumping' section in CLAUDE.md for the file list.");
        bail!("version sync check failed ({} error(s))", errors.len());
    }

    println!("OK: all version-bearing files are in sync at {version}.");
    Ok(())
}

#[cfg(test)]
#[path = "version_sync_tests.rs"]
mod tests;
