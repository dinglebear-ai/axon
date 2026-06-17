use super::files::read_gradle_version_code;
use super::{Component, GateMode, ReleaseContext, ReleaseResult, VersionKind};
use semver::Version;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

pub(super) fn latest_tag(root: &Path, prefix: &str) -> ReleaseResult<Option<String>> {
    let output = git_output(root, &["tag", "-l", &format!("{prefix}*")])?;
    let mut candidates = Vec::new();
    for tag in output.lines().filter(|line| !line.trim().is_empty()) {
        let Some(version) = tag.strip_prefix(prefix) else {
            continue;
        };
        if let Ok(version) = Version::parse(version) {
            candidates.push((version, tag.to_owned()));
        }
    }
    candidates.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(candidates.pop().map(|(_, tag)| tag))
}

pub(super) fn tag_exists(root: &Path, tag: &str) -> ReleaseResult<bool> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "-q", "--verify"])
        .arg(format!("refs/tags/{tag}"))
        .output()
        .with_release_context(|| format!("failed to check tag {tag}"))?;
    Ok(output.status.success())
}

pub(super) fn component_changed_since_ref(
    root: &Path,
    component: &Component,
    base: &str,
    head: &str,
) -> ReleaseResult<bool> {
    let changed = changed_paths_since_ref(root, base, head, &component.shipping_paths)?;
    if changed.is_empty() {
        return Ok(false);
    }

    if component.id == "cli"
        && changed.iter().all(|path| path == "Cargo.lock")
        && cargo_lock_only_xtask_package_changed(root, base, head)?
    {
        return Ok(false);
    }

    Ok(true)
}

fn changed_paths_since_ref(
    root: &Path,
    base: &str,
    head: &str,
    paths: &[String],
) -> ReleaseResult<Vec<String>> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(root)
        .args(["diff", "--name-only"])
        .arg(format!("{base}..{head}"))
        .arg("--")
        .args(paths);
    let output = command
        .output()
        .with_release_context(|| format!("failed to diff {base}..{head}"))?;
    if !output.status.success() {
        release_bail!(
            "git diff failed for {base}..{head}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

pub(super) fn merge_base(root: &Path, base: &str, head: &str) -> ReleaseResult<String> {
    git_output(root, &["merge-base", base, head]).map(|output| output.trim().to_owned())
}

fn git_output(root: &Path, args: &[&str]) -> ReleaseResult<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .with_release_context(|| format!("failed to run git {args:?}"))?;
    if !output.status.success() {
        release_bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub(super) fn compare_ref_for_component(
    root: &Path,
    component: &Component,
    base: Option<&str>,
    head: &str,
    mode: GateMode,
) -> ReleaseResult<Option<String>> {
    match mode {
        GateMode::Pr => Ok(Some(match base {
            Some(base) => merge_base(root, base, head)?,
            None => {
                merge_base(root, "origin/main", head).unwrap_or_else(|_| "origin/main".to_owned())
            }
        })),
        GateMode::Main => Ok(latest_tag(root, &component.tag_prefix)?),
    }
}

pub(super) fn check_gradle_version_code_increased(
    root: &Path,
    component: &Component,
    compare_ref: &str,
) -> ReleaseResult<()> {
    let Some(file) = component
        .version_files
        .iter()
        .find(|file| file.kind == VersionKind::GradleVersionCode)
    else {
        return Ok(());
    };
    let current_content = std::fs::read_to_string(root.join(&file.path))
        .with_release_context(|| format!("failed to read {}", file.path))?;
    let current = read_gradle_version_code(&current_content)?;
    let previous_content = git_show(root, compare_ref, &file.path)
        .with_release_context(|| format!("failed to read previous {}", file.path))?;
    let previous = read_gradle_version_code(&previous_content).with_release_context(|| {
        format!(
            "failed to parse previous versionCode in {} at {}",
            file.path, compare_ref
        )
    })?;
    if current <= previous {
        release_bail!(
            "{} versionCode must increase when Android shipping paths change ({} <= {})",
            file.path,
            current,
            previous
        );
    }
    Ok(())
}

fn git_show(root: &Path, reference: &str, path: &str) -> ReleaseResult<String> {
    git_output(root, &["show", &format!("{reference}:{path}")])
}

fn cargo_lock_only_xtask_package_changed(
    root: &Path,
    base: &str,
    head: &str,
) -> ReleaseResult<bool> {
    let before = git_show(root, base, "Cargo.lock")?;
    let after = git_show(root, head, "Cargo.lock")?;
    let before = cargo_lock_package_sections(&before);
    let after = cargo_lock_package_sections(&after);
    let mut package_ids = before.keys().chain(after.keys()).collect::<Vec<_>>();
    package_ids.sort();
    package_ids.dedup();
    let changed = package_ids
        .into_iter()
        .filter(|package_id| before.get(*package_id) != after.get(*package_id))
        .map(|package_id| package_id.as_str())
        .collect::<Vec<_>>();
    Ok(changed.len() == 1 && changed[0].starts_with("xtask|"))
}

fn cargo_lock_package_sections(content: &str) -> BTreeMap<String, String> {
    let mut packages = BTreeMap::new();
    for section in content.split("[[package]]").skip(1) {
        if let Some(package_id) = cargo_lock_package_id(section) {
            packages.insert(package_id, section.trim().to_owned());
        }
    }
    packages
}

fn cargo_lock_package_id(section: &str) -> Option<String> {
    let name = cargo_lock_field(section, "name")?;
    let version = cargo_lock_field(section, "version").unwrap_or_default();
    let source = cargo_lock_field(section, "source").unwrap_or_default();
    Some(format!("{name}|{version}|{source}"))
}

fn cargo_lock_field(section: &str, key: &str) -> Option<String> {
    let prefix = format!("{key} = ");
    section.lines().find_map(|line| {
        line.trim()
            .strip_prefix(&prefix)
            .and_then(|value| value.trim().strip_prefix('"')?.strip_suffix('"'))
            .map(ToOwned::to_owned)
    })
}
