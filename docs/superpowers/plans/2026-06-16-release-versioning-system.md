# Release Versioning System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Axon's multi-component version bump enforcement from a post-merge `auto-tag` failure into a shared, pre-merge release-version system that detects changed shipping components, validates their version bumps, keeps each component's version-bearing files in sync, and feeds the existing auto-tag release flow from one source of truth.

**Architecture:** Add a versioned release manifest plus an `xtask` release-version checker. The checker owns component definitions, path diffing, version extraction, version parity, tag lookup, monotonic bump validation, and JSON output for GitHub Actions. CI runs the checker on pull requests before merge. `auto-tag.yml` uses the same checker on `main` before creating tags and dispatching release workflows.

**Tech Stack:** Rust 2024 `xtask`, `clap`, `anyhow`, `regex`, `serde`, `serde_json`, `toml`, `semver`, Git CLI, GitHub Actions, existing Axon workflows.

**Implementation Status:** Completed in PR #233 (`codex/release-versioning-system`). The checklist below is retained as the original execution record.

## Global Constraints

- Preserve per-component releases: CLI, palette, Android, and Chrome extension continue to tag and publish independently.
- A docs-only, workflow-only, `xtask`-only, or test-only change must not require a release bump unless it touches a component's configured shipping paths.
- A PR that changes a component's shipping paths must fail before merge unless that component has a strictly higher version than its latest existing tag.
- CLI parity still requires `Cargo.toml`, `README.md`, `CHANGELOG.md`, `apps/web/package.json`, and `apps/web/openapi/axon.json` to carry the same version. `plugins/axon/.claude-plugin/plugin.json` must still carry no `version`.
- Android must bump both `versionName` and `versionCode` when `apps/android` shipping paths changed.
- Chrome extension is included in the same gate; changes under `apps/chrome-extension` or shared `assets` require a `chrome-ext-v*` version bump.
- Keep `.github/workflows/codeql.yml` untouched unless the worker owns that untracked file in their checkout.
- Do not rewrite unrelated release workflows. Keep the existing tag prefixes and release workflow names.
- Use `apply_patch` for manual edits and do not revert unrelated worktree changes.

---

## Task 1: Add a Component Release Manifest

- [ ] Create `release/components.toml` with this exact content:

```toml
schema_version = 1

[[components]]
id = "cli"
name = "Axon CLI"
tag_prefix = "v"
release_workflow = "release.yml"
shipping_paths = [
  "src",
  "Cargo.toml",
  "Cargo.lock",
  "build.rs",
  "migrations",
  "apps/web",
  "rust-toolchain.toml",
  "vendor",
]
version_source = { kind = "cargo_package", path = "Cargo.toml", package = "axon" }
version_files = [
  { kind = "cargo_package", path = "Cargo.toml", package = "axon" },
  { kind = "readme_version_line", path = "README.md" },
  { kind = "changelog_heading", path = "CHANGELOG.md" },
  { kind = "json_version", path = "apps/web/package.json" },
  { kind = "json_version", path = "apps/web/openapi/axon.json" },
  { kind = "json_no_version", path = "plugins/axon/.claude-plugin/plugin.json" },
]

[[components]]
id = "palette"
name = "Desktop Palette"
tag_prefix = "palette-v"
release_workflow = "palette-release.yml"
shipping_paths = ["apps/palette-tauri"]
version_source = { kind = "json_version", path = "apps/palette-tauri/src-tauri/tauri.conf.json" }
version_files = [
  { kind = "json_version", path = "apps/palette-tauri/src-tauri/tauri.conf.json" },
  { kind = "json_version", path = "apps/palette-tauri/package.json" },
  { kind = "cargo_package", path = "apps/palette-tauri/src-tauri/Cargo.toml", package = "axon-palette-tauri" },
]

[[components]]
id = "android"
name = "Android APK"
tag_prefix = "android-v"
release_workflow = "android-release.yml"
shipping_paths = ["apps/android"]
version_source = { kind = "gradle_version_name", path = "apps/android/app/build.gradle.kts" }
version_files = [
  { kind = "gradle_version_name", path = "apps/android/app/build.gradle.kts" },
  { kind = "gradle_version_code", path = "apps/android/app/build.gradle.kts" },
]

[[components]]
id = "chrome"
name = "Chrome Extension"
tag_prefix = "chrome-ext-v"
release_workflow = "chrome-extension-release.yml"
shipping_paths = ["apps/chrome-extension", "assets"]
version_source = { kind = "json_version", path = "apps/chrome-extension/manifest.json" }
version_files = [
  { kind = "json_version", path = "apps/chrome-extension/manifest.json" },
]
```

