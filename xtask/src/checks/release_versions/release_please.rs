use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

use super::{
    Component, ReleaseContext, ReleaseResult, VersionKind, read_version,
    replace_gradle_version_name,
};
use crate::checks::release_versions::files::{
    increment_gradle_version_code, read_gradle_version_name,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReleasePleaseDispatchItem {
    pub id: String,
    pub workflow: String,
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReleasePleaseFixupItem {
    pub id: String,
    pub version: String,
}

pub(super) fn print_fixup_plan(items: &[ReleasePleaseFixupItem], json: bool) -> ReleaseResult<()> {
    print_items(
        items,
        json,
        "failed to serialize release-please fixup plan",
        |item| println!("{} version={}", item.id, item.version),
    )
}

pub(super) fn print_dispatch_plan(
    items: &[ReleasePleaseDispatchItem],
    json: bool,
) -> ReleaseResult<()> {
    print_items(
        items,
        json,
        "failed to serialize release-please dispatch plan",
        |item| println!("{} workflow={} tag={}", item.id, item.workflow, item.tag),
    )
}

fn print_items<T: Serialize>(
    items: &[T],
    json: bool,
    json_error: &str,
    print_text: impl Fn(&T),
) -> ReleaseResult<()> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(items).release_context(json_error)?
        );
    } else {
        for item in items {
            print_text(item);
        }
    }
    Ok(())
}

pub(super) fn check_manifest_versions(
    root: &Path,
    components: &[Component],
) -> ReleaseResult<Vec<String>> {
    let versions = read_release_please_manifest(root)?;
    let mut errors = Vec::new();

    for component in components {
        let Some(manifest_version) = versions.get(&component.release_please_path) else {
            errors.push(format!(
                "{}: .release-please-manifest.json is missing path {}",
                component.id, component.release_please_path
            ));
            continue;
        };
        let source_version = read_version(root, &component.version_source)?;
        if manifest_version != &source_version {
            errors.push(format!(
                "{}: .release-please-manifest.json path {} has version {}, expected {}",
                component.id, component.release_please_path, manifest_version, source_version
            ));
        }
    }

    Ok(errors)
}

pub(super) fn fixups(
    root: &Path,
    components: &[Component],
    component_id: &str,
    version: &str,
) -> ReleaseResult<()> {
    let component = components
        .iter()
        .find(|component| component.id == component_id)
        .with_release_context(|| format!("unknown release component {component_id}"))?;

    match component.id.as_str() {
        "cli" => run_cargo_update(root, "axon", version),
        "palette" => run_cargo_update(
            &root.join("apps/palette-tauri/src-tauri"),
            "axon-palette-tauri",
            version,
        ),
        "android" => android_fixup(root, component, version),
        "chrome" => Ok(()),
        other => release_bail!("unsupported release-please fixup component {other}"),
    }
}

pub(super) fn fixup_items(
    root: &Path,
    components: &[Component],
    files: &str,
) -> ReleaseResult<Vec<ReleasePleaseFixupItem>> {
    let changed_files = files.lines().collect::<BTreeSet<_>>();
    let versions = read_release_please_manifest(root)?;
    let mut items = Vec::new();

    for component in components {
        let release_please_touched_component = component.version_files.iter().any(|file| {
            !matches!(
                file.kind,
                VersionKind::CargoLockPackage | VersionKind::JsonNoVersion
            ) && changed_files.contains(file.path.as_str())
        });
        if !release_please_touched_component {
            continue;
        }

        let version = versions
            .get(&component.release_please_path)
            .with_release_context(|| {
                format!(
                    ".release-please-manifest.json is missing path {}",
                    component.release_please_path
                )
            })?;
        items.push(ReleasePleaseFixupItem {
            id: component.id.clone(),
            version: version.clone(),
        });
    }

    Ok(items)
}

