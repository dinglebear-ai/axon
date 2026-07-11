//! Source-input manifest: which crates/modules feed each generated doc
//! family (`docs-generator-contract.md` "Source Input Manifest").
//!
//! The manifest is derived, not hand-maintained: every family's generated
//! JSON schema artifact under `docs/reference/**/*.json` already carries an
//! `x-axon.source_inputs` list (path/kind/checksum) plus `x-axon.generated_by`
//! stamped as `cargo xtask schemas <family>`. This module scans those tracked
//! JSON files, groups entries by family slug, and unions/dedupes their
//! source inputs into one manifest.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceInputEntry {
    pub path: String,
    pub kind: String,
    pub checksum: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FamilyManifest {
    pub family: String,
    pub generated_by: String,
    pub source_inputs: Vec<SourceInputEntry>,
    pub manifest_checksum: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocsManifest {
    pub families: Vec<FamilyManifest>,
}

/// Scan every generated JSON schema artifact under `docs/reference` and
/// build one `FamilyManifest` per distinct `schemas <slug>` family found.
pub fn build(root: &Path) -> Result<DocsManifest> {
    let docs_root = root.join("docs/reference");
    let mut by_family: BTreeMap<String, Vec<SourceInputEntry>> = BTreeMap::new();
    if !docs_root.is_dir() {
        return Ok(DocsManifest {
            families: Vec::new(),
        });
    }
    for entry in WalkDir::new(&docs_root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            continue;
        };
        let Some(family_slug) = generated_by_family(&value) else {
            continue;
        };
        let inputs = extract_source_inputs(&value);
        by_family.entry(family_slug).or_default().extend(inputs);
    }

    let mut families = Vec::with_capacity(by_family.len());
    for (family, mut inputs) in by_family {
        inputs.sort_by(|a, b| a.path.cmp(&b.path));
        inputs.dedup_by(|a, b| a.path == b.path);
        let manifest_checksum = checksum_inputs(&inputs);
        families.push(FamilyManifest {
            generated_by: format!("cargo xtask docs generate --family {family}"),
            family,
            source_inputs: inputs,
            manifest_checksum,
        });
    }
    Ok(DocsManifest { families })
}

/// Serialize the manifest as stable, sorted JSON.
pub fn to_json(manifest: &DocsManifest) -> Result<String> {
    let mut content = serde_json::to_string_pretty(manifest)?;
    content.push('\n');
    Ok(content)
}

fn generated_by_family(value: &Value) -> Option<String> {
    let generated_by = value.get("x-axon")?.get("generated_by")?.as_str()?;
    generated_by
        .strip_prefix("cargo xtask schemas ")
        .map(str::to_string)
}

fn extract_source_inputs(value: &Value) -> Vec<SourceInputEntry> {
    let Some(inputs) = value
        .get("x-axon")
        .and_then(|x| x.get("source_inputs"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    inputs
        .iter()
        .filter_map(|entry| {
            Some(SourceInputEntry {
                path: entry.get("path")?.as_str()?.to_string(),
                kind: entry.get("kind")?.as_str()?.to_string(),
                checksum: entry.get("checksum")?.as_str()?.to_string(),
            })
        })
        .collect()
}

fn checksum_inputs(inputs: &[SourceInputEntry]) -> String {
    let mut hasher = Sha256::new();
    for input in inputs {
        hasher.update(input.path.as_bytes());
        hasher.update([0]);
        hasher.update(input.kind.as_bytes());
        hasher.update([0]);
        hasher.update(input.checksum.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

/// The manifest artifact's repo-relative output path.
pub const MANIFEST_PATH: &str = "docs/reference/source-input-manifest.json";

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
