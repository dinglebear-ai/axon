use std::collections::BTreeSet;
use std::path::Path;

use super::super::{Component, ReleaseContext, ReleaseResult, VersionKind};
use super::read_release_please_manifest;

pub(super) fn validate(root: &Path, components: &[Component]) -> ReleaseResult<()> {
    let all_component_paths = components
        .iter()
        .map(|component| component.release_please_path.clone())
        .collect::<BTreeSet<_>>();
    if all_component_paths.len() != components.len() {
        let duplicate = components
            .iter()
            .find_map(|component| {
                (components
                    .iter()
                    .filter(|candidate| {
                        candidate.release_please_path == component.release_please_path
                    })
                    .count()
                    > 1)
                .then_some(component.release_please_path.as_str())
            })
            .unwrap_or("<unknown>");
        release_bail!("duplicate release_please_path {duplicate}");
    }

    let config_path = root.join("release-please-config.json");
    let config_content = std::fs::read_to_string(&config_path)
        .release_context("failed to read release-please-config.json")?;
    let config: serde_json::Value = serde_json::from_str(&config_content)
        .release_context("failed to parse release-please-config.json")?;
    let config_packages = config
        .get("packages")
        .and_then(serde_json::Value::as_object)
        .release_context("release-please-config.json is missing object field packages")?;
    let config_paths = config_packages.keys().cloned().collect::<BTreeSet<_>>();
    let manifest_paths = read_release_please_manifest(root)?
        .into_keys()
        .collect::<BTreeSet<_>>();
    let managed_paths = components
        .iter()
        .filter(|component| component.release_driver.is_release_please())
        .map(|component| component.release_please_path.clone())
        .collect::<BTreeSet<_>>();

    validate_owner_paths("release-please-config.json", &managed_paths, &config_paths)?;
    validate_release_please_tag_prefixes(components, config_packages)?;
    validate_release_please_extra_files(components, config_packages)?;
    validate_owner_paths(
        ".release-please-manifest.json",
        &managed_paths,
        &manifest_paths,
    )
}

fn validate_release_please_extra_files(
    components: &[Component],
    config_packages: &serde_json::Map<String, serde_json::Value>,
) -> ReleaseResult<()> {
    for component in components
        .iter()
        .filter(|component| component.release_driver.is_release_please())
    {
        let package = config_packages
            .get(&component.release_please_path)
            .and_then(serde_json::Value::as_object)
            .with_release_context(|| {
                format!(
                    "release-please-config.json package {} must be an object",
                    component.release_please_path
                )
            })?;
        let expected = component
            .version_files
            .iter()
            .filter(|file| directly_updated_by_release_please(file.kind))
            .map(|file| file.path.clone())
            .collect::<BTreeSet<_>>();
        let extra_files: &[serde_json::Value] = match package.get("extra-files") {
            Some(value) => value.as_array().with_release_context(|| {
                format!(
                    "release-please-config.json package {} field extra-files must be an array",
                    component.release_please_path
                )
            })?,
            None => &[],
        };
        let mut actual = BTreeSet::new();
        for (index, extra_file) in extra_files.iter().enumerate() {
            let path = extra_file
                .as_object()
                .and_then(|entry| entry.get("path"))
                .and_then(serde_json::Value::as_str)
                .with_release_context(|| {
                    format!(
                        "release-please-config.json package {} extra-files[{index}] is missing string field path",
                        component.release_please_path
                    )
                })?;
            let resolved =
                resolve_release_please_extra_file_path(&component.release_please_path, path)?;
            if !actual.insert(resolved.clone()) {
                release_bail!(
                    "release-please-config.json package {} declares duplicate extra-file path {resolved}",
                    component.release_please_path
                );
            }
        }
        validate_owner_paths(
            &format!(
                "release-please-config.json package {} extra-files",
                component.release_please_path
            ),
            &expected,
            &actual,
        )?;
    }
    Ok(())
}

