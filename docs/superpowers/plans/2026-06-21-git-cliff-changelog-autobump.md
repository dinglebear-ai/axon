# git-cliff Changelogs + Auto-Bump Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `cargo xtask bump-version` auto-derive the version bump from conventional commits and write real, per-component changelogs via git-cliff, without changing the existing auto-tag/release flow.

**Architecture:** A new `cliff.toml` (bump rules + Keep-a-Changelog template) plus a new `xtask/src/checks/release_versions/cliff.rs` module that shells out to the installed `git-cliff` binary, scoped per component via `--include-path` (from `shipping_paths`) and `--tag-pattern` (from `tag_prefix`). Pure parsing/decision logic is split from subprocess IO so the test suite never requires git-cliff. `bump()` gains an optional level (auto-derived when omitted) and routes the changelog through git-cliff; change-detection ignores `CHANGELOG.md` so documenting a release can't re-trigger one.

**Tech Stack:** Rust (xtask), `git-cliff` 2.13.1 (external binary), `semver`, `tempfile` (tests). Config: `cliff.toml`, `release/components.toml`.

**Tracking:** bead `axon_rust-qbvn`. Design spec: `docs/superpowers/specs/2026-06-21-git-cliff-changelog-autobump-design.md`.

## Global Constraints

- **Module layout:** never `mod.rs`. New submodule is `xtask/src/checks/release_versions/cliff.rs`, declared `mod cliff;` inside `release_versions.rs` *after* the `release_bail!` macro definition (so the macro is in scope).
- **Tests:** sidecar `_tests.rs` with `#[cfg(test)] #[path = "..."] mod tests;`. Use `use super::*;`. One sidecar per original test module.
- **Monolith policy:** non-test files ≤500 lines; functions warn at 80, hard-fail at 120 lines. `**/*_tests.*` is exempt.
- **CI must not require git-cliff.** `cargo xtask check-release-versions` and `cargo test` run in CI where git-cliff is absent. Any test that spawns the real `git-cliff` binary MUST early-return when it is not on `PATH`. The PR-gate suggested-level hint MUST be best-effort (silently omitted when git-cliff is missing).
- **No version bumps in this change.** It touches only `xtask`, `cliff.toml`, `release/`, `docs/`, and `CHANGELOG.md` files — none of which (after the change-detection fix) mark a component as shipping-changed. Do not bump any component version.
- **Errors:** use the module's `release_bail!` macro and `ReleaseResult` / `with_release_context` / `ReleaseVersionError::msg` patterns. No `unwrap`/`expect` in non-test code.
- **Formatting/lint:** `cargo fmt` and `cargo clippy` clean before each commit.
- **Commit messages:** conventional commits (`feat:`, `fix:`, `test:`, `docs:`, `chore:`). These are dev-tooling changes; `feat:`/`fix:` here do NOT ship the CLI (xtask is outside cli shipping paths).

---

### Task 1: Spike — verify git-cliff scoping & bump against the real tags

**Goal:** Confirm the exact git-cliff invocation works per component (especially the non-`v` prefixes `palette-v`, `chrome-ext-v`) before building on it. No code commit; record findings in the bead.

**Files:** none (investigation only).

- [ ] **Step 1: Confirm git-cliff sees each component's latest tag via tag-pattern**

Run (from repo root), for each component:
```bash
# cli
git-cliff --tag-pattern '^v[0-9]' --include-path 'src/**' --include-path 'Cargo.toml' --unreleased --bumped-version || echo "no unreleased / error"
# palette
git-cliff --tag-pattern '^palette-v' --include-path 'apps/palette-tauri/**' --unreleased --bumped-version || echo "no unreleased / error"
# android
git-cliff --tag-pattern '^android-v' --include-path 'apps/android/**' --unreleased --bumped-version || echo "no unreleased / error"
# chrome
git-cliff --tag-pattern '^chrome-ext-v' --include-path 'apps/chrome-extension/**' --include-path 'assets/**' --unreleased --bumped-version || echo "no unreleased / error"
```

Expected: each prints a semver-looking string (possibly prefixed, e.g. `v5.17.0` or `palette-v5.11.0`) **or** a "no unreleased changes" message — NOT a bump from `0.0.0`. A bump from `0.0.0` means the tag-pattern did not match that component's tags.

- [ ] **Step 2: Record the actual output shape**

Note in the bead (`bd update axon_rust-qbvn --notes "..."`) for each component: does `--bumped-version` emit a bare semver, or prefixed? Does the tag-pattern correctly anchor on the latest component tag? This confirms that `parse_cliff_version` (strip-to-first-digit) + `derive_level` (delta vs `latest_tag`) is sufficient, or whether the tag-pattern regex needs adjustment.

- [ ] **Step 3: Decision gate**

If any component bumps from `0.0.0` (tag-pattern miss), adjust that component's `build_tag_pattern` output in Task 3 accordingly and re-run this step. Do not proceed to Task 4 until all four components resolve against their own latest tag.

---

### Task 2: Add `cliff.toml` with bump rules + Keep-a-Changelog template

**Goal:** A single git-cliff config that renders sections byte-compatible with the existing `CHANGELOG.md` and bumps per the `feat!`/`feat`/`fix` convention.

