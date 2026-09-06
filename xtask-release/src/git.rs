use super::files::read_gradle_version_code;
use super::{Component, GateMode, ReleaseContext, ReleaseResult, VersionKind};
use semver::Version;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
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
    let mut command = git_command_without_local_env()?;
    let output = command
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
    let changed: Vec<String> =
        changed_paths_since_ref(root, base, head, &component.shipping_paths)?
            .into_iter()
            .filter(|path| !is_changelog_path(path))
            .filter(|path| !is_non_shipping_documentation_path(path))
            .collect();
    if changed.is_empty() {
        return Ok(false);
    }

    if component.id == "android"
        && changed
            .iter()
            .all(|path| path == "apps/android/app/build.gradle.kts")
        && android_release_please_marker_only(root, base, head)?
    {
        return Ok(false);
    }

    if component.id == "chrome"
        && chrome_release_please_asset_bootstrap_only(root, base, head, &changed)?
    {
        return Ok(false);
    }

    if component.id == "cli"
        && changed.iter().all(|path| path == "Cargo.lock")
        && cargo_lock_only_non_shipping_packages_changed(root, component, base, head)?
    {
        return Ok(false);
    }

    Ok(true)
}

fn android_release_please_marker_only(root: &Path, base: &str, head: &str) -> ReleaseResult<bool> {
    let path = "apps/android/app/build.gradle.kts";
    let before = git_show(root, base, path)?;
    let after = git_show(root, head, path)?;
    Ok(strip_android_release_please_markers(&before)
        == strip_android_release_please_markers(&after))
}