- [ ] Add `release/components.toml` to every CI sparse checkout block that needs `cargo xtask check`, `cargo xtask check-version-sync`, or the new release checker.

- [ ] Verification:

```bash
test -f release/components.toml
rg -n 'id = "(cli|palette|android|chrome)"|chrome-ext-v|android-v|palette-v' release/components.toml
```

## Task 2: Add Dependencies for Manifest Parsing and Semver Output

- [ ] Update `xtask/Cargo.toml` dependencies:

```toml
[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
regex = "1"
semver = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.9"
walkdir = "2"
```

- [ ] Verification:

```bash
cargo test -p xtask --locked --no-run
```

## Task 3: Implement the Shared Release-Version Checker

- [ ] Add `xtask/src/checks/release_versions.rs`.

- [ ] Implement these public types and entry points:

```rust
use anyhow::{Context, Result, bail};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateMode {
    Pr,
    Main,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentPlan {
    pub id: String,
    pub name: String,
    pub changed: bool,
    pub version: String,
    pub candidate_tag: String,
    pub last_tag: Option<String>,
    pub release_workflow: String,
    pub shipping_paths: Vec<String>,
}

pub fn check(root: &Path, base: Option<&str>, head: &str, mode: GateMode, json: bool) -> Result<()>;
pub fn plan(root: &Path, base: Option<&str>, head: &str) -> Result<Vec<ComponentPlan>>;
pub fn bump(root: &Path, component_id: &str, level: BumpLevel) -> Result<()>;
```

- [ ] Implement manifest structs matching `release/components.toml`:

```rust
#[derive(Debug, Deserialize)]
struct Manifest {
    schema_version: u32,
    components: Vec<Component>,
}

#[derive(Debug, Deserialize)]
struct Component {
    id: String,
    name: String,
    tag_prefix: String,
    release_workflow: String,
    shipping_paths: Vec<String>,
    version_source: VersionFile,
    version_files: Vec<VersionFile>,
}

#[derive(Debug, Deserialize, Clone)]
struct VersionFile {
    kind: VersionKind,
    path: String,
    package: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum VersionKind {
    CargoPackage,
    ReadmeVersionLine,
    ChangelogHeading,
    JsonVersion,
    JsonNoVersion,
    GradleVersionName,
    GradleVersionCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BumpLevel {
    Patch,
    Minor,
    Major,
}
```

- [ ] Implement version readers with component-specific, friendly errors:

```rust
fn read_version(root: &Path, file: &VersionFile) -> Result<String> {
    let path = root.join(&file.path);
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", file.path))?;
    match file.kind {
        VersionKind::CargoPackage => read_cargo_package_version(&content, file.package.as_deref())
            .with_context(|| format!("failed to read Cargo package version from {}", file.path)),
        VersionKind::JsonVersion => read_json_version(&content)
            .with_context(|| format!("failed to read JSON version from {}", file.path)),
        VersionKind::GradleVersionName => read_gradle_version_name(&content)
            .with_context(|| format!("failed to read versionName from {}", file.path)),
        VersionKind::ReadmeVersionLine
        | VersionKind::ChangelogHeading
        | VersionKind::JsonNoVersion
        | VersionKind::GradleVersionCode => {
            bail!("{:?} is not a canonical version source", file.kind)
        }
    }
}
```

- [ ] Implement parity checks:

```rust
fn check_component_parity(root: &Path, component: &Component, expected: &str) -> Result<Vec<String>> {
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
            VersionKind::CargoPackage => check_cargo_package_version(&content, file.package.as_deref(), expected),
            VersionKind::ReadmeVersionLine => check_readme_version_line(&content, expected),
            VersionKind::ChangelogHeading => check_changelog_heading(&content, expected),
            VersionKind::JsonVersion => check_json_version(&content, expected),
            VersionKind::JsonNoVersion => check_json_no_version(&content),
            VersionKind::GradleVersionName => check_gradle_version_name(&content, expected),
            VersionKind::GradleVersionCode => check_gradle_version_code_present(&content),
        };
        if let Err(error) = result {
            errors.push(format!("{}: {error}", file.path));
        }
    }
    Ok(errors)
}
```

- [ ] Implement Git operations using `git -C <root>`:

```rust
fn latest_tag(root: &Path, prefix: &str) -> Result<Option<String>>;
fn tag_exists(root: &Path, tag: &str) -> Result<bool>;
fn changed_since_ref(root: &Path, base: &str, head: &str, paths: &[String]) -> Result<bool>;
fn merge_base_origin_main(root: &Path) -> Result<String>;
```

- [ ] `latest_tag` must sort semver by stripping the prefix and parsing with `semver::Version`; do not rely on lexical order.

- [ ] `check` behavior:
  - Build a `ComponentPlan` for each manifest component.
  - For each component, compute `candidate_tag = format!("{}{}", tag_prefix, version)`.
  - In PR mode, compare `base.unwrap_or("origin/main")..head` for changed paths and fail if changed and candidate tag already exists.
  - In main mode, compare `latest_tag..head` for changed paths and fail if changed and candidate tag exists.
  - In both modes, fail if changed and `version <= latest_version`.
  - Always run parity checks for every component, even unchanged components, so drift is caught early.
  - When `json == true`, write a stable JSON array of `ComponentPlan` to stdout.
  - When `json == false`, print a human-readable summary with `changed`, `version`, `candidate_tag`, `last_tag`, and release workflow.

- [ ] Error message shape for missing bump:

```text
android code changed but tag android-v1.3.2 already exists. Bump apps/android/app/build.gradle.kts versionName and versionCode before merging.
```

- [ ] Error message shape for Chrome inclusion:

```text
chrome code changed but tag chrome-ext-v0.2.0 already exists. Bump apps/chrome-extension/manifest.json before merging.
```

- [ ] Verification:

```bash
cargo test -p xtask release_versions --locked
cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
cargo xtask release-plan --base origin/main --head HEAD --json
```

## Task 4: Add Tests for the Release-Version Checker

- [ ] Add `xtask/src/checks/release_versions_tests.rs`.

- [ ] Cover these unit tests:

```rust
#[test]
fn reads_component_manifest();

#[test]
fn cargo_package_version_reader_ignores_workspace_version();

#[test]
fn json_version_reader_handles_pretty_and_compact_json();

#[test]
fn gradle_version_reader_extracts_version_name_and_code();

#[test]
fn cli_parity_requires_changelog_and_web_versions();

#[test]
fn plugin_json_version_is_rejected();

#[test]
fn android_parity_requires_version_code();

#[test]
fn semver_tag_sorting_keeps_component_prefixes_separate();

#[test]
fn changed_shipping_path_requires_new_tag();

#[test]
fn docs_only_change_does_not_require_component_bump();

#[test]
fn chrome_assets_change_requires_chrome_bump();
```

- [ ] Include test fixtures in temporary directories. Use real `git init`, `git tag`, and commits for path-diff behavior so this does not become a string-only test.

- [ ] The test helper should configure Git identity:

```rust
fn git(root: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {:?} failed", args);
}
```

- [ ] Wire the tests from the bottom of `release_versions.rs`:

```rust
#[cfg(test)]
#[path = "release_versions_tests.rs"]
mod tests;
```

- [ ] Verification:

```bash
cargo test -p xtask release_versions --locked
```

## Task 5: Wire New `xtask` Commands

- [ ] Update `xtask/src/checks.rs`:

```rust
pub mod release_versions;

pub fn check(root: &Path) -> Result<()> {
    no_mod_rs::check(root)?;
    mcp_http::check(root)?;
    env_staged::check(root)?;
    unwraps::check(root)?;
    claude_symlinks::check(root)?;
    broken_symlinks::check(root)?;
    secrets::check(root)?;
    version_sync::check(root)?;
    release_versions::check(root, None, "HEAD", release_versions::GateMode::Pr, false)?;
    println!("All checks passed.");
    Ok(())
}
```

- [ ] Update `xtask/src/main.rs` command enum:

```rust
/// Verify all releasable components have valid versions and changed shipping paths have bumps.
CheckReleaseVersions {
    #[arg(long)]
    base: Option<String>,
    #[arg(long, default_value = "HEAD")]
    head: String,
    #[arg(long, value_parser = ["pr", "main"], default_value = "pr")]
    mode: String,
    #[arg(long)]
    json: bool,
},
/// Print the release plan consumed by GitHub Actions.
ReleasePlan {
    #[arg(long)]
    base: Option<String>,
    #[arg(long, default_value = "HEAD")]
    head: String,
    #[arg(long)]
    json: bool,
},
/// Bump all version-bearing files for one component.
BumpVersion {
    component: String,
    #[arg(value_parser = ["patch", "minor", "major"])]
    level: String,
},
```