**Files:**
- Create: `cliff.toml`

**Interfaces:**
- Produces: a `cliff.toml` at repo root that git-cliff auto-discovers via `current_dir(root)` (used by `cliff.rs` in later tasks). Per-component `--include-path`/`--tag-pattern` are passed on the CLI, not in this file.

- [ ] **Step 1: Write `cliff.toml`**

```toml
# git-cliff configuration for Axon's per-component changelogs.
# Scoping (--include-path / --tag-pattern) is passed per-invocation by
# `cargo xtask bump-version`; this file holds only formatting + bump rules.

[changelog]
header = """
# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
"""
# NOTE: git-cliff renders this body ONCE PER RELEASE — `version`/`timestamp`/
# `commits` are the current release's fields. Do NOT wrap in
# `{% for release in releases %}` (that variable is empty in per-release context
# and yields a header-only changelog). Structure copied from git-cliff's
# canonical `keepachangelog` template, with per-component prefix stripping added.
body = """
{% if version -%}
    ## [{{ version | replace(from="palette-v", to="") | replace(from="android-v", to="") | replace(from="chrome-ext-v", to="") | trim_start_matches(pat="v") }}] - {{ timestamp | date(format="%Y-%m-%d") }}
{% else -%}
    ## [Unreleased]
{% endif -%}
{% for group, commits in commits | group_by(attribute="group") %}
    ### {{ group | upper_first }}
    {% for commit in commits %}
        - {{ commit.message | split(pat="\\n") | first | upper_first | trim }}\
    {% endfor %}
{% endfor %}\\n
"""

[git]
conventional_commits = true
filter_unconventional = true
filter_commits = true
commit_parsers = [
  { message = "^feat", group = "Added" },
  { message = "^fix", group = "Fixed" },
  { message = "^perf", group = "Changed" },
  { message = "^refactor", group = "Changed" },
  { message = "^chore", skip = true },
  { message = "^ci", skip = true },
  { message = "^docs", skip = true },
  { message = "^test", skip = true },
  { message = "^build", skip = true },
  { message = "^style", skip = true },
  { message = "^Merge", skip = true },
]

[bump]
features_always_bump_minor = true
breaking_always_bump_major = true
```

- [ ] **Step 2: Verify the rendered format matches the existing changelog**

Run:
```bash
git-cliff --tag-pattern '^v[0-9]' --include-path 'src/**' --include-path 'Cargo.toml' --include-path 'Cargo.lock' --include-path 'build.rs' --include-path 'migrations/**' --include-path 'apps/web/**' --include-path 'rust-toolchain.toml' --include-path 'vendor/**' --unreleased --tag 5.99.0 2>/dev/null | head -20
```
Expected: output begins with `## [5.99.0] - <today>` followed by `### Added` / `### Fixed` style sections (no `v` prefix in the heading, matching the existing `## [5.16.5]` format), and each bullet is a clean description without the `feat:`/`fix:` type prefix.

**Fallback:** if bullets still contain the type prefix (e.g. `- feat: add x`), git-cliff in this version puts the full summary in `commit.message`. Run `git-cliff --init keepachangelog` in a scratch dir to get the canonical template, copy its `[changelog] body` (which uses the conventional-commit fields correctly), and re-apply the per-prefix `replace(...)` chain to the version heading line. Re-run this step to confirm.

- [ ] **Step 3: Commit**

```bash
git add cliff.toml
git commit -m "chore: add git-cliff config for per-component changelogs"
```

---

### Task 3: Pure helpers in `cliff.rs` (scoping, parsing, level derivation)

**Goal:** The testable core — build scoping args, parse git-cliff's version output, decide the bump magnitude — with zero subprocess dependency.

**Files:**
- Create: `xtask/src/checks/release_versions/cliff.rs`
- Create: `xtask/src/checks/release_versions/cliff_tests.rs`
- Modify: `xtask/src/checks/release_versions.rs` (add `mod cliff;` after the macro)

**Interfaces:**
- Consumes: `Component`, `BumpLevel`, `ReleaseResult`, `ReleaseContext`, `ReleaseVersionError` from `super` (re-exported in `release_versions.rs`).
- Produces (used by Task 4 & Task 6 & Task 8):
  - `fn build_include_paths(component: &Component) -> Vec<String>`
  - `fn build_tag_pattern(prefix: &str) -> String`
  - `fn parse_cliff_version(output: &str) -> ReleaseResult<Version>`
  - `fn derive_level(latest: &Version, bumped: &Version) -> Option<BumpLevel>`
  - `fn apply_level(current: &Version, level: BumpLevel) -> Version`
  - `fn resolve_next(current: &Version, latest: Option<&Version>, level: BumpLevel) -> Version` — bumps from `max(current, latest)` so a worktree whose `version_source` lags the latest tag cannot undershoot (spike finding).
  - `fn next_version_from_outputs(current: &Version, latest: Option<&Version>, cliff_bumped_raw: &str, component_id: &str) -> ReleaseResult<Version>`

- [ ] **Step 1: Declare the module**

In `xtask/src/checks/release_versions.rs`, add `mod cliff;` alongside the other `mod` declarations (they are all below the `release_bail!` macro, so this is correct):

