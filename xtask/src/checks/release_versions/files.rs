use super::{Component, ReleaseContext, ReleaseResult, VersionFile, VersionKind};
use regex::Regex;
use std::path::Path;
use std::process::Command;

mod readme;

pub(super) const MAX_ANDROID_VERSION_CODE: u64 = 2_100_000_000;

pub(super) fn read_version(root: &Path, file: &VersionFile) -> ReleaseResult<String> {
    let path = root.join(&file.path);
    let content = std::fs::read_to_string(&path)
        .with_release_context(|| format!("failed to read {}", file.path))?;
    match file.kind {
        VersionKind::CargoPackage => read_cargo_package_version(&content, file.package.as_deref())
            .with_release_context(|| {
                format!("failed to read Cargo package version from {}", file.path)
            }),
        VersionKind::CargoLockPackage => {
            read_cargo_lock_package_version(&content, file.package.as_deref()).with_release_context(
                || {
                    format!(
                        "failed to read Cargo.lock package version from {}",
                        file.path
                    )
                },
            )
        }
        VersionKind::JsonVersion => read_json_version(&content, file.json_pointer.as_deref())
            .with_release_context(|| format!("failed to read JSON version from {}", file.path)),
        VersionKind::NpmPackageLock => {
            read_npm_package_lock_version(&content, file.package.as_deref()).with_release_context(
                || format!("failed to read npm package-lock version from {}", file.path),
            )
        }
        VersionKind::GradleVersionName => read_gradle_version_name(&content)
            .with_release_context(|| format!("failed to read versionName from {}", file.path)),
        VersionKind::ReadmeVersionLine
        | VersionKind::ChangelogHeading
        | VersionKind::JsonNoVersion
        | VersionKind::GradleVersionCode => {
            release_bail!("{:?} is not a canonical version source", file.kind)
        }
    }
}

pub(super) fn check_component_parity(
    root: &Path,
    component: &Component,
    expected: &str,
) -> ReleaseResult<Vec<String>> {
    let mut errors = Vec::new();
    for file in &component.version_files {
        let content = match std::fs::read_to_string(root.join(&file.path)) {
            Ok(content) => content,
            Err(error) => {
                errors.push(format!("{}: failed to read: {error}", file.path));
                continue;
            }
        };
        let result = match file.kind {
            VersionKind::CargoPackage => {
                check_cargo_package_version(&content, file.package.as_deref(), expected)
            }
            VersionKind::CargoLockPackage => {
                check_cargo_lock_package_version(&content, file.package.as_deref(), expected)
            }
            VersionKind::ReadmeVersionLine => readme::check_version_line(&content, expected),
            VersionKind::ChangelogHeading => check_changelog_heading(&content, expected),
            VersionKind::JsonVersion => {
                check_json_version(&content, file.json_pointer.as_deref(), expected)
            }
            VersionKind::JsonNoVersion => check_json_no_version(&content),
            VersionKind::NpmPackageLock => {
                check_npm_package_lock_version(&content, file.package.as_deref(), expected)
            }
            VersionKind::GradleVersionName => check_gradle_version_name(&content, expected),
            VersionKind::GradleVersionCode => check_gradle_version_code_present(&content),
        };
        if let Err(error) = result {
            errors.push(format!("{}: {error}", file.path));
        }
    }
    Ok(errors)
}

pub(super) fn read_cargo_package_version(
    content: &str,
    package: Option<&str>,
) -> ReleaseResult<String> {
    let value: toml::Value = toml::from_str(content).release_context("invalid TOML")?;
    let package_table = value
        .get("package")
        .and_then(|value| value.as_table())
        .release_context("missing [package] table")?;
    if let Some(expected_name) = package {
        let name = package_table
            .get("name")
            .and_then(|value| value.as_str())
            .release_context("missing package.name")?;
        if name != expected_name {
            release_bail!("expected package {expected_name}, found {name}");
        }
    }
    package_table
        .get("version")
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .release_context("missing package.version")
}

/// Read `[workspace.package] version` if the manifest declares one.
///
/// Returns `Ok(None)` when there is no `[workspace.package]` table or it carries
/// no string `version` (e.g. a workspace that shares only `edition`). Used to
/// enforce that the workspace-inherited version — which every extracted crate
/// picks up via `version.workspace = true`, and therefore every crate's
/// `CARGO_PKG_VERSION` — stays equal to the product's `[package] version`.
pub(super) fn read_workspace_package_version(content: &str) -> ReleaseResult<Option<String>> {
    let value: toml::Value = toml::from_str(content).release_context("invalid TOML")?;
    Ok(value
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(|version| version.as_str())
        .map(ToOwned::to_owned))
}