- [ ] Add match arms:

```rust
Command::CheckReleaseVersions { base, head, mode, json } => {
    let mode = match mode.as_str() {
        "pr" => checks::release_versions::GateMode::Pr,
        "main" => checks::release_versions::GateMode::Main,
        _ => unreachable!(),
    };
    checks::release_versions::check(&root, base.as_deref(), &head, mode, json)
}
Command::ReleasePlan { base, head, json } => {
    let plans = checks::release_versions::plan(&root, base.as_deref(), &head)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&plans)?);
    } else {
        for plan in plans {
            println!(
                "{} changed={} version={} tag={} workflow={}",
                plan.id, plan.changed, plan.version, plan.candidate_tag, plan.release_workflow
            );
        }
    }
    Ok(())
}
Command::BumpVersion { component, level } => {
    let level = match level.as_str() {
        "patch" => checks::release_versions::BumpLevel::Patch,
        "minor" => checks::release_versions::BumpLevel::Minor,
        "major" => checks::release_versions::BumpLevel::Major,
        _ => unreachable!(),
    };
    checks::release_versions::bump(&root, &component, level)
}
```

- [ ] Verification:

```bash
cargo xtask --help
cargo xtask check-release-versions --help
cargo xtask release-plan --help
cargo test -p xtask --locked
```

## Task 6: Keep `check-version-sync` as the CLI-Focused Compatibility Command

- [ ] Leave `cargo xtask check-version-sync` working for existing hooks and muscle memory.

- [ ] Refactor `xtask/src/checks/version_sync.rs` only enough to avoid duplicate file lists:
  - Either call `release_versions::check_cli_parity_only(root)`.
  - Or keep current implementation and add a comment saying the full multi-component gate is `check-release-versions`.

- [ ] Update the command help text in `xtask/src/main.rs`:

```rust
/// Compatibility check for the CLI component's version-bearing files.
/// The full multi-component gate is `check-release-versions`.
CheckVersionSync,
```

- [ ] Verification:

```bash
cargo xtask check-version-sync
cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
```

## Task 7: Add CI Pre-Merge Release Gate

- [ ] Update `.github/workflows/ci.yml`.

- [ ] In the `version-sync` job checkout, include `release`:

```yaml
sparse-checkout: |
  src
  xtask
  benches
  apps/web
  apps/palette-tauri
  apps/android
  apps/chrome-extension
  assets
  plugins
  docs
  .github
  tests
  scripts
  config
  release
  vendor
  .cargo
```

- [ ] Add this step after `check-version-sync`:

```yaml
- name: Verify release versions for changed shipping components
  run: cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
```

- [ ] Ensure the checkout has full history and tags for this job:

```yaml
with:
  fetch-depth: 0
```

- [ ] Verification:

```bash
rg -n 'check-release-versions|fetch-depth: 0|release$|apps/chrome-extension|assets' .github/workflows/ci.yml
cargo test --locked --test workflow_shapes
```

## Task 8: Refactor Auto-Tag to Consume the Shared Release Plan

- [ ] Update `.github/workflows/auto-tag.yml`.

- [ ] Replace the hard-coded matrix's component metadata with a plan generated by `xtask`:

```yaml
jobs:
  plan:
    runs-on: ubuntu-latest
    outputs:
      matrix: ${{ steps.plan.outputs.matrix }}
    steps:
      - uses: actions/checkout@v5
        with:
          fetch-depth: 0
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          shared-key: xtask
      - name: Build release plan
        id: plan
        run: |
          set -euo pipefail
          cargo xtask check-release-versions --head HEAD --mode main --json > release-plan.json
          matrix=$(jq -c '{include: [.[] | select(.changed == true)]}' release-plan.json)
          echo "matrix=$matrix" >> "$GITHUB_OUTPUT"
          cat release-plan.json

  release:
    needs: plan
    if: ${{ needs.plan.outputs.matrix != '{"include":[]}' }}
    strategy:
      fail-fast: false
      matrix: ${{ fromJson(needs.plan.outputs.matrix) }}
```

- [ ] In release job steps, use matrix fields from `ComponentPlan`:

```yaml
- name: Wait for CI to pass on this commit
  env:
    GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  run: |
    set -euo pipefail
    sha="${{ github.sha }}"
    deadline=$(( $(date +%s) + 1800 ))
    while :; do
      conclusion=$(gh run list --workflow=ci.yml --commit "$sha" --json conclusion --jq '.[0].conclusion' 2>/dev/null || true)
      status=$(gh run list --workflow=ci.yml --commit "$sha" --json status --jq '.[0].status' 2>/dev/null || true)
      if [ "$conclusion" = "success" ]; then break; fi
      if [ "$conclusion" = "failure" ] || [ "$conclusion" = "cancelled" ] || [ "$conclusion" = "timed_out" ]; then
        echo "::error::CI concluded '$conclusion' for $sha — refusing to cut a release." >&2
        exit 1
      fi
      if [ "$(date +%s)" -ge "$deadline" ]; then
        echo "::error::timed out waiting for CI to finish for $sha." >&2
        exit 1
      fi
      echo "CI status='${status:-unknown}' conclusion='${conclusion:-pending}'; sleeping 20s ..."
      sleep 20
    done

- name: Create and push tag
  run: |
    git config user.name "github-actions[bot]"
    git config user.email "github-actions[bot]@users.noreply.github.com"
    git tag "${{ matrix.candidate_tag }}"
    git push origin "${{ matrix.candidate_tag }}"

- name: Dispatch release workflow
  env:
    GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  run: gh workflow run "${{ matrix.release_workflow }}" --ref "${{ matrix.candidate_tag }}" -f publish=true
```

- [ ] Keep the workflow permissions and concurrency unchanged.

- [ ] Update the header comments to say component metadata lives in `release/components.toml`.

- [ ] Verification:

```bash
rg -n 'release/components.toml|release-plan|candidate_tag|release_workflow|check-release-versions' .github/workflows/auto-tag.yml
cargo test --locked --test workflow_shapes
```

## Task 9: Add Workflow Shape Tests for the New Contract

- [ ] Update `tests/workflow_shapes.rs`.

- [ ] Add a test that asserts CI runs the new gate:

```rust
#[test]
fn ci_runs_release_version_gate_before_merge() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    assert!(
        workflow.contains("cargo xtask check-release-versions --base origin/main --head HEAD --mode pr"),
        "CI must run the multi-component release version gate on pull requests"
    );
    assert!(
        workflow.contains("fetch-depth: 0"),
        "release version gate needs tags and history"
    );
}
```

- [ ] Add a test that asserts auto-tag uses the shared plan:

```rust
#[test]
fn auto_tag_uses_xtask_release_plan() {
    let workflow = include_str!("../.github/workflows/auto-tag.yml");
    assert!(
        workflow.contains("cargo xtask check-release-versions --head HEAD --mode main --json"),
        "auto-tag must use the shared xtask release-version detector"
    );
    assert!(
        workflow.contains("matrix.candidate_tag") && workflow.contains("matrix.release_workflow"),
        "auto-tag must consume tags and workflows from the xtask release plan"
    );
}
```

- [ ] Verification:

```bash
cargo test --locked --test workflow_shapes
```

## Task 10: Implement the Convenience Version Bumper

- [ ] In `release_versions.rs`, implement `bump(root, component_id, level)`:
  - Load the manifest.
  - Find the named component.
  - Read the component's canonical version.
  - Compute the next semver:
    - patch: `X.Y.Z+1`
    - minor: `X.Y+1.0`
    - major: `X+1.0.0`
  - Rewrite every component version file that carries that version.
  - For Android, increment `versionCode` by `1` whenever `versionName` changes.
  - For CLI, insert a `CHANGELOG.md` heading if the next version heading is missing.

- [ ] Add replacement helpers:

```rust
fn replace_json_version(content: &str, next: &str) -> Result<String>;
fn replace_cargo_package_version(content: &str, package: Option<&str>, next: &str) -> Result<String>;
fn replace_gradle_version_name(content: &str, next: &str) -> Result<String>;
fn increment_gradle_version_code(content: &str) -> Result<String>;
fn replace_readme_version_line(content: &str, next: &str) -> Result<String>;
fn ensure_changelog_heading(content: &str, next: &str) -> String;
```