```rust
mod cliff;
mod error;
mod files;
mod git;
mod manifest;
```

- [ ] **Step 2: Write the failing tests**

Create `xtask/src/checks/release_versions/cliff_tests.rs` (`use super::*;` resolves cliff's helpers plus `Component`/`VersionFile`/`VersionKind`/`BumpLevel`/`Version`, which cliff.rs brings into scope):

```rust
use super::*;

fn component(prefix: &str, paths: &[&str]) -> Component {
    Component {
        id: "test".to_owned(),
        name: "Test".to_owned(),
        tag_prefix: prefix.to_owned(),
        release_workflow: "release.yml".to_owned(),
        shipping_paths: paths.iter().map(|p| p.to_string()).collect(),
        version_source: VersionFile {
            kind: VersionKind::CargoPackage,
            path: "Cargo.toml".to_owned(),
            package: Some("axon".to_owned()),
            json_pointer: None,
        },
        version_files: Vec::new(),
    }
}

#[test]
fn include_paths_emit_file_and_dir_globs() {
    let c = component("v", &["src", "Cargo.toml"]);
    assert_eq!(
        build_include_paths(&c),
        vec![
            "src".to_owned(),
            "src/**".to_owned(),
            "Cargo.toml".to_owned(),
            "Cargo.toml/**".to_owned(),
        ]
    );
}

#[test]
fn tag_pattern_is_anchored_and_escaped() {
    // Hyphens are literal outside a character class (not escaped); regex
    // metacharacters would be. Unescaped form is spike-verified.
    assert_eq!(build_tag_pattern("v"), "^v");
    assert_eq!(build_tag_pattern("palette-v"), "^palette-v");
    assert_eq!(build_tag_pattern("chrome-ext-v"), "^chrome-ext-v");
    assert_eq!(build_tag_pattern("a.b"), "^a\\.b");
}

#[test]
fn parse_cliff_version_tolerates_prefixes() {
    assert_eq!(parse_cliff_version("5.17.0").unwrap().to_string(), "5.17.0");
    assert_eq!(parse_cliff_version("v5.17.0\n").unwrap().to_string(), "5.17.0");
    assert_eq!(
        parse_cliff_version(" palette-v5.11.0 ").unwrap().to_string(),
        "5.11.0"
    );
    assert!(parse_cliff_version("not-a-version").is_err());
}

#[test]
fn derive_level_picks_highest_change() {
    let v = |s: &str| Version::parse(s).unwrap();
    assert_eq!(derive_level(&v("5.16.5"), &v("6.0.0")), Some(BumpLevel::Major));
    assert_eq!(derive_level(&v("5.16.5"), &v("5.17.0")), Some(BumpLevel::Minor));
    assert_eq!(derive_level(&v("5.16.5"), &v("5.16.6")), Some(BumpLevel::Patch));
    assert_eq!(derive_level(&v("5.16.5"), &v("5.16.5")), None);
}

#[test]
fn resolve_next_bumps_from_max_of_source_and_tag() {
    let v = |s: &str| Version::parse(s).unwrap();
    // Normal: source == tag.
    assert_eq!(
        resolve_next(&v("5.16.6"), Some(&v("5.16.6")), BumpLevel::Minor).to_string(),
        "5.17.0"
    );
    // Stale worktree: source (5.16.5) lags tag (5.16.6) -> bump from the tag,
    // so a patch yields 5.16.7 (not a collision at 5.16.6).
    assert_eq!(
        resolve_next(&v("5.16.5"), Some(&v("5.16.6")), BumpLevel::Patch).to_string(),
        "5.16.7"
    );
    // Source ahead of tag (manual pre-bump): bump from the source.
    assert_eq!(
        resolve_next(&v("5.17.0"), Some(&v("5.16.6")), BumpLevel::Patch).to_string(),
        "5.17.1"
    );
    // No tag: bump from the source.
    assert_eq!(
        resolve_next(&v("0.2.1"), None, BumpLevel::Major).to_string(),
        "1.0.0"
    );
}

#[test]
fn next_version_uses_git_cliff_magnitude_from_max_baseline() {
    let v = |s: &str| Version::parse(s).unwrap();
    // git-cliff bumped tag 5.16.6 -> 5.17.0 (minor); source 5.16.5 lags the tag.
    let next =
        next_version_from_outputs(&v("5.16.5"), Some(&v("5.16.6")), "v5.17.0", "cli").unwrap();
    assert_eq!(next.to_string(), "5.17.0");
}

#[test]
fn next_version_patch_does_not_collide_with_tag_on_stale_worktree() {
    let v = |s: &str| Version::parse(s).unwrap();
    // git-cliff patch-bumped tag 5.16.6 -> 5.16.7; source 5.16.5 lags.
    // Applying to max(source, tag) gives 5.16.7, not a 5.16.6 collision.
    let next =
        next_version_from_outputs(&v("5.16.5"), Some(&v("5.16.6")), "v5.16.7", "cli").unwrap();
    assert_eq!(next.to_string(), "5.16.7");
}

#[test]
fn next_version_tolerates_custom_prefix_output() {
    let v = |s: &str| Version::parse(s).unwrap();
    let next = next_version_from_outputs(
        &v("5.10.5"),
        Some(&v("5.10.5")),
        "palette-v5.11.0",
        "palette",
    )
    .unwrap();
    assert_eq!(next.to_string(), "5.11.0");
}

#[test]
fn next_version_errors_when_no_releasable_commits() {
    let v = |s: &str| Version::parse(s).unwrap();
    // git-cliff echoes the latest tag unchanged ("nothing to bump").
    let err = next_version_from_outputs(&v("5.16.6"), Some(&v("5.16.6")), "v5.16.6", "cli");
    assert!(err.is_err());
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p xtask cliff::tests 2>&1 | tail -20`
Expected: FAIL — `cannot find function build_include_paths` etc.

- [ ] **Step 4: Write `cliff.rs` pure helpers**

Create `xtask/src/checks/release_versions/cliff.rs`:

```rust
use super::error::{ReleaseContext, ReleaseVersionError};
use super::{BumpLevel, Component, ReleaseResult};
// Test-only: brought into cliff's scope so `use super::*;` in cliff_tests.rs
// resolves these without re-importing (avoids E0252 duplicate imports).
#[cfg(test)]
use super::{VersionFile, VersionKind};
use semver::Version;
use std::path::Path;
use std::process::Command;

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
pub(super) fn resolve_next(current: &Version, latest: Option<&Version>, level: BumpLevel) -> Version {
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
        format!("{component_id}: no releasable commits since {magnitude_baseline} (nothing to bump)")
    })?;
    Ok(resolve_next(current, latest, level))
}

#[cfg(test)]
#[path = "cliff_tests.rs"]
mod tests;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p xtask cliff::tests 2>&1 | tail -20`
Expected: PASS (7 tests).

- [ ] **Step 6: Commit**

```bash
cargo fmt && cargo clippy -p xtask
git add xtask/src/checks/release_versions.rs xtask/src/checks/release_versions/cliff.rs xtask/src/checks/release_versions/cliff_tests.rs
git commit -m "feat: add git-cliff scoping and bump-derivation helpers to xtask"
```

---

### Task 4: git-cliff subprocess IO (next_version, prepend, full regen)

**Goal:** Thin IO wrappers that invoke the real `git-cliff` binary, plus one PATH-gated end-to-end test proving the wiring against a temp repo.

**Files:**
- Modify: `xtask/src/checks/release_versions/cliff.rs` (append IO functions)
- Modify: `xtask/src/checks/release_versions/cliff_tests.rs` (append gated integration test)

**Interfaces:**
- Consumes: `build_include_paths`, `build_tag_pattern`, `next_version_from_outputs` (Task 3).
- Produces (used by Task 6 & Task 8):
  - `fn next_version(root: &Path, component: &Component, current: &Version, latest: Option<&Version>) -> ReleaseResult<Version>`
  - `fn prepend_changelog(root: &Path, component: &Component, next: &Version, changelog_path: &str) -> ReleaseResult<()>`
  - `fn generate_full_changelog(root: &Path, component: &Component, changelog_path: &str) -> ReleaseResult<()>`
  - `fn suggested_level(root: &Path, component: &Component, current: &Version) -> Option<BumpLevel>`

- [ ] **Step 1: Append IO functions to `cliff.rs`**

```rust
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
    args.extend(["--unreleased", "--tag", &next_str, "--prepend", changelog_path]);
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
```

- [ ] **Step 2: Append the PATH-gated integration test to `cliff_tests.rs`**

`Path` and `Command` already arrive via `use super::*;` (cliff.rs imports them), so only `TempDir` is newly imported here:

```rust
use tempfile::TempDir;

fn git_cliff_available() -> bool {
    Command::new("git-cliff")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed");
}

#[test]
fn next_version_end_to_end_when_git_cliff_present() {
    if !git_cliff_available() {
        eprintln!("skipping next_version_end_to_end: git-cliff not on PATH");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    // Minimal cliff.toml with the bump rules.
    std::fs::write(
        root.join("cliff.toml"),
        "[git]\nconventional_commits = true\nfilter_unconventional = true\n\
         [bump]\nfeatures_always_bump_minor = true\nbreaking_always_bump_major = true\n",
    )
    .unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "t@t"]);
    git(root, &["config", "user.name", "t"]);
    std::fs::write(root.join("src/a.rs"), "// a").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "chore: init"]);
    git(root, &["tag", "v1.2.3"]);
    std::fs::write(root.join("src/b.rs"), "// b").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "feat: add b"]);

    let c = component("v", &["src", "Cargo.toml"]);
    let current = Version::parse("1.2.3").unwrap();
    let latest = Version::parse("1.2.3").unwrap();
    let next = next_version(root, &c, &current, Some(&latest)).unwrap();
    assert_eq!(next.to_string(), "1.3.0", "a feat must bump the minor");
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p xtask cliff::tests 2>&1 | tail -20`
Expected: PASS. The integration test runs (git-cliff present locally) and asserts `1.3.0`; in CI it prints the skip line and returns.

- [ ] **Step 4: Commit**

```bash
cargo fmt && cargo clippy -p xtask
git add xtask/src/checks/release_versions/cliff.rs xtask/src/checks/release_versions/cliff_tests.rs
git commit -m "feat: add git-cliff subprocess wrappers for version + changelog"
```

---

### Task 5: Exclude `CHANGELOG.md` from change detection (self-trigger fix)

**Goal:** A commit that only touches a component's `CHANGELOG.md` must not count as a shipping change (otherwise documenting a release re-triggers one).

**Files:**
- Modify: `xtask/src/checks/release_versions/git.rs` (`component_changed_since_ref`)
- Modify: `xtask/src/checks/release_versions/git_tests.rs` (create if absent; else append) — see note.

**Note on test sidecar:** `git.rs` currently has no `#[cfg(test)] mod tests`. Add one declaration at the bottom of `git.rs` and a new sidecar `git_tests.rs`.

**Interfaces:**
- Produces: `fn is_changelog_path(path: &str) -> bool` (private to `git.rs`).

- [ ] **Step 1: Write the failing test**

Create `xtask/src/checks/release_versions/git_tests.rs`:

```rust
use super::*;

#[test]
fn changelog_paths_are_recognized() {
    assert!(is_changelog_path("CHANGELOG.md"));
    assert!(is_changelog_path("apps/android/CHANGELOG.md"));
    assert!(is_changelog_path("apps/palette-tauri/CHANGELOG.md"));
    assert!(!is_changelog_path("src/lib.rs"));
    assert!(!is_changelog_path("docs/CHANGELOG.md.bak"));
    assert!(!is_changelog_path("apps/android/app/build.gradle.kts"));
}
```

At the bottom of `git.rs` add:

```rust
#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p xtask release_versions::git::tests 2>&1 | tail -20`
Expected: FAIL — `cannot find function is_changelog_path`.

- [ ] **Step 3: Implement the exclusion**

In `git.rs`, add the helper and filter the changed paths inside `component_changed_since_ref`:

```rust
fn is_changelog_path(path: &str) -> bool {
    path == "CHANGELOG.md" || path.ends_with("/CHANGELOG.md")
}
```

Change `component_changed_since_ref` so the changed list drops changelog-only edits:

```rust
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
            .collect();
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
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p xtask release_versions::git::tests 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p xtask
git add xtask/src/checks/release_versions/git.rs xtask/src/checks/release_versions/git_tests.rs
git commit -m "fix: ignore CHANGELOG.md edits in release change detection"
```

---

### Task 6: Wire auto-bump into `bump()` + optional level + `--skip-changelog`

**Goal:** `cargo xtask bump-version <component>` auto-derives the level and writes a real changelog; `<level>` still overrides; `--skip-changelog` falls back to the old stamp; missing git-cliff fails clearly.

**Files:**
- Modify: `xtask/src/checks/release_versions.rs` (`bump` signature + body)
- Modify: `xtask/src/main.rs` (`BumpVersion` args + dispatch)

**Interfaces:**
- Consumes: `cliff::{next_version, prepend_changelog, apply_level}`, existing `latest_tag`, `read_version`, the `replace_*` writers.
- Produces: `pub fn bump(root: &Path, component_id: &str, level: Option<BumpLevel>, skip_changelog: bool) -> ReleaseResult<()>`.

- [ ] **Step 1: Update `bump()` in `release_versions.rs`**

Replace the existing `bump` function body. Compute `next` first (auto or manual), then route the changelog through git-cliff:

```rust
pub fn bump(
    root: &Path,
    component_id: &str,
    level: Option<BumpLevel>,
    skip_changelog: bool,
) -> ReleaseResult<()> {
    let manifest = load_manifest(root)?;
    let component = manifest
        .components
        .iter()
        .find(|component| component.id == component_id)
        .with_release_context(|| format!("unknown release component {component_id}"))?;

    let current = read_version(root, &component.version_source)?;
    let current = Version::parse(&current).with_release_context(|| {
        format!("{} version is not valid semver: {current}", component.id)
    })?;

    let latest_tag_version = latest_tag(root, &component.tag_prefix)?
        .as_deref()
        .and_then(|tag| tag.strip_prefix(component.tag_prefix.as_str()))
        .and_then(|version| Version::parse(version).ok());

    let next_version = match level {
        Some(level) => cliff::resolve_next(&current, latest_tag_version.as_ref(), level),
        None => cliff::next_version(root, component, &current, latest_tag_version.as_ref())?,
    };
    let next = next_version.to_string();

    for file in &component.version_files {
        if file.kind == VersionKind::ChangelogHeading {
            if skip_changelog {
                let path = root.join(&file.path);
                let content = std::fs::read_to_string(&path)
                    .with_release_context(|| format!("failed to read {}", file.path))?;
                let updated = ensure_changelog_heading(&content, &next)?;
                if updated != content {
                    std::fs::write(&path, updated)
                        .with_release_context(|| format!("failed to write {}", file.path))?;
                }
            } else {
                cliff::prepend_changelog(root, component, &next_version, &file.path)?;
            }
            continue;
        }

        let path = root.join(&file.path);
        let content = std::fs::read_to_string(&path)
            .with_release_context(|| format!("failed to read {}", file.path))?;
        let updated = match file.kind {
            VersionKind::CargoPackage => {
                replace_cargo_package_version(&content, file.package.as_deref(), &next)?
            }
            VersionKind::CargoLockPackage => {
                replace_cargo_lock_package_version(&content, file.package.as_deref(), &next)?
            }
            VersionKind::ReadmeVersionLine => replace_readme_version_line(&content, &next)?,
            VersionKind::ChangelogHeading => unreachable!("handled above"),
            VersionKind::JsonVersion => {
                replace_json_version(&content, file.json_pointer.as_deref(), &next)?
            }
            VersionKind::JsonNoVersion => content.clone(),
            VersionKind::NpmPackageLock => {
                replace_npm_package_lock_version(&content, file.package.as_deref(), &next)?
            }
            VersionKind::GradleVersionName => replace_gradle_version_name(&content, &next)?,
            VersionKind::GradleVersionCode => increment_gradle_version_code(&content)?,
        };
        if updated != content {
            std::fs::write(&path, updated)
                .with_release_context(|| format!("failed to write {}", file.path))?;
        }
    }

    println!("bumped {} to {next}", component.id);
    Ok(())
}
```

Add `use cliff::...` access — `cliff` is a submodule, so call as `cliff::next_version(...)` (no new `use` needed; ensure `mod cliff;` from Task 3 is present).

- [ ] **Step 2: Update `main.rs` args + dispatch**

Change the `BumpVersion` variant:

```rust
    /// Bump all version-bearing files for one component. Level is auto-derived
    /// from conventional commits when omitted.
    BumpVersion {
        component: String,
        #[arg(value_enum)]
        level: Option<checks::release_versions::BumpLevel>,
        /// Skip git-cliff changelog generation (stamp an empty heading instead).
        #[arg(long)]
        skip_changelog: bool,
    },
```

And the dispatch arm:

```rust
        Command::BumpVersion {
            component,
            level,
            skip_changelog,
        } => Ok(checks::release_versions::bump(
            &root,
            &component,
            level,
            skip_changelog,
        )?),
```

- [ ] **Step 3: Verify build + existing tests still pass**

Run: `cargo build -p xtask && cargo test -p xtask release_versions 2>&1 | tail -20`
Expected: builds; existing release_versions tests PASS (no regression). The `bump` arithmetic now lives in `cliff::apply_level`; confirm no duplicate-definition or unused-import warnings.

- [ ] **Step 4: Manual end-to-end smoke (local, git-cliff present)**

Run a dry check on a throwaway branch state (do NOT commit the result):
```bash
cargo run -p xtask -- bump-version cli
git --no-pager diff --stat   # expect Cargo.toml, Cargo.lock, README.md, CHANGELOG.md, apps/web/* updated
git --no-pager diff CHANGELOG.md | head -30   # expect a real "## [x.y.z]" section with grouped commits
git checkout -- .            # revert the smoke test
```
Expected: version files move to the auto-derived version and `CHANGELOG.md` gains a real section (not "Release version bump."). Then revert.

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p xtask
git add xtask/src/checks/release_versions.rs xtask/src/main.rs
git commit -m "feat: auto-derive bump level and generate changelogs in bump-version"
```

---

### Task 7: Add component changelogs + manifest entries (seed + regenerate)

**Goal:** Create the three new changelogs and register them; regenerate the CLI changelog cleanly. All four `version_files` paths must exist for the manifest to validate.

**Files:**
- Create: `apps/palette-tauri/CHANGELOG.md`, `apps/android/CHANGELOG.md`, `apps/chrome-extension/CHANGELOG.md`
- Modify: `CHANGELOG.md` (regenerated)
- Modify: `release/components.toml`

**Interfaces:**
- Consumes: `cliff::generate_full_changelog` (via a temporary one-shot, see Step 1) or direct git-cliff CLI.

- [ ] **Step 1: Generate the four changelogs from history**

Run from repo root (git-cliff present). These reproduce `generate_full_changelog`'s scoping:

```bash
# CLI (regenerate, overwriting the filler entries)
git-cliff --tag-pattern '^v[0-9]' \
  --include-path 'src/**' --include-path 'Cargo.toml' --include-path 'Cargo.lock' \
  --include-path 'build.rs' --include-path 'migrations/**' --include-path 'apps/web/**' \
  --include-path 'rust-toolchain.toml' --include-path 'vendor/**' \
  -o CHANGELOG.md

# palette
git-cliff --tag-pattern '^palette-v' --include-path 'apps/palette-tauri/**' \
  -o apps/palette-tauri/CHANGELOG.md
# android
git-cliff --tag-pattern '^android-v' --include-path 'apps/android/**' \
  -o apps/android/CHANGELOG.md
# chrome
git-cliff --tag-pattern '^chrome-ext-v' \
  --include-path 'apps/chrome-extension/**' --include-path 'assets/**' \
  -o apps/chrome-extension/CHANGELOG.md
```

- [ ] **Step 2: Verify each changelog has its current-version heading**

Run:
```bash
head -5 CHANGELOG.md
head -5 apps/palette-tauri/CHANGELOG.md
head -5 apps/android/CHANGELOG.md
head -5 apps/chrome-extension/CHANGELOG.md
```
Expected: each starts with the `# Changelog` header and a `## [<version>]` heading at (or below) the component's current version. If a component has only one tag and no newer commits, git-cliff still emits that tag's section.

**Fallback if a component's changelog is empty** (no tags yet / all commits skipped): create a minimal stub so the manifest validates and parity passes at the current version, e.g. for android at its current `versionName`:
```bash
printf '# Changelog\n\n## [<CURRENT_VERSION>] - <YYYY-MM-DD>\n\n### Changed\n- Initial changelog.\n' > apps/android/CHANGELOG.md
```
(Replace `<CURRENT_VERSION>` with the value from the component's `version_source`.)

- [ ] **Step 3: Register the new changelogs in `release/components.toml`**

Add a `changelog_heading` entry to each of `palette`, `android`, `chrome` `version_files` arrays (cli already has one). For `palette`:

```toml
version_files = [
  { kind = "json_version", path = "apps/palette-tauri/src-tauri/tauri.conf.json", json_pointer = "/version" },
  { kind = "json_version", path = "apps/palette-tauri/package.json", json_pointer = "/version" },
  { kind = "cargo_package", path = "apps/palette-tauri/src-tauri/Cargo.toml", package = "axon-palette-tauri" },
  { kind = "cargo_lock_package", path = "apps/palette-tauri/src-tauri/Cargo.lock", package = "axon-palette-tauri" },
  { kind = "changelog_heading", path = "apps/palette-tauri/CHANGELOG.md" },
]
```

For `android`, append to its `version_files`:
```toml
  { kind = "changelog_heading", path = "apps/android/CHANGELOG.md" },
```

For `chrome`, append to its `version_files`:
```toml
  { kind = "changelog_heading", path = "apps/chrome-extension/CHANGELOG.md" },
```

- [ ] **Step 4: Validate the manifest + parity**

Run:
```bash
cargo run -p xtask -- check-release-versions --mode pr 2>&1 | tail -30
cargo test -p xtask reads_component_manifest 2>&1 | tail -5
```
Expected: `check-release-versions` passes (each `changelog_heading` parity check now finds its file with the current-version heading); `reads_component_manifest` still passes (it asserts only id/tag_prefix/workflow/shipping_paths, which are unchanged).

If parity fails because a generated heading version differs from the component's `version_source` (e.g. the latest tag is behind the working version), prepend a heading at the current `version_source` value to that changelog so parity holds.

- [ ] **Step 5: Commit (all together — manifest validation requires the files to exist)**

```bash
git add release/components.toml CHANGELOG.md apps/palette-tauri/CHANGELOG.md apps/android/CHANGELOG.md apps/chrome-extension/CHANGELOG.md
git commit -m "feat: add per-component changelogs and register them in release manifest"
```

---

### Task 8: PR-gate suggested-level hint (best-effort, CI-safe)

**Goal:** When a component changed but wasn't bumped, `check-release-versions` additionally prints git-cliff's suggested level — only when git-cliff is available.

**Files:**
- Modify: `xtask/src/checks/release_versions.rs` (`collect_changed_component_errors`)
- Modify: `xtask/src/checks/release_versions_tests.rs` (append a unit test for the hint formatting)

**Interfaces:**
- Consumes: `cliff::suggested_level` (Task 4).

- [ ] **Step 1: Add a pure hint-formatting helper + test**

In `release_versions.rs`, add:

```rust
fn suggested_level_hint(level: Option<BumpLevel>) -> String {
    match level {
        Some(BumpLevel::Major) => " (suggested bump: major)".to_owned(),
        Some(BumpLevel::Minor) => " (suggested bump: minor)".to_owned(),
        Some(BumpLevel::Patch) => " (suggested bump: patch)".to_owned(),
        None => String::new(),
    }
}
```

Append to `release_versions_tests.rs`:

```rust
#[test]
fn suggested_level_hint_renders_or_is_empty() {
    assert_eq!(
        suggested_level_hint(Some(BumpLevel::Minor)),
        " (suggested bump: minor)"
    );
    assert_eq!(suggested_level_hint(None), "");
}
```

- [ ] **Step 2: Run to verify the test passes (helper compiles)**

Run: `cargo test -p xtask suggested_level_hint 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 3: Wire the hint into the changed-but-unbumped errors**

In `collect_changed_component_errors`, compute the hint once (best-effort) and append it to the two relevant error pushes:

```rust
    let hint = suggested_level_hint(cliff::suggested_level(root, component, &candidate));

    let latest = latest_version_from_plan(component, plan)?;
    if let Some(latest) = latest
        && candidate <= latest
    {
        errors.push(format!(
            "{} code changed but version {} is not greater than latest {} tag version {}. \
             Bump {} before merging.{hint}",
            component.id, plan.version, component.tag_prefix, latest, bump_hint(component)
        ));
    }

    if tag_exists(root, &plan.candidate_tag)? {
        errors.push(format!(
            "{} code changed but tag {} already exists. Bump {} before merging.{hint}",
            component.id, plan.candidate_tag, bump_hint(component)
        ));
    }
```

(`candidate` is the `Version` already parsed at the top of `collect_changed_component_errors`.)

- [ ] **Step 4: Verify build + CI-safety**

Run:
```bash
cargo build -p xtask
cargo run -p xtask -- check-release-versions --mode pr 2>&1 | tail -5
```
Expected: builds and the gate passes (no changed-but-unbumped components). CI-safety is **structural**, not environmental: `cliff::suggested_level` returns `Option<BumpLevel>` and converts every git-cliff failure to `None` via `.ok()?`, so it can never propagate an error or change the gate's pass/fail. Confirm by inspection that `suggested_level`'s body has no `?` on a `ReleaseResult` that escapes the function and that `run_git_cliff(...)` is consumed with `.ok()?`. To exercise the missing-binary path locally without breaking the cargo toolchain, build first, then run the built binary with git-cliff hidden:
```bash
mkdir -p /tmp/nocliff && ln -sf "$(command -v git)" /tmp/nocliff/git
PATH="/tmp/nocliff:$(rustc --print sysroot)/bin" ./target/debug/xtask check-release-versions --mode pr 2>&1 | tail -5
```
Expected: runs without panic; gate result unchanged (hint silently absent because git-cliff is not on this PATH).

- [ ] **Step 5: Commit**

```bash
cargo fmt && cargo clippy -p xtask
git add xtask/src/checks/release_versions.rs xtask/src/checks/release_versions_tests.rs
git commit -m "feat: print git-cliff suggested bump level in release gate (best-effort)"
```

---

### Task 9: Documentation + final verification

**Goal:** Update `CLAUDE.md`'s Release Pipeline section and confirm the whole gate is green.

**Files:**
- Modify: `CLAUDE.md` (Release Pipeline / Version bumping rules sections)

- [ ] **Step 1: Update `CLAUDE.md`**

In the "Version bumping rules" area, document the new behavior. Add (adapt wording to fit the existing section):

```markdown
**Bump level is auto-derived.** `cargo xtask bump-version <component>` now derives the
bump level from conventional commits since the component's last tag (via git-cliff,
configured in `cliff.toml`): `feat!`/`BREAKING CHANGE` → major, `feat` → minor, everything
else → patch. Pass an explicit level to override: `cargo xtask bump-version <component> minor`.

**Changelogs are generated, not stamped.** Each component has its own `CHANGELOG.md`
(`CHANGELOG.md`, `apps/palette-tauri/CHANGELOG.md`, `apps/android/CHANGELOG.md`,
`apps/chrome-extension/CHANGELOG.md`), written by git-cliff scoped to that component's
shipping paths + tag prefix. `git-cliff` must be installed where you run `bump-version`
(`mise use -g git-cliff`); CI's `check-release-versions` does not require it. Use
`--skip-changelog` to fall back to an empty heading in an emergency.

**`CHANGELOG.md` edits never trigger a release** — change detection ignores them, so
documenting a release cannot recursively cut another.
```

- [ ] **Step 2: Run the full pre-PR gate**

Run:
```bash
just verify 2>&1 | tail -40
```
Expected: `fmt --check`, `clippy`, `check`, and `test` all pass. If `just` is unavailable, run `cargo fmt --check && cargo clippy && cargo check && cargo test` (the git-cliff integration test self-skips if git-cliff is absent, but it is present locally).

- [ ] **Step 3: Confirm no unintended version bump / release trigger**

Run:
```bash
cargo run -p xtask -- check-release-versions --mode pr 2>&1 | tail -20
```
Expected: passes with no component flagged as "changed but not bumped" — the only shipping-path additions are `CHANGELOG.md` files, which change detection now ignores.

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: document git-cliff changelogs and auto-bump in release pipeline"
```

- [ ] **Step 5: Close the bead**

```bash
bd close axon_rust-qbvn
```

---

## Self-Review

**Spec coverage:**
- All four components get changelogs → Task 7 (+ `cliff.toml` template Task 2).
- Auto-derive bump, optional override → Task 6 (logic Task 3/4).
- git-cliff for both version + changelog → Task 4 (`next_version`, `prepend_changelog`).
- Per-component scoping (include-path/tag-pattern) → Task 3 (`build_include_paths`, `build_tag_pattern`).
- Custom-prefix robustness + spike → Task 1 + delta-based `derive_level`/`next_version_from_outputs` (Task 3).
- Changelog self-trigger fix → Task 5.
- Regenerate CLI changelog from history → Task 7 Step 1.
- CI-safe gate hint → Task 8.
- git-cliff-missing fails bump (not silent) + `--skip-changelog` escape hatch → Task 4 (`run_git_cliff` error) + Task 6.
- CI must not require git-cliff → integration test PATH-gated (Task 4), hint best-effort (Task 8).
- Docs → Task 9.

**Placeholder scan:** Generated changelog *content* is produced by git-cliff at build time (Task 7), not hand-authored; the one stub fallback uses explicit `<CURRENT_VERSION>`/`<YYYY-MM-DD>` markers the implementer fills from `version_source` — these are runtime values, not plan placeholders. No `TBD`/`TODO`/"handle edge cases" remain.

**Type consistency:** `BumpLevel`, `Component`, `Version`, `ReleaseResult` used consistently. `apply_level` (Task 3) is the single bump-arithmetic function reused by `bump()` (Task 6) and `next_version_from_outputs` (Task 3). `next_version` (IO, Task 4) wraps `next_version_from_outputs` (pure, Task 3). `suggested_level` (Task 4) feeds `suggested_level_hint` (Task 8). Names match across tasks.

## Execution Handoff

(Provided after the plan is approved.)
