use super::error::{ReleaseContext, ReleaseVersionError};
use super::{BumpLevel, Component, ReleaseResult};
#[cfg(test)]
use super::{VersionFile, VersionKind};
use semver::Version;
use std::path::Path;
use std::process::Command;

// ---------------------------------------------------------------------------
// Pure helpers (no subprocess) — fully unit-testable without git-cliff.
// ---------------------------------------------------------------------------

/// Build git-cliff `--include-path` globs from a component's shipping paths.
/// Each path is emitted as-is (matches a file) and as `<path>/**` (matches a
/// directory subtree), so callers need not know whether a path is a file.
pub(super) fn build_include_paths(component: &Component) -> Vec<String> {
    let mut globs = Vec::with_capacity(component.shipping_paths.len() * 2);
    for path in &component.shipping_paths {
        let trimmed = path.trim_end_matches('/');
        globs.push(trimmed.to_owned());
        globs.push(format!("{trimmed}/**"));
    }
    globs
}

/// Build a git-cliff `--tag-pattern` regex anchored to this component's prefix.
pub(super) fn build_tag_pattern(prefix: &str) -> String {
    format!("^{}", regex_escape(prefix))
}

fn regex_escape(input: &str) -> String {
    const META: &str = r"\.+*?()|[]{}^$";
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if META.contains(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Parse the version printed by `git-cliff --bumped-version`, tolerating an
/// arbitrary leading tag prefix (`v5.17.0`, `palette-v5.11.0`, `5.17.0`).
pub(super) fn parse_cliff_version(output: &str) -> ReleaseResult<Version> {
    let trimmed = output.trim();
    let start = trimmed
        .find(|c: char| c.is_ascii_digit())
        .with_release_context(|| format!("git-cliff produced no version: {trimmed:?}"))?;
    Version::parse(&trimmed[start..])
        .with_release_context(|| format!("git-cliff version is not semver: {trimmed:?}"))
}

/// The bump magnitude git-cliff chose, derived from the delta between the
/// latest released version and git-cliff's bumped version. `None` means no
/// releasable commits. Using the delta (not the absolute value) keeps this
/// robust against git-cliff's prefix handling for custom tag prefixes.
pub(super) fn derive_level(latest: &Version, bumped: &Version) -> Option<BumpLevel> {
    if bumped.major > latest.major {
        Some(BumpLevel::Major)
    } else if bumped.minor > latest.minor {
        Some(BumpLevel::Minor)
    } else if bumped.patch > latest.patch {
        Some(BumpLevel::Patch)
    } else {
        None
    }
}

/// Apply a bump level to a version (shared with `bump()`'s manual path).
pub(super) fn apply_level(current: &Version, level: BumpLevel) -> Version {
    match level {
        BumpLevel::Patch => Version::new(current.major, current.minor, current.patch + 1),
        BumpLevel::Minor => Version::new(current.major, current.minor + 1, 0),
        BumpLevel::Major => Version::new(current.major + 1, 0, 0),
    }
}

/// Apply a bump level to the higher of `current` (version_source) and `latest`
/// (the latest released tag). git-cliff bumps from the latest tag, but a
/// worktree's version_source can lag that tag — bumping from the max prevents a
/// patch from colliding with the existing tag (spike finding).
pub(super) fn resolve_next(
    current: &Version,
    latest: Option<&Version>,
    level: BumpLevel,
) -> Version {
    let baseline = match latest {
        Some(latest) if latest > current => latest,
        _ => current,
    };
    apply_level(baseline, level)
}

/// Pure core: given the authoritative current version, the latest tag version,
/// and git-cliff's raw `--bumped-version` output, compute the next version.
/// git-cliff bumps from the latest tag; we read off its chosen magnitude and
/// re-apply it from `max(current, latest)` so a lagging worktree cannot
/// undershoot. Errors when git-cliff reports nothing to bump.
pub(super) fn next_version_from_outputs(
    current: &Version,
    latest: Option<&Version>,
    cliff_bumped_raw: &str,
    component_id: &str,
) -> ReleaseResult<Version> {
    let bumped = parse_cliff_version(cliff_bumped_raw)?;
    let magnitude_baseline = latest.unwrap_or(current);
    let level = derive_level(magnitude_baseline, &bumped).with_release_context(|| {
        format!(
            "{component_id}: no releasable commits since {magnitude_baseline} (nothing to bump)"
        )
    })?;
    Ok(resolve_next(current, latest, level))
}

// ---------------------------------------------------------------------------
// Subprocess IO — thin wrappers around the `git-cliff` binary.
// ---------------------------------------------------------------------------

/// Run the `git-cliff` binary with `root` as the working directory (so it
/// discovers `cliff.toml` and resolves `--include-path` relative to the repo).
fn run_git_cliff(root: &Path, args: &[&str]) -> ReleaseResult<String> {
    let output = Command::new("git-cliff")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|err| {
            ReleaseVersionError::msg(format!(
                "failed to run git-cliff (install it: `mise use -g git-cliff` \
                 or https://git-cliff.org/docs/installation): {err}"
            ))
        })?;
    if !output.status.success() {
        release_bail!(
            "git-cliff {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn scoping_args<'a>(includes: &'a [String], tag_pattern: &'a str) -> Vec<&'a str> {
    let mut args = vec!["--tag-pattern", tag_pattern];
    for glob in includes {
        args.push("--include-path");
        args.push(glob);
    }
    args
}

/// Compute the next version for a component from unreleased commits.
pub(super) fn next_version(
    root: &Path,
    component: &Component,
    current: &Version,
    latest: Option<&Version>,
) -> ReleaseResult<Version> {
    let includes = build_include_paths(component);
    let tag_pattern = build_tag_pattern(&component.tag_prefix);
    let mut args = scoping_args(&includes, &tag_pattern);
    args.push("--unreleased");
    args.push("--bumped-version");
    let raw = run_git_cliff(root, &args)?;
    next_version_from_outputs(current, latest, &raw, &component.id)
}

/// Prepend the unreleased section (labelled `next`) to the component changelog.
pub(super) fn prepend_changelog(
    root: &Path,
    component: &Component,
    next: &Version,
    changelog_path: &str,
) -> ReleaseResult<()> {
    let includes = build_include_paths(component);
    let tag_pattern = build_tag_pattern(&component.tag_prefix);
    let next_str = next.to_string();
    let mut args = scoping_args(&includes, &tag_pattern);
    args.extend([
        "--unreleased",
        "--tag",
        &next_str,
        "--prepend",
        changelog_path,
    ]);
    run_git_cliff(root, &args)?;
    Ok(())
}

/// Regenerate a full changelog from a component's entire scoped history.
pub(super) fn generate_full_changelog(
    root: &Path,
    component: &Component,
    changelog_path: &str,
) -> ReleaseResult<()> {
    let includes = build_include_paths(component);
    let tag_pattern = build_tag_pattern(&component.tag_prefix);
    let mut args = scoping_args(&includes, &tag_pattern);
    args.extend(["--output", changelog_path]);
    run_git_cliff(root, &args)?;
    Ok(())
}

/// Best-effort suggested bump level for advisory gate output. Returns `None`
/// (never errors) when git-cliff is unavailable, so CI without git-cliff is
/// unaffected.
pub(super) fn suggested_level(
    root: &Path,
    component: &Component,
    current: &Version,
) -> Option<BumpLevel> {
    let includes = build_include_paths(component);
    let tag_pattern = build_tag_pattern(&component.tag_prefix);
    let mut args = scoping_args(&includes, &tag_pattern);
    args.push("--unreleased");
    args.push("--bumped-version");
    let raw = run_git_cliff(root, &args).ok()?;
    let bumped = parse_cliff_version(&raw).ok()?;
    derive_level(current, &bumped)
}

#[cfg(test)]
#[path = "cliff_tests.rs"]
mod tests;