- [ ] The `ensure_changelog_heading` insertion should add this block after the top-level changelog heading and before the previous release:

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Changed
- Release version bump.
```

- [ ] Use `chrono` only if the repo already has it in `xtask`; otherwise use `std::process::Command::new("date").arg("+%F")` to avoid adding a date dependency solely for the bumper.

- [ ] Verification:

```bash
tmp=$(mktemp -d)
git archive HEAD | tar -x -C "$tmp"
(cd "$tmp" && cargo xtask bump-version chrome patch && git diff -- apps/chrome-extension/manifest.json)
(cd "$tmp" && cargo xtask bump-version android patch && git diff -- apps/android/app/build.gradle.kts)
```

## Task 11: Update Documentation and Agent Memory

- [ ] Update `CLAUDE.md` release section:
  - State that `release/components.toml` is the source of truth for component shipping paths, tag prefixes, release workflows, and version files.
  - State that `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr` is the pre-merge gate.
  - State that `cargo xtask bump-version <component> patch|minor|major` is the preferred way to bump versions.
  - Keep the existing bump rules:

```markdown
feat!: / BREAKING CHANGE -> major
feat / feat(...) -> minor
everything else -> patch
```

- [ ] Update `docs/contributing/repo/rules.md` so it no longer says `.claude-plugin/plugin.json` should carry a version. It must say the plugin manifest has no `version` key.

- [ ] Add a short release checklist:

```markdown
1. Identify changed components with `cargo xtask release-plan --base origin/main --head HEAD`.
2. Bump only those components with `cargo xtask bump-version <component> patch|minor|major`.
3. Run `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`.
4. Run `cargo xtask check`.
```

- [ ] Verification:

```bash
rg -n 'release/components.toml|check-release-versions|bump-version|plugin.json.*no `version`|chrome-ext-v' CLAUDE.md docs/contributing/repo/rules.md
```

## Task 12: Final Verification

- [ ] Run the full focused verification:

```bash
cargo fmt --check
cargo test -p xtask --locked
cargo test --locked --test workflow_shapes
cargo xtask check-version-sync
cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
cargo xtask release-plan --base origin/main --head HEAD --json
```

- [ ] Run the repo-level check:

```bash
cargo xtask check
```

- [ ] Inspect release-plan JSON and confirm it contains all four components:

```bash
cargo xtask release-plan --base origin/main --head HEAD --json \
  | jq -r '.[].id' \
  | sort
```

Expected output:

```text
android
chrome
cli
palette
```

- [ ] Confirm Chrome extension is included in auto-tag and the PR gate:

```bash
cargo xtask release-plan --base origin/main --head HEAD --json \
  | jq -e '.[] | select(.id == "chrome" and .candidate_tag | startswith("chrome-ext-v"))'
```

## Task 13: Commit

- [ ] Review the diff:

```bash
git diff -- release/components.toml xtask/Cargo.toml xtask/src/checks.rs xtask/src/main.rs xtask/src/checks/release_versions.rs xtask/src/checks/release_versions_tests.rs xtask/src/checks/version_sync.rs .github/workflows/ci.yml .github/workflows/auto-tag.yml tests/workflow_shapes.rs CLAUDE.md docs/contributing/repo/rules.md
```

- [ ] Stage only owned files:

```bash
git add release/components.toml xtask/Cargo.toml xtask/src/checks.rs xtask/src/main.rs xtask/src/checks/release_versions.rs xtask/src/checks/release_versions_tests.rs xtask/src/checks/version_sync.rs .github/workflows/ci.yml .github/workflows/auto-tag.yml tests/workflow_shapes.rs CLAUDE.md docs/contributing/repo/rules.md docs/superpowers/plans/2026-06-16-release-versioning-system.md
```

- [ ] Commit:

```bash
git commit -m "chore(release): add shared component version gate"
```

- [ ] Do not stage unrelated `.github/workflows/codeql.yml` unless the worker confirms they own it.

## Rollback Plan

- If the PR gate is too strict, remove the new `check-release-versions` CI step first; leave `check-version-sync` intact.
- If `auto-tag.yml` fails after merge, temporarily restore the previous hard-coded matrix from Git history while keeping `release/components.toml` and `xtask` tests for a follow-up fix.
- If the bumper rewrites formatting poorly, keep the checker and remove only the `bump-version` command from the PR.

## Success Criteria

- A PR changing `apps/android` without bumping `versionName` and `versionCode` fails CI before merge.
- A PR changing `apps/chrome-extension` or `assets` without bumping `manifest.json` fails CI before merge.
- A docs-only PR does not require any component version bump.
- The post-merge `auto-tag` workflow no longer duplicates component metadata; it consumes `cargo xtask check-release-versions --mode main --json`.
- The latest component versions remain independently tagged as `v*`, `palette-v*`, `android-v*`, and `chrome-ext-v*`.
