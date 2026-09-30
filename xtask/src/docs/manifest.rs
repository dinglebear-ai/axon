//! Preserve producer provenance from generated JSON without inventing docs commands.
use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
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
    /// Actual producer of the schema inputs, not a guessed downstream command.
    pub generated_by: String,
    pub source_inputs: Vec<SourceInputEntry>,
    pub manifest_checksum: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocsManifest {
    pub families: Vec<FamilyManifest>,
}

type Inputs = BTreeMap<String, SourceInputEntry>;

pub fn build(root: &Path) -> Result<DocsManifest> {
    let directory = root.join("docs/reference");
    let mut grouped: BTreeMap<String, (String, Inputs)> = BTreeMap::new();
    if !directory.is_dir() {
        return Ok(DocsManifest {
            families: Vec::new(),
        });
    }
    for entry in WalkDir::new(&directory).sort_by_file_name() {
        let entry = entry.context("scan generated schema provenance")?;
        let path = entry.path();
        if !entry.file_type().is_file() || path.extension().and_then(|e| e.to_str()) != Some("json")
        {
            continue;
        }
        let raw =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let value: Value = serde_json::from_str(&raw).with_context(|| {
            format!(
                "parse {}; malformed JSON cannot be omitted from provenance",
                path.display()
            )
        })?;
        let Some((family, producer)) = generated_by_family(&value)? else {
            continue;
        };
        let group = grouped
            .entry(family.clone())
            .or_insert_with(|| (producer.clone(), Inputs::new()));
        if group.0 != producer {
            bail!(
                "{}: conflicting producers for {family}; fix the owning schema",
                path.display()
            );
        }
        for input in extract_source_inputs(&value)
            .with_context(|| format!("{}: invalid source provenance", path.display()))?
        {
            if let Some(previous) = group.1.get(&input.path) {
                if previous != &input {
                    bail!(
                        "{}: conflicting provenance for {}; regenerate all dependent schemas",
                        path.display(),
                        input.path
                    );
                }
            } else {
                group.1.insert(input.path.clone(), input);
            }
        }
    }
    let families = grouped
        .into_iter()
        .map(|(family, (generated_by, inputs))| {
            let source_inputs = inputs.into_values().collect::<Vec<_>>();
            FamilyManifest {
                family,
                generated_by,
                manifest_checksum: checksum_inputs(&source_inputs),
                source_inputs,
            }
        })
        .collect();
    Ok(DocsManifest { families })
}

pub fn to_json(manifest: &DocsManifest) -> Result<String> {
    Ok(serde_json::to_string_pretty(manifest)? + "\n")
}

pub(super) fn refresh(root: &Path) -> Result<()> {
    let content = to_json(&build(root)?)?;
    let path = root.join(MANIFEST_PATH);
    std::fs::create_dir_all(path.parent().context("manifest parent")?)?;
    std::fs::write(&path, content).with_context(|| format!("write {}", path.display()))?;
    println!("docs generate: wrote {MANIFEST_PATH}.");
    Ok(())
}

pub(super) fn check(root: &Path) -> Result<()> {
    let expected = to_json(&build(root)?)?;
    let path = root.join(MANIFEST_PATH);
    match std::fs::read_to_string(path) {
        Ok(actual) if actual == expected => {
            println!("docs generate --check: {MANIFEST_PATH} is up to date.");
            Ok(())
        }
        Ok(_) => bail!("{MANIFEST_PATH} differs; run cargo xtask generated-contracts refresh"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            bail!("{MANIFEST_PATH} is missing; run cargo xtask generated-contracts refresh")
        }
        Err(error) => Err(error).context("read source-input manifest"),
    }
}

fn generated_by_family(value: &Value) -> Result<Option<(String, String)>> {
    let Some(meta) = value.get("x-axon") else {
        return Ok(None);
    };
    let producer = meta
        .get("generated_by")
        .and_then(Value::as_str)
        .context("x-axon.generated_by must name the actual schema producer")?;
    let family = if producer == "cargo xtask presentation generate" {
        "presentation"
    } else if let Some(family) = producer.strip_prefix("cargo xtask schemas ") {
        if <crate::schemas::SchemaFamily as clap::ValueEnum>::from_str(family, false).is_err() {
            bail!("invalid schema producer {producer}; use a canonical family command");
        }
        family
    } else {
        bail!(
            "unrecognized generated schema producer {producer}; declare its ownership explicitly"
        );
    };
    Ok(Some((family.to_owned(), producer.to_owned())))
}

fn extract_source_inputs(value: &Value) -> Result<Vec<SourceInputEntry>> {
    let inputs = value
        .pointer("/x-axon/source_inputs")
        .and_then(Value::as_array)
        .context("x-axon.source_inputs must be an array")?;
    inputs
        .iter()
        .map(|entry| {
            let text = |key| {
                entry
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|v| !v.is_empty())
                    .with_context(|| format!("source input is missing {key}"))
            };
            Ok(SourceInputEntry {
                path: text("path")?.to_owned(),
                kind: text("kind")?.to_owned(),
                checksum: text("checksum")?.to_owned(),
            })
        })
        .collect()
}

fn checksum_inputs(inputs: &[SourceInputEntry]) -> String {
    let mut hash = Sha256::new();
    for input in inputs {
        for part in [&input.path, &input.kind, &input.checksum] {
            hash.update(part.as_bytes());
            hash.update([0]);
        }
    }
    format!("sha256:{:x}", hash.finalize())
}

pub const MANIFEST_PATH: &str = "docs/reference/source-input-manifest.json";

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