fn directly_updated_by_release_please(kind: VersionKind) -> bool {
    !matches!(
        kind,
        VersionKind::CargoLockPackage
            | VersionKind::ChangelogHeading
            | VersionKind::JsonNoVersion
            | VersionKind::GradleVersionCode
    )
}

fn resolve_release_please_extra_file_path(package_path: &str, path: &str) -> ReleaseResult<String> {
    let mut segments = Vec::new();
    if package_path != "." {
        append_release_path_segments(&mut segments, package_path, "package path")?;
    }
    append_release_path_segments(&mut segments, path, "extra-file path")?;
    if segments.is_empty() {
        release_bail!("release-please extra-file path must name a file");
    }
    Ok(segments.join("/"))
}

fn append_release_path_segments(
    output: &mut Vec<String>,
    path: &str,
    label: &str,
) -> ReleaseResult<()> {
    if path.starts_with('/') || path.contains('\\') {
        release_bail!(
            "release-please {label} must be a canonical repository-relative path: {path}"
        );
    }
    for segment in path.split('/') {
        match segment {
            "" | ".." => release_bail!(
                "release-please {label} must be a canonical repository-relative path: {path}"
            ),
            "." => {}
            segment => output.push(segment.to_owned()),
        }
    }
    Ok(())
}

fn validate_release_please_tag_prefixes(
    components: &[Component],
    config_packages: &serde_json::Map<String, serde_json::Value>,
) -> ReleaseResult<()> {
    for component in components
        .iter()
        .filter(|component| component.release_driver.is_release_please())
    {
        let package = config_packages
            .get(&component.release_please_path)
            .and_then(serde_json::Value::as_object)
            .with_release_context(|| {
                format!(
                    "release-please-config.json package {} must be an object",
                    component.release_please_path
                )
            })?;
        let release_component = package
            .get("component")
            .and_then(serde_json::Value::as_str)
            .with_release_context(|| {
                format!(
                    "release-please-config.json package {} is missing string field component",
                    component.release_please_path
                )
            })?;
        let include_v = package
            .get("include-v-in-tag")
            .and_then(serde_json::Value::as_bool)
            .with_release_context(|| {
                format!(
                    "release-please-config.json package {} is missing boolean field include-v-in-tag",
                    component.release_please_path
                )
            })?;
        let include_component = match package.get("include-component-in-tag") {
            Some(value) => value.as_bool().with_release_context(|| {
                format!(
                    "release-please-config.json package {} field include-component-in-tag must be boolean",
                    component.release_please_path
                )
            })?,
            None => true,
        };
        let separator = package
            .get("tag-separator")
            .and_then(serde_json::Value::as_str)
            .with_release_context(|| {
                format!(
                    "release-please-config.json package {} is missing string field tag-separator",
                    component.release_please_path
                )
            })?;
        let version_prefix = if include_v { "v" } else { "" };
        let derived_prefix = if include_component {
            format!("{release_component}{separator}{version_prefix}")
        } else {
            version_prefix.to_owned()
        };
        if derived_prefix != component.tag_prefix {
            release_bail!(
                "release-please-config.json package {} derives tag prefix {}, expected {} from release/components.toml",
                component.release_please_path,
                derived_prefix,
                component.tag_prefix
            );
        }
    }
    Ok(())
}

fn validate_owner_paths(
    source: &str,
    expected: &BTreeSet<String>,
    actual: &BTreeSet<String>,
) -> ReleaseResult<()> {
    let missing = expected.difference(actual).cloned().collect::<Vec<_>>();
    let unexpected = actual.difference(expected).cloned().collect::<Vec<_>>();
    if !missing.is_empty() || !unexpected.is_empty() {
        release_bail!(
            "{source} ownership does not match release/components.toml: missing [{}]; unexpected [{}]",
            missing.join(", "),
            unexpected.join(", ")
        );
    }
    Ok(())
}