pub(super) fn read_cargo_lock_package_version(
    content: &str,
    package: Option<&str>,
) -> ReleaseResult<String> {
    let package = package.release_context("cargo_lock_package requires package")?;
    let section = cargo_lock_package_section(content, package)
        .with_release_context(|| format!("missing Cargo.lock package {package}"))?;
    cargo_lock_field(section, "version")
        .with_release_context(|| format!("missing Cargo.lock package {package} version"))
}

pub(super) fn read_json_version(content: &str, pointer: Option<&str>) -> ReleaseResult<String> {
    let value: serde_json::Value = serde_json::from_str(content).release_context("invalid JSON")?;
    let pointer = pointer.unwrap_or("/version");
    value
        .pointer(pointer)
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .with_release_context(|| format!("missing JSON version field at {pointer}"))
}

pub(super) fn read_npm_package_lock_version(
    content: &str,
    package: Option<&str>,
) -> ReleaseResult<String> {
    let package = package.release_context("npm_package_lock requires package")?;
    let value: serde_json::Value = serde_json::from_str(content).release_context("invalid JSON")?;
    let root_name = value
        .get("name")
        .and_then(|value| value.as_str())
        .release_context("missing package-lock root name")?;
    if root_name != package {
        release_bail!("expected package-lock package {package}, found {root_name}");
    }
    let root_version = value
        .get("version")
        .and_then(|value| value.as_str())
        .release_context("missing package-lock root version")?;
    let package_version = value
        .pointer("/packages/")
        .and_then(|value| value.get("version"))
        .and_then(|value| value.as_str())
        .release_context("missing package-lock packages[''] version")?;
    if root_version != package_version {
        release_bail!(
            "package-lock root version {root_version} does not match packages[''] version {package_version}"
        );
    }
    Ok(root_version.to_owned())
}

pub(super) fn read_gradle_version_name(content: &str) -> ReleaseResult<String> {
    let regex = Regex::new(r#"(?m)^\s*versionName\s*=\s*"([^"]+)""#)
        .release_context("invalid versionName regex")?;
    regex
        .captures(content)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_owned())
        .release_context("missing versionName")
}

pub(super) fn read_gradle_version_code(content: &str) -> ReleaseResult<u64> {
    let regex = Regex::new(r#"(?m)^\s*versionCode\s*=\s*(\d+)"#)
        .release_context("invalid versionCode regex")?;
    regex
        .captures(content)
        .and_then(|captures| captures.get(1))
        .release_context("missing versionCode")?
        .as_str()
        .parse()
        .release_context("versionCode is not an integer")
        .and_then(validate_gradle_version_code)
}

fn check_cargo_package_version(
    content: &str,
    package: Option<&str>,
    expected: &str,
) -> ReleaseResult<()> {
    let actual = read_cargo_package_version(content, package)?;
    if actual != expected {
        release_bail!("expected package version {expected}, found {actual}");
    }
    Ok(())
}

fn check_cargo_lock_package_version(
    content: &str,
    package: Option<&str>,
    expected: &str,
) -> ReleaseResult<()> {
    let actual = read_cargo_lock_package_version(content, package)?;
    if actual != expected {
        release_bail!("expected Cargo.lock package version {expected}, found {actual}");
    }
    Ok(())
}

fn check_changelog_heading(content: &str, expected: &str) -> ReleaseResult<()> {
    let expected = format!("## [{expected}]");
    if !content.lines().any(|line| line.starts_with(&expected)) {
        release_bail!("missing '{expected}' heading");
    }
    Ok(())
}

fn check_json_version(content: &str, pointer: Option<&str>, expected: &str) -> ReleaseResult<()> {
    let actual = read_json_version(content, pointer)?;
    if actual != expected {
        release_bail!("expected JSON version {expected}, found {actual}");
    }
    Ok(())
}

fn check_npm_package_lock_version(
    content: &str,
    package: Option<&str>,
    expected: &str,
) -> ReleaseResult<()> {
    let actual = read_npm_package_lock_version(content, package)?;
    if actual != expected {
        release_bail!("expected package-lock version {expected}, found {actual}");
    }
    Ok(())
}

fn check_json_no_version(content: &str) -> ReleaseResult<()> {
    let value: serde_json::Value = serde_json::from_str(content).release_context("invalid JSON")?;
    if contains_json_version_key(&value) {
        release_bail!("must not contain a version key");
    }
    Ok(())
}

fn contains_json_version_key(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => {
            map.contains_key("version") || map.values().any(contains_json_version_key)
        }
        serde_json::Value::Array(values) => values.iter().any(contains_json_version_key),
        _ => false,
    }
}

