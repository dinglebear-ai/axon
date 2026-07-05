use std::path::Path;

use anyhow::{Context, Result, bail};
use jsonschema::validator_for;
use serde_json::Value;

use super::SchemaFamily;
use super::artifact_index::ArtifactIndex;
use super::report::FamilyReport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationMode {
    Check,
    UpdateFixtures,
}

pub fn validate_family(
    root: &Path,
    family: SchemaFamily,
    index: &ArtifactIndex,
    mode: ValidationMode,
) -> Result<FamilyReport> {
    let fixture_root = root
        .join("xtask/tests/fixtures/schemas")
        .join(family.as_str());
    if !fixture_root.exists() {
        bail!(
            "{} is missing required schema fixture root {}",
            family.as_str(),
            fixture_root.display()
        );
    }

    let required_dirs = ["valid", "invalid", "snapshots"];
    for dir in required_dirs {
        let path = fixture_root.join(dir);
        if !path.is_dir() {
            bail!(
                "{} is missing required schema fixture category {dir}",
                family.as_str()
            );
        }
    }

    let schema = primary_json_schema(index)
        .with_context(|| format!("{} has no generated JSON schema artifact", family.as_str()))?;
    let validator = validator_for(schema)?;
    let fixtures_validated = validate_valid_fixtures(&validator, &fixture_root.join("valid"))?
        + validate_invalid_fixtures(&validator, &fixture_root.join("invalid"))?;
    if fixtures_validated < 2 {
        bail!(
            "{} must have at least one valid fixture and one invalid fixture",
            family.as_str()
        );
    }
    if mode == ValidationMode::UpdateFixtures {
        update_snapshots(index, &fixture_root.join("snapshots"))?;
    }
    let snapshots_checked = validate_snapshots(index, &fixture_root.join("snapshots"))?;
    if snapshots_checked == 0 {
        bail!(
            "{} must have at least one schema snapshot fixture",
            family.as_str()
        );
    }
    let mut report = FamilyReport::ok(family, index.iter().count());
    report.fixtures_validated = fixtures_validated;
    report.snapshots_checked = snapshots_checked;
    if mode == ValidationMode::UpdateFixtures {
        report
            .warnings
            .push("--update-fixtures accepted for local fixture refresh".to_string());
    }
    Ok(report)
}

fn primary_json_schema(index: &ArtifactIndex) -> Option<&Value> {
    index.iter().find_map(|artifact| artifact.json.as_ref())
}

fn validate_valid_fixtures(validator: &jsonschema::Validator, path: &Path) -> Result<usize> {
    let mut count = 0;
    for fixture in json_files(path)? {
        let value = read_json(&fixture)?;
        if let Err(error) = validator.validate(&value) {
            bail!(
                "valid fixture {} failed schema validation: {error}",
                fixture.display()
            );
        }
        count += 1;
    }
    Ok(count)
}

fn validate_invalid_fixtures(validator: &jsonschema::Validator, path: &Path) -> Result<usize> {
    let mut count = 0;
    for fixture in json_files(path)? {
        let value = read_json(&fixture)?;
        if validator.validate(&value).is_ok() {
            bail!(
                "invalid fixture {} unexpectedly passed schema validation",
                fixture.display()
            );
        }
        count += 1;
    }
    Ok(count)
}

fn validate_snapshots(index: &ArtifactIndex, path: &Path) -> Result<usize> {
    let mut count = 0;
    for fixture in json_files(path)? {
        let snapshot = read_json(&fixture)?;
        let Some(file_name) = fixture.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(artifact) = index.iter().find(|artifact| {
            artifact.path.file_name().and_then(|name| name.to_str()) == Some(file_name)
        }) else {
            bail!(
                "snapshot {} has no matching generated artifact",
                fixture.display()
            );
        };
        if artifact.json.as_ref() != Some(&snapshot) {
            bail!(
                "snapshot {} differs from generated artifact",
                fixture.display()
            );
        }
        count += 1;
    }
    Ok(count)
}

fn update_snapshots(index: &ArtifactIndex, path: &Path) -> Result<()> {
    std::fs::create_dir_all(path)?;
    for entry in json_files(path)? {
        std::fs::remove_file(&entry)
            .with_context(|| format!("failed to remove stale snapshot {}", entry.display()))?;
    }
    for artifact in index.iter().filter(|artifact| artifact.json.is_some()) {
        let Some(file_name) = artifact.path.file_name() else {
            continue;
        };
        let target = path.join(file_name);
        std::fs::write(&target, &artifact.raw)
            .with_context(|| format!("failed to write snapshot {}", target.display()))?;
    }
    Ok(())
}

fn json_files(path: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(path) {
        let entry = entry?;
        if entry.file_type().is_file()
            && entry.path().extension().and_then(|ext| ext.to_str()) == Some("json")
        {
            files.push(entry.path().to_path_buf());
        }
    }
    files.sort();
    Ok(files)
}

fn read_json(path: &Path) -> Result<Value> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read fixture {}", path.display()))?;
    serde_json::from_str(&content)
        .with_context(|| format!("failed to parse fixture JSON {}", path.display()))
}
