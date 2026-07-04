use std::collections::HashSet;
use std::path::Path;

use super::{Component, ReleaseResult, VersionFile, VersionKind};

pub(super) fn validate_manifest(root: &Path, manifest: &super::Manifest) -> ReleaseResult<()> {
    let mut component_ids = HashSet::new();
    let mut tag_prefixes: Vec<&str> = Vec::new();

    for component in &manifest.components {
        if component.id.trim().is_empty() {
            release_bail!("release manifest contains an empty component id");
        }
        if !component_ids.insert(component.id.as_str()) {
            release_bail!("duplicate release component id {}", component.id);
        }
        if component.tag_prefix.trim().is_empty() {
            release_bail!("{} has an empty tag_prefix", component.id);
        }
        if component.release_please_path.trim().is_empty() {
            release_bail!("{} has an empty release_please_path", component.id);
        }
        if component.release_please_path != "."
            && !root.join(&component.release_please_path).is_dir()
        {
            release_bail!(
                "{} release_please_path does not exist: {}",
                component.id,
                component.release_please_path
            );
        }
        if tag_prefixes.iter().any(|existing| {
            existing.starts_with(&component.tag_prefix)
                || component.tag_prefix.starts_with(*existing)
        }) {
            release_bail!("{} tag_prefix overlaps another component", component.id);
        }
        tag_prefixes.push(&component.tag_prefix);
        if component.shipping_paths.is_empty() {
            release_bail!("{} has no shipping_paths", component.id);
        }
        for path in &component.shipping_paths {
            if !root.join(path).exists() {
                release_bail!("{} shipping path does not exist: {path}", component.id);
            }
        }
        if !component.release_workflow.ends_with(".yml")
            && !component.release_workflow.ends_with(".yaml")
        {
            release_bail!("{} release_workflow must be a YAML workflow", component.id);
        }
        if !root
            .join(".github/workflows")
            .join(&component.release_workflow)
            .is_file()
        {
            release_bail!(
                "{} release_workflow does not exist: {}",
                component.id,
                component.release_workflow
            );
        }
        validate_version_file(component, "version_source", &component.version_source)?;
        for file in &component.version_files {
            validate_version_file(component, "version_files", file)?;
            if !root.join(&file.path).is_file() {
                release_bail!(
                    "{} version file does not exist: {}",
                    component.id,
                    file.path
                );
            }
        }
        if !component
            .version_files
            .iter()
            .any(|file| same_version_file(file, &component.version_source))
        {
            release_bail!(
                "{} version_source is not listed in version_files",
                component.id
            );
        }
    }

    Ok(())
}

fn validate_version_file(
    component: &Component,
    field: &str,
    file: &VersionFile,
) -> ReleaseResult<()> {
    match file.kind {
        VersionKind::CargoPackage | VersionKind::CargoLockPackage | VersionKind::NpmPackageLock => {
            if file.package.as_deref().unwrap_or("").trim().is_empty() {
                release_bail!(
                    "{} {field} {} {:?} requires package",
                    component.id,
                    file.path,
                    file.kind
                );
            }
            if file.json_pointer.is_some() {
                release_bail!(
                    "{} {field} {} {:?} must not set json_pointer",
                    component.id,
                    file.path,
                    file.kind
                );
            }
        }
        VersionKind::JsonVersion => {
            if file.package.is_some() {
                release_bail!(
                    "{} {field} {} json_version must not set package",
                    component.id,
                    file.path
                );
            }
            let pointer = file.json_pointer.as_deref().unwrap_or("");
            if !pointer.starts_with('/') {
                release_bail!(
                    "{} {field} {} json_version requires an absolute json_pointer",
                    component.id,
                    file.path
                );
            }
        }
        _ => {
            if file.package.is_some() {
                release_bail!(
                    "{} {field} {} {:?} must not set package",
                    component.id,
                    file.path,
                    file.kind
                );
            }
            if file.json_pointer.is_some() {
                release_bail!(
                    "{} {field} {} {:?} must not set json_pointer",
                    component.id,
                    file.path,
                    file.kind
                );
            }
        }
    }
    Ok(())
}

pub(super) fn same_version_file(left: &VersionFile, right: &VersionFile) -> bool {
    left.kind == right.kind
        && left.path == right.path
        && left.package == right.package
        && left.json_pointer == right.json_pointer
}