fn check_gradle_version_name(content: &str, expected: &str) -> ReleaseResult<()> {
    let actual = read_gradle_version_name(content)?;
    if actual != expected {
        release_bail!("expected versionName {expected}, found {actual}");
    }
    Ok(())
}

fn check_gradle_version_code_present(content: &str) -> ReleaseResult<()> {
    read_gradle_version_code(content)?;
    Ok(())
}

pub(super) fn replace_json_version(
    content: &str,
    pointer: Option<&str>,
    next: &str,
) -> ReleaseResult<String> {
    let pointer = pointer.unwrap_or("/version");
    let mut value: serde_json::Value =
        serde_json::from_str(content).release_context("invalid JSON")?;
    let target = value
        .pointer_mut(pointer)
        .with_release_context(|| format!("missing JSON version field at {pointer}"))?;
    if !target.is_string() {
        release_bail!("JSON version field at {pointer} is not a string");
    }
    *target = serde_json::Value::String(next.to_owned());
    serde_json::to_string_pretty(&value).release_context("failed to serialize JSON")
}

pub(super) fn replace_cargo_package_version(
    content: &str,
    package: Option<&str>,
    next: &str,
) -> ReleaseResult<String> {
    read_cargo_package_version(content, package)?;
    // The root manifest also carries `[workspace.package] version`, inherited by
    // every extracted crate via `version.workspace = true`. Bump it in lockstep
    // with `[package]` so the workspace version — and therefore every crate's
    // `CARGO_PKG_VERSION` — tracks the product version. Crate manifests without a
    // `[workspace.package]` table are unaffected. The two sections are tracked
    // separately so a half-bump (one section's version present but not rewritten)
    // is caught rather than silently passing once `[package]` alone is bumped.
    let has_workspace_version = read_workspace_package_version(content)?.is_some();
    let mut in_package = false;
    let mut in_workspace_package = false;
    let mut package_replaced = false;
    let mut workspace_replaced = false;
    let mut output = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[package]" {
            in_package = true;
            in_workspace_package = false;
        } else if trimmed == "[workspace.package]" {
            in_workspace_package = true;
            in_package = false;
        } else if trimmed.starts_with('[') {
            in_package = false;
            in_workspace_package = false;
        }

        let mut next_line = line.to_owned();
        if (in_package || in_workspace_package)
            && trimmed.starts_with("version")
            && extract_toml_string_assignment(trimmed).is_some()
        {
            let leading = &line[..line.len() - line.trim_start().len()];
            next_line = format!(r#"{leading}version = "{next}""#);
            if in_package {
                package_replaced = true;
            } else {
                workspace_replaced = true;
            }
        }
        output.push(next_line);
    }
    if !package_replaced {
        release_bail!("missing Cargo package version");
    }
    if has_workspace_version && !workspace_replaced {
        release_bail!(
            "[workspace.package] version present but not bumped; refusing a half-bump that \
             would drift the workspace-inherited version from the package version"
        );
    }
    Ok(preserve_trailing_newline(content, output.join("\n")))
}

pub(super) fn replace_cargo_lock_package_version(
    content: &str,
    package: Option<&str>,
    next: &str,
) -> ReleaseResult<String> {
    let package = package.release_context("cargo_lock_package requires package")?;
    read_cargo_lock_package_version(content, Some(package))?;
    let mut output = Vec::new();
    let mut active_package: Option<String> = None;
    let mut replaced = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "[[package]]" {
            active_package = None;
        } else if let Some(name) = cargo_lock_assignment(trimmed, "name") {
            active_package = Some(name);
        }

        let mut next_line = line.to_owned();
        if active_package.as_deref() == Some(package)
            && trimmed.starts_with("version")
            && cargo_lock_assignment(trimmed, "version").is_some()
        {
            let leading = &line[..line.len() - line.trim_start().len()];
            next_line = format!(r#"{leading}version = "{next}""#);
            replaced = true;
        }
        output.push(next_line);
    }

    if !replaced {
        release_bail!("missing Cargo.lock package {package} version");
    }
    Ok(preserve_trailing_newline(content, output.join("\n")))
}

fn extract_toml_string_assignment(line: &str) -> Option<&str> {
    let (_, value) = line.split_once('=')?;
    value.trim().strip_prefix('"')?.strip_suffix('"')
}