pub(super) fn release_please_dispatch_items(
    components: &[Component],
    release_outputs: &str,
) -> ReleaseResult<Vec<ReleasePleaseDispatchItem>> {
    let outputs: serde_json::Value =
        serde_json::from_str(release_outputs).release_context("failed to parse release outputs")?;
    let paths_released = outputs
        .get("paths_released")
        .and_then(|value| value.as_str())
        .release_context("release outputs missing paths_released")?;
    let released_paths = parse_paths_released(paths_released)?;
    let mut items = Vec::new();

    for component in components {
        if !released_paths.contains(&component.release_please_path) {
            continue;
        }
        let tag_key = format!("{}_tag", component.id);
        let tag = outputs
            .get(&tag_key)
            .and_then(|value| value.as_str())
            .with_release_context(|| format!("release outputs missing {tag_key}"))?;
        items.push(ReleasePleaseDispatchItem {
            id: component.id.clone(),
            workflow: component.release_workflow.clone(),
            tag: tag.to_owned(),
        });
    }

    Ok(items)
}

pub(super) fn android_fixup(
    root: &Path,
    component: &Component,
    version: &str,
) -> ReleaseResult<()> {
    let version_file = component
        .version_files
        .iter()
        .find(|file| file.kind == VersionKind::GradleVersionName)
        .release_context("android component is missing gradle versionName file")?;
    let path = root.join(&version_file.path);
    let content = std::fs::read_to_string(&path)
        .with_release_context(|| format!("failed to read {}", version_file.path))?;
    if !content.contains("x-release-please-version-code") {
        release_bail!(
            "{} is missing x-release-please-version-code marker",
            version_file.path
        );
    }

    if read_gradle_version_name(&content)? == version
        && version_code_marker_matches(&content, version)
    {
        return Ok(());
    }

    let renamed = replace_gradle_version_name(&content, version)?;
    let updated = stamp_version_code_marker(&increment_gradle_version_code(&renamed)?, version)?;
    std::fs::write(&path, updated)
        .with_release_context(|| format!("failed to write {}", version_file.path))?;
    Ok(())
}

fn version_code_marker_matches(content: &str, version: &str) -> bool {
    content.lines().any(|line| {
        let Some((_, suffix)) = line.split_once("x-release-please-version-code") else {
            return false;
        };
        suffix.split_whitespace().next() == Some(version)
    })
}

fn stamp_version_code_marker(content: &str, version: &str) -> ReleaseResult<String> {
    let marker = regex::Regex::new(r"x-release-please-version-code(?:\s+\S+)?")
        .release_context("invalid release-please versionCode marker regex")?;
    Ok(marker
        .replace_all(content, format!("x-release-please-version-code {version}"))
        .into_owned())
}

fn run_cargo_update(root: &Path, package: &str, version: &str) -> ReleaseResult<()> {
    let status = Command::new("cargo")
        .arg("update")
        .arg("-p")
        .arg(package)
        .arg("--precise")
        .arg(version)
        .current_dir(root)
        .status()
        .with_release_context(|| format!("failed to run cargo update for {package}"))?;
    if !status.success() {
        release_bail!("cargo update -p {package} --precise {version} failed");
    }
    Ok(())
}

fn read_release_please_manifest(
    root: &Path,
) -> ReleaseResult<std::collections::BTreeMap<String, String>> {
    let path = root.join(".release-please-manifest.json");
    let content = std::fs::read_to_string(&path)
        .release_context("failed to read .release-please-manifest.json")?;
    serde_json::from_str(&content).release_context("failed to parse .release-please-manifest.json")
}

fn parse_paths_released(paths_released: &str) -> ReleaseResult<BTreeSet<String>> {
    let value: serde_json::Value =
        serde_json::from_str(paths_released).release_context("failed to parse paths_released")?;
    let paths = match value {
        serde_json::Value::Array(paths) => paths
            .into_iter()
            .map(|value| {
                value
                    .as_str()
                    .map(ToOwned::to_owned)
                    .release_context("paths_released array entries must be strings")
            })
            .collect::<ReleaseResult<_>>()?,
        _ => release_bail!("paths_released must be a JSON array"),
    };
    Ok(paths)
}