fn strip_android_release_please_markers(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed != "// x-release-please-start-version" && trimmed != "// x-release-please-end"
        })
        .map(|line| {
            line.strip_suffix(" // x-release-please-version-code")
                .unwrap_or(line)
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn chrome_release_please_asset_bootstrap_only(
    root: &Path,
    base: &str,
    head: &str,
    changed: &[String],
) -> ReleaseResult<bool> {
    if !changed.iter().all(|path| {
        path == "apps/chrome-extension/package.sh"
            || path == "apps/chrome-extension/assets"
            || path.starts_with("apps/chrome-extension/assets/")
    }) {
        return Ok(false);
    }

    if changed
        .iter()
        .any(|path| path == "apps/chrome-extension/package.sh")
        && !non_comment_shell_equal(root, base, head, "apps/chrome-extension/package.sh")?
    {
        return Ok(false);
    }

    let asset_paths = changed
        .iter()
        .filter(|path| {
            path.as_str() == "apps/chrome-extension/assets"
                || path.starts_with("apps/chrome-extension/assets/")
        })
        .collect::<Vec<_>>();
    if asset_paths.is_empty() {
        return Ok(true);
    }

    let tree = git_output(root, &["ls-tree", base, "apps/chrome-extension/assets"])?;
    if !tree.starts_with("120000 blob") {
        return Ok(false);
    }

    for path in asset_paths {
        if path.as_str() == "apps/chrome-extension/assets" {
            continue;
        }
        let top_level = path.replacen("apps/chrome-extension/assets/", "assets/", 1);
        let after = fs::read(root.join(path))
            .with_release_context(|| format!("failed to read {}", path))?;
        let original = fs::read(root.join(&top_level))
            .with_release_context(|| format!("failed to read {}", top_level))?;
        if after != original {
            return Ok(false);
        }
    }

    Ok(true)
}

fn non_comment_shell_equal(root: &Path, base: &str, head: &str, path: &str) -> ReleaseResult<bool> {
    let before = git_show(root, base, path)?;
    let after = git_show(root, head, path)?;
    Ok(shell_without_comments(&before) == shell_without_comments(&after))
}

fn shell_without_comments(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect()
}

/// A component's own `CHANGELOG.md` lives inside its shipping paths, but editing
/// it must not count as a shipping change — otherwise documenting a release
/// would re-trigger one. (The cli changelog at the repo root is already outside
/// shipping paths; this keeps the rule uniform across components.)
fn is_changelog_path(path: &str) -> bool {
    path == "CHANGELOG.md" || path.ends_with("/CHANGELOG.md")
}

fn is_non_shipping_documentation_path(path: &str) -> bool {
    path == "README.md"
        || path == "CLAUDE.md"
        || path == "AGENTS.md"
        || path == "GEMINI.md"
        || path.ends_with("/README.md")
        || path.ends_with("/CLAUDE.md")
        || path.ends_with("/AGENTS.md")
        || path.ends_with("/GEMINI.md")
        || path.starts_with("docs/")
}

pub(super) fn changed_paths_since_ref(
    root: &Path,
    base: &str,
    head: &str,
    paths: &[String],
) -> ReleaseResult<Vec<String>> {
    let mut command = git_command_without_local_env()?;
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
    let mut command = git_command_without_local_env()?;
    let output = command
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

/// Build a Git command that honors its explicit `-C <root>` target even when
/// xtask is launched from a Git hook. Git exports repository-local variables
/// such as `GIT_DIR`, `GIT_WORK_TREE`, and `GIT_INDEX_FILE` to hooks; those
/// variables take precedence over `-C` and can silently redirect fixture or
/// comparison commands back into the caller's real worktree.
fn git_command_without_local_env() -> ReleaseResult<Command> {
    let local_env = Command::new("git")
        .args(["rev-parse", "--local-env-vars"])
        .output()
        .release_context("failed to list repository-local Git environment variables")?;
    if !local_env.status.success() {
        release_bail!(
            "git rev-parse --local-env-vars failed: {}",
            String::from_utf8_lossy(&local_env.stderr).trim()
        );
    }

    let mut command = Command::new("git");
    for variable in String::from_utf8_lossy(&local_env.stdout)
        .lines()
        .filter(|variable| !variable.is_empty())
    {
        command.env_remove(variable);
    }
    Ok(command)
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
    let current_content = fs::read_to_string(root.join(&file.path))
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

pub(super) fn git_show(root: &Path, reference: &str, path: &str) -> ReleaseResult<String> {
    git_output(root, &["show", &format!("{reference}:{path}")])
}

/// True when a `Cargo.lock`-only diff touches nothing that actually ships.
///
/// Dev tooling lives in workspace members outside every shipping path
/// (`xtask`, `xtask-release`), so adding or re-wiring one churns the root
/// lockfile without changing a single shipped byte. Membership is derived from
/// the workspace manifest rather than named here, so a new tooling package
/// needs no change to this gate.
///
/// Any changed package that is not such a member — a shipping crate, or a
/// third-party dependency — fails the carve-out and still requires a bump.
fn cargo_lock_only_non_shipping_packages_changed(
    root: &Path,
    component: &Component,
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
        .filter_map(|package_id| package_id.split('|').next())
        .collect::<Vec<_>>();
    if changed.is_empty() {
        return Ok(false);
    }
    let non_shipping = non_shipping_workspace_packages(root, component)?;
    Ok(changed.iter().all(|name| non_shipping.contains(*name)))
}

/// Names of workspace members whose directory sits outside every one of
/// `component`'s shipping paths.
fn non_shipping_workspace_packages(
    root: &Path,
    component: &Component,
) -> ReleaseResult<BTreeSet<String>> {
    let mut packages = BTreeSet::new();
    for member in workspace_members(root)? {
        if component
            .shipping_paths
            .iter()
            .any(|shipping| path_is_within(&member, shipping))
        {
            continue;
        }
        if let Some(name) = member_package_name(root, &member)? {
            packages.insert(name);
        }
    }
    Ok(packages)
}

fn workspace_members(root: &Path) -> ReleaseResult<Vec<String>> {
    let manifest = fs::read_to_string(root.join("Cargo.toml"))
        .release_context("failed to read the workspace Cargo.toml")?;
    let manifest: toml::Value =
        toml::from_str(&manifest).release_context("failed to parse the workspace Cargo.toml")?;
    Ok(manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
        .map(|members| {
            members
                .iter()
                .filter_map(toml::Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default())
}

fn member_package_name(root: &Path, member: &str) -> ReleaseResult<Option<String>> {
    let path = root.join(member).join("Cargo.toml");
    if !path.exists() {
        return Ok(None);
    }
    let manifest = fs::read_to_string(&path)
        .with_release_context(|| format!("failed to read {member}/Cargo.toml"))?;
    let manifest: toml::Value = toml::from_str(&manifest)
        .with_release_context(|| format!("failed to parse {member}/Cargo.toml"))?;
    Ok(manifest
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
        .map(ToOwned::to_owned))
}

fn path_is_within(path: &str, ancestor: &str) -> bool {
    let path = path.trim_end_matches('/');
    let ancestor = ancestor.trim_end_matches('/');
    path == ancestor || path.starts_with(&format!("{ancestor}/"))
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

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
