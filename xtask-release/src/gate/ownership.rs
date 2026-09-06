use regex::Regex;
use std::collections::BTreeSet;
use std::path::Path;

use super::super::files::{
    read_cargo_lock_package_version, read_cargo_package_version, read_gradle_version_code,
    read_gradle_version_name, read_json_version, read_npm_package_lock_version,
};
use super::super::git::git_show;
use super::super::{Component, ReleaseContext, ReleaseResult, VersionFile, VersionKind};

pub(super) struct ManagedPrChanges {
    pub(super) version_fields: Vec<String>,
    pub(super) ordinary: Vec<String>,
}

pub(super) fn classify(
    root: &Path,
    component: &Component,
    compare_ref: &str,
    head: &str,
    changed: Vec<String>,
) -> ReleaseResult<ManagedPrChanges> {
    let mut version_fields = BTreeSet::new();
    let mut ordinary = BTreeSet::new();

    for path in changed {
        let files = component
            .version_files
            .iter()
            .filter(|file| file.path == path)
            .collect::<Vec<_>>();
        if files.is_empty() {
            ordinary.insert(path);
            continue;
        }

        let before = git_show(root, compare_ref, &path)?;
        let after = git_show(root, head, &path)?;
        let field_changed = version_field_changed(&files, &before, &after)?;
        if field_changed {
            version_fields.insert(path.clone());
        }
        if ordinary_content_changed(&files, &before, &after, field_changed)? {
            ordinary.insert(path);
        }
    }

    Ok(ManagedPrChanges {
        version_fields: version_fields.into_iter().collect(),
        ordinary: ordinary.into_iter().collect(),
    })
}