pub(super) fn replace_gradle_version_name(content: &str, next: &str) -> ReleaseResult<String> {
    let regex = Regex::new(r#"(?m)^(\s*versionName\s*=\s*)"[^"]+""#)
        .release_context("invalid versionName replacement regex")?;
    if !regex.is_match(content) {
        release_bail!("missing versionName");
    }
    Ok(regex.replace(content, format!(r#"$1"{next}""#)).to_string())
}

pub(super) fn increment_gradle_version_code(content: &str) -> ReleaseResult<String> {
    let regex = Regex::new(r#"(?m)^(\s*versionCode\s*=\s*)(\d+)"#)
        .release_context("invalid versionCode replacement regex")?;
    let captures = regex
        .captures(content)
        .release_context("missing versionCode")?;
    let current: u64 = captures[2]
        .parse()
        .release_context("versionCode is not an integer")?;
    validate_gradle_version_code(current)?;
    let next = current
        .checked_add(1)
        .release_context("versionCode overflowed while bumping")?;
    validate_gradle_version_code(next)?;
    Ok(regex.replace(content, format!("${{1}}{next}")).to_string())
}

pub(super) fn replace_readme_version_line(content: &str, next: &str) -> ReleaseResult<String> {
    readme::replace_version_line(content, next)
}

pub(super) fn ensure_changelog_heading(content: &str, next: &str) -> ReleaseResult<String> {
    let heading = format!("## [{next}]");
    if content.lines().any(|line| line.starts_with(&heading)) {
        return Ok(content.to_owned());
    }
    let date = release_date()?;
    let block = format!("## [{next}] - {date}\n\n### Changed\n- Release version bump.\n\n");
    let mut lines = content.lines();
    let Some(first) = lines.next() else {
        return Ok(format!("# Changelog\n\n{block}"));
    };
    Ok(if first.starts_with("# ") {
        let rest = lines.collect::<Vec<_>>().join("\n");
        if rest.trim().is_empty() {
            format!("{first}\n\n{block}")
        } else {
            preserve_trailing_newline(content, format!("{first}\n\n{block}{}", rest.trim_start()))
        }
    } else {
        format!("{block}{content}")
    })
}

fn release_date() -> ReleaseResult<String> {
    let output = Command::new("date")
        .arg("+%F")
        .output()
        .release_context("failed to run date +%F")?;
    if !output.status.success() {
        release_bail!(
            "date +%F failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let date = String::from_utf8(output.stdout).release_context("date +%F returned non-UTF-8")?;
    let date = date.trim();
    if !is_iso_date(date) {
        release_bail!("date +%F returned invalid date: {date}");
    }
    Ok(date.to_owned())
}

pub(super) fn replace_npm_package_lock_version(
    content: &str,
    package: Option<&str>,
    next: &str,
) -> ReleaseResult<String> {
    let package = package.release_context("npm_package_lock requires package")?;
    read_npm_package_lock_version(content, Some(package))?;
    let mut value: serde_json::Value =
        serde_json::from_str(content).release_context("invalid JSON")?;
    *value
        .get_mut("version")
        .release_context("missing package-lock root version")? =
        serde_json::Value::String(next.to_owned());
    *value
        .pointer_mut("/packages//version")
        .release_context("missing package-lock packages[''] version")? =
        serde_json::Value::String(next.to_owned());
    serde_json::to_string_pretty(&value).release_context("failed to serialize package-lock")
}

fn validate_gradle_version_code(code: u64) -> ReleaseResult<u64> {
    if code == 0 || code > MAX_ANDROID_VERSION_CODE {
        release_bail!("versionCode {code} must be between 1 and {MAX_ANDROID_VERSION_CODE}");
    }
    Ok(code)
}

fn cargo_lock_package_section<'a>(content: &'a str, package: &str) -> Option<&'a str> {
    content
        .split("[[package]]")
        .skip(1)
        .find(|section| cargo_lock_field(section, "name").as_deref() == Some(package))
}

fn cargo_lock_field(section: &str, key: &str) -> Option<String> {
    section
        .lines()
        .find_map(|line| cargo_lock_assignment(line.trim(), key))
}

fn cargo_lock_assignment(line: &str, key: &str) -> Option<String> {
    let prefix = format!("{key} = ");
    line.strip_prefix(&prefix)
        .and_then(|value| value.trim().strip_prefix('"')?.strip_suffix('"'))
        .map(ToOwned::to_owned)
}

fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}

fn preserve_trailing_newline(original: &str, mut updated: String) -> String {
    if original.ends_with('\n') && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated
}