fn version_field_changed(files: &[&VersionFile], before: &str, after: &str) -> ReleaseResult<bool> {
    for file in files {
        if version_field_value(file, before)? != version_field_value(file, after)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn version_field_value(file: &VersionFile, content: &str) -> ReleaseResult<Option<String>> {
    let value = match file.kind {
        VersionKind::CargoPackage => read_cargo_package_version(content, file.package.as_deref())?,
        VersionKind::CargoLockPackage => {
            read_cargo_lock_package_version(content, file.package.as_deref())?
        }
        VersionKind::ReadmeVersionLine => read_readme_version(content)?,
        VersionKind::ChangelogHeading => release_headings(content).join("\n"),
        VersionKind::JsonVersion => read_json_version(content, file.json_pointer.as_deref())?,
        VersionKind::JsonNoVersion => return Ok(None),
        VersionKind::NpmPackageLock => {
            read_npm_package_lock_version(content, file.package.as_deref())?
        }
        VersionKind::GradleVersionName => read_gradle_version_name(content)?,
        VersionKind::GradleVersionCode => read_gradle_version_code(content)?.to_string(),
    };
    Ok(Some(value))
}

fn ordinary_content_changed(
    files: &[&VersionFile],
    before: &str,
    after: &str,
    field_changed: bool,
) -> ReleaseResult<bool> {
    if files
        .iter()
        .any(|file| file.kind == VersionKind::ChangelogHeading)
    {
        // A new semver heading identifies the whole generated release entry as
        // release-please output. With no heading change, prose-only edits are
        // ordinary content and do not claim release ownership.
        return Ok(!field_changed && before != after);
    }

    Ok(normalize_non_version_content(files, before)?
        != normalize_non_version_content(files, after)?)
}

fn normalize_non_version_content(files: &[&VersionFile], content: &str) -> ReleaseResult<String> {
    if files.iter().any(|file| {
        matches!(
            file.kind,
            VersionKind::JsonVersion | VersionKind::JsonNoVersion | VersionKind::NpmPackageLock
        )
    }) {
        return normalize_json(files, content);
    }
    if files.iter().any(|file| {
        matches!(
            file.kind,
            VersionKind::GradleVersionName | VersionKind::GradleVersionCode
        )
    }) {
        return normalize_gradle(content);
    }
    if files
        .iter()
        .any(|file| file.kind == VersionKind::CargoPackage)
    {
        return normalize_cargo_manifest(files, content);
    }
    if files
        .iter()
        .any(|file| file.kind == VersionKind::CargoLockPackage)
    {
        return normalize_cargo_lock(files, content);
    }
    if files
        .iter()
        .any(|file| file.kind == VersionKind::ReadmeVersionLine)
    {
        return normalize_readme(content);
    }
    Ok(content.to_owned())
}

fn normalize_json(files: &[&VersionFile], content: &str) -> ReleaseResult<String> {
    let mut value: serde_json::Value =
        serde_json::from_str(content).release_context("invalid JSON version file")?;
    for file in files {
        match file.kind {
            VersionKind::JsonVersion => {
                let pointer = file.json_pointer.as_deref().unwrap_or("/version");
                let field = value
                    .pointer_mut(pointer)
                    .with_release_context(|| format!("missing JSON version field at {pointer}"))?;
                *field = serde_json::Value::String("__VERSION__".to_owned());
            }
            VersionKind::NpmPackageLock => {
                let root_version = value
                    .get_mut("version")
                    .release_context("missing package-lock root version")?;
                *root_version = serde_json::Value::String("__VERSION__".to_owned());
                let package_version = value
                    .pointer_mut("/packages//version")
                    .release_context("missing package-lock packages[''] version")?;
                *package_version = serde_json::Value::String("__VERSION__".to_owned());
            }
            VersionKind::JsonNoVersion => {}
            _ => {}
        }
    }
    serde_json::to_string(&value).release_context("failed to normalize JSON version file")
}

fn normalize_gradle(content: &str) -> ReleaseResult<String> {
    let name = Regex::new(r#"(?m)^(\s*versionName\s*=\s*)"[^"]+""#)
        .release_context("invalid versionName normalization regex")?;
    let code = Regex::new(r"(?m)^(\s*versionCode\s*=\s*)\d+")
        .release_context("invalid versionCode normalization regex")?;
    let marker = Regex::new(r"x-release-please-version-code(?:\s+\S+)?")
        .release_context("invalid versionCode marker normalization regex")?;
    let normalized = name.replace_all(content, r#"${1}"__VERSION__""#);
    let normalized = code.replace_all(&normalized, "${1}1");
    Ok(marker
        .replace_all(&normalized, "x-release-please-version-code __VERSION__")
        .into_owned())
}

fn normalize_cargo_manifest(files: &[&VersionFile], content: &str) -> ReleaseResult<String> {
    let mut value: toml::Value =
        toml::from_str(content).release_context("invalid Cargo version manifest")?;
    for file in files {
        if file.kind != VersionKind::CargoPackage {
            continue;
        }
        let package = value
            .get_mut("package")
            .and_then(toml::Value::as_table_mut)
            .release_context("missing [package] table")?;
        package.insert(
            "version".to_owned(),
            toml::Value::String("__VERSION__".to_owned()),
        );
    }
    toml::to_string(&value).release_context("failed to normalize Cargo version manifest")
}

fn normalize_cargo_lock(files: &[&VersionFile], content: &str) -> ReleaseResult<String> {
    let packages = files
        .iter()
        .filter(|file| file.kind == VersionKind::CargoLockPackage)
        .map(|file| {
            file.package
                .as_deref()
                .release_context("cargo_lock_package requires package")
        })
        .collect::<ReleaseResult<BTreeSet<_>>>()?;
    let version = Regex::new(r#"(?m)^(\s*version\s*=\s*)"[^"]+""#)
        .release_context("invalid Cargo.lock version normalization regex")?;
    let mut normalized = String::new();
    for (index, section) in content.split("[[package]]").enumerate() {
        if index > 0 {
            normalized.push_str("[[package]]");
        }
        if section.lines().any(|line| {
            packages
                .iter()
                .any(|package| line.trim() == format!(r#"name = "{package}""#))
        }) {
            normalized.push_str(&version.replace(section, r#"${1}"__VERSION__""#));
        } else {
            normalized.push_str(section);
        }
    }
    Ok(normalized)
}

fn read_readme_version(content: &str) -> ReleaseResult<String> {
    let regex =
        Regex::new(r"(?m)^Version:\s*(\S+)").release_context("invalid README version regex")?;
    regex
        .captures(content)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_owned())
        .release_context("missing 'Version:' line")
}

fn normalize_readme(content: &str) -> ReleaseResult<String> {
    let regex =
        Regex::new(r"(?m)^Version:\s*\S+\b(?:\s+<!-- x-release-please-version -->)?[ \t]*$")
            .release_context("invalid README version normalization regex")?;
    if !regex.is_match(content) {
        return Err(super::super::ReleaseVersionError::msg(
            "missing 'Version:' line",
        ));
    }
    Ok(regex.replace(content, "Version: __VERSION__").into_owned())
}

fn release_headings(content: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| line.strip_prefix("## ["))
        .filter_map(|line| line.split_once(']').map(|(version, _)| version))
        .filter(|version| semver::Version::parse(version).is_ok())
        .map(ToOwned::to_owned)
        .collect()
}
