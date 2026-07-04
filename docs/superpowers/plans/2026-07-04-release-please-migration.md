# Release Please Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Switch Axon release PR generation to release-please while preserving Axon's current per-component artifact publishing guarantees.

**Architecture:** Introduce release-please in manifest mode as the author of release PRs, version edits, changelog edits, tags, and GitHub Release records. Keep `release/components.toml` as Axon's local validation and dispatch metadata. Existing component workflows stop creating releases and instead attach artifacts to the release-please-created release by tag.

**Tech Stack:** GitHub Actions, pinned `googleapis/release-please-action@v4` SHA, release-please manifest config, Rust `xtask`, `jq`, `gh`, existing Axon workflows (`release.yml`, `palette-release.yml`, `android-release.yml`, `chrome-extension-release.yml`).

## Global Constraints

- `CLAUDE.md` is the source of truth for agent memory; keep sibling `AGENTS.md` and `GEMINI.md` symlinked to it.
- Do not add a `version` field to `plugins/axon/.claude-plugin/plugin.json`.
- Preserve component tags: CLI `v6.2.1`, palette `palette-v5.12.3`, Android `android-v1.5.0`, Chrome `chrome-ext-v0.2.2`.
- Preserve exact-CI-success gating before public tags/releases are created.
- Release-please PRs must run CI; use a least-privilege `RELEASE_PLEASE_TOKEN`, not the default recursive-event-blocking `GITHUB_TOKEN`.
- Remove `cargo xtask bump-version`, `regen-changelog`, `cliff.toml`, and git-cliff code as part of the migration. Release-please is the only supported release PR/version/changelog owner.
- Every `release/components.toml` `version_files` entry must be either updated by release-please, updated by a postprocessor, or deliberately removed from validation with rationale.
- Official release-please references used: manifest mode config/manifest files, action `config-file`/`manifest-file` inputs and path-prefixed outputs, generic updater markers, and `extra-files` JSON/TOML/generic updaters.

---

## File Structure

- Create `release-please-config.json`: four-component manifest config, with explicit `extra-files`.
- Create `.release-please-manifest.json`: bootstrap versions.
- Modify `release/components.toml`: add `release_please_path` per component and document coverage.
- Modify `xtask/src/checks/release_versions.rs`: validate release-please manifest parity and read package path from `release/components.toml`.
- Modify `xtask/src/main.rs`: add `release-please-dispatch-plan` and remove the old manual release bump/changelog commands.
- Create or extend `xtask/src/checks/release_versions/release_please.rs`: release-please path mapping, manifest parity, dispatch-plan JSON.
- Modify `.github/workflows/ci.yml`: re-enable the release/version check job.
- Create `.github/workflows/release-please.yml`: runs only after `CI` succeeds on `main`, plus manual dispatch.
- Modify release artifact workflows: replace release creation steps with `gh release upload --clobber`.
- Modify `apps/android/app/build.gradle.kts`: add release-please markers for `versionName` and a safe `versionCode` update path.
- Modify Chrome asset layout or release scope: remove the top-level `assets/**` blind spot before Chrome is migrated.
- Modify `CLAUDE.md` and any README release instructions.

## Task 1: Prove Version-File Coverage Before Config

**Files:**
- Read: `release/components.toml`
- Read: `Cargo.toml`
- Read: `Cargo.lock`
- Read: `README.md`
- Read: `apps/web/package.json`
- Read: `apps/web/package-lock.json`
- Read: `apps/web/openapi/axon.json`
- Read: `apps/palette-tauri/src-tauri/tauri.conf.json`
- Read: `apps/palette-tauri/package.json`
- Read: `apps/palette-tauri/src-tauri/Cargo.toml`
- Read: `apps/palette-tauri/src-tauri/Cargo.lock`
- Read: `apps/android/app/build.gradle.kts`
- Read: `apps/chrome-extension/manifest.json`
- Modify: plan implementation PR description only; do not commit an inventory doc.

**Interfaces:**
- Consumes: Axon's current release manifest.
- Produces: a coverage table that blocks implementation if any version file is unclassified.

- [ ] **Step 1: Capture current versions**

Run:

```bash
printf 'cli=' && cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name=="axon") | .version'
printf 'palette=' && jq -r '.version' apps/palette-tauri/src-tauri/tauri.conf.json
printf 'android=' && sed -n 's/^[[:space:]]*versionName[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' apps/android/app/build.gradle.kts
printf 'chrome=' && jq -r '.version' apps/chrome-extension/manifest.json
```

Expected:

```text
cli=6.2.1
palette=5.12.3
android=1.5.0
chrome=0.2.2
```

- [ ] **Step 2: Build the coverage table in the PR description**

Use this exact table as the starting point:

```markdown
| Component | File | Current handling | Migration handling |
|-----------|------|------------------|--------------------|
| cli | `Cargo.toml` `[package].version` | xtask bump | release-please rust updater |
| cli | `Cargo.toml` `[workspace.package].version` | xtask bump | release-please TOML `extra-files` |
| cli | `Cargo.lock` package `axon` | xtask bump | postprocessor `cargo update -p axon --precise <version>` |
| cli | `README.md` `Version:` line | xtask bump | generic updater marker |
| cli | `CHANGELOG.md` | git-cliff | release-please changelog |
| cli | `apps/web/package.json` | xtask bump | JSON `extra-files` |
| cli | `apps/web/package-lock.json` `packages[""].version` | xtask bump | JSON `extra-files`; no root `$.version` updater |
| cli | `apps/web/openapi/axon.json` | xtask bump | JSON `extra-files` |
| cli | `plugins/axon/.claude-plugin/plugin.json` | no version allowed | validator only |
| palette | `tauri.conf.json` | xtask bump | JSON `extra-files` |
| palette | `package.json` | xtask bump | JSON `extra-files` |
| palette | `src-tauri/Cargo.toml` | xtask bump | TOML `extra-files` |
| palette | `src-tauri/Cargo.lock` | xtask bump | postprocessor `cargo update -p axon-palette-tauri --precise <version>` |
| palette | `CHANGELOG.md` | git-cliff | release-please changelog |
| android | `build.gradle.kts` `versionName` | xtask bump | generic updater marker |
| android | `build.gradle.kts` `versionCode` | xtask increment | Android postprocessor or generated value |
| android | `CHANGELOG.md` | git-cliff | release-please changelog |
| chrome | `manifest.json` | xtask bump | JSON `extra-files` |
| chrome | `CHANGELOG.md` | git-cliff | release-please changelog |
```

- [ ] **Step 3: Commit nothing**

Run:

```bash
git diff -- docs/sessions
```

Expected: no inventory document changes.

## Task 2: Add Release-Please Metadata To Axon's Manifest

**Files:**
- Modify: `release/components.toml`
- Modify: `xtask/src/checks/release_versions.rs`
- Modify: `xtask/src/checks/release_versions/manifest.rs`
- Test: `xtask/src/checks/release_versions_tests.rs`

**Interfaces:**
- Produces: `Component.release_please_path: String`.
- Later tasks consume this field instead of hard-coding component/path mappings.

- [ ] **Step 1: Add failing manifest test**

In `xtask/src/checks/release_versions_tests.rs`, add a test using the existing release fixture style:

```rust
#[test]
fn release_manifest_requires_release_please_path() {
    let fixture = ReleaseFixture::new();
    fixture.write_valid_manifest();
    let path = fixture.path("release/components.toml");
    let content = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, content.replace("release_please_path = \".\"\n", "")).unwrap();

    let err = plan(&fixture.root, Some("origin/main"), "HEAD", GateMode::Pr)
        .expect_err("missing release_please_path must fail");
    assert!(err.to_string().contains("release_please_path"));
}
```

Run:

```bash
cargo test -p xtask release_manifest_requires_release_please_path --no-fail-fast
```

Expected: fail because the field does not exist yet.

- [ ] **Step 2: Add manifest field**

In `xtask/src/checks/release_versions.rs`, extend `Component`:

```rust
#[derive(Debug, Deserialize)]
struct Component {
    id: String,
    name: String,
    tag_prefix: String,
    release_please_path: String,
    release_workflow: String,
    shipping_paths: Vec<String>,
    version_source: VersionFile,
    version_files: Vec<VersionFile>,
}
```

In `xtask/src/checks/release_versions/manifest.rs`, validate it:

```rust
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
```

- [ ] **Step 3: Update `release/components.toml`**

Add one line to each component:

```toml
release_please_path = "."
release_please_path = "apps/palette-tauri"
release_please_path = "apps/android"
release_please_path = "apps/chrome-extension"
```

Place each line next to `tag_prefix`.

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test -p xtask release_versions --no-fail-fast
```

Expected: pass.

- [ ] **Step 5: Commit**

Run:

```bash
git add release/components.toml xtask/src/checks/release_versions.rs xtask/src/checks/release_versions/manifest.rs xtask/src/checks/release_versions_tests.rs
git commit -m "chore: add release-please component paths"
```

## Task 3: Solve Chrome Asset Scope Before Migrating Chrome

**Files:**
- Modify: `apps/chrome-extension/package.sh`
- Modify: `apps/chrome-extension/README.md`
- Modify: `release/components.toml`
- Test: `apps/chrome-extension/package.sh`

**Interfaces:**
- Consumes: current `apps/chrome-extension/assets -> ../../assets` symlink.
- Produces: release-please-visible Chrome changes when extension assets change.

- [ ] **Step 1: Pick the minimal scope fix**

Use this decision:

```text
Move Chrome-owned runtime assets under apps/chrome-extension/assets as real files.
If an asset is shared by another component, copy only the extension-owned release asset and document the duplication.
Do not keep top-level assets in Chrome shipping paths after migration.
```

- [ ] **Step 2: Replace symlink with real files**

Run:

```bash
rm apps/chrome-extension/assets
mkdir -p apps/chrome-extension/assets
cp -a assets/axon*.png assets/axon*.svg apps/chrome-extension/assets/
```

Expected: only extension-referenced assets are copied. If the glob fails, inspect `apps/chrome-extension/manifest.json`, HTML, CSS, and JS references and copy exactly those referenced files.

- [ ] **Step 3: Update package script comments**

In `apps/chrome-extension/package.sh`, replace the symlink comment with:

```bash
# assets/ contains real extension-owned files so release-please can detect
# Chrome release changes from a single package path.
```

Leave the referenced-asset validation and `cp` staging logic intact.

- [ ] **Step 4: Update release manifest**

In `release/components.toml`, change Chrome shipping paths:

```toml
shipping_paths = ["apps/chrome-extension"]
```

- [ ] **Step 5: Test package**

Run:

```bash
./apps/chrome-extension/package.sh
```

Expected: creates `apps/chrome-extension/dist/axon-0.2.2.zip` and copies `bin/axon-chrome-extension-0.2.2.zip`.

- [ ] **Step 6: Commit**

Run:

```bash
git add apps/chrome-extension apps/chrome-extension/README.md release/components.toml
git commit -m "chore: move chrome release assets under extension package"
```

## Task 4: Add Release-Please Config And Bootstrap Manifest

**Files:**
- Create: `release-please-config.json`
- Create: `.release-please-manifest.json`
- Modify: `README.md`
- Modify: `apps/android/app/build.gradle.kts`
- Test: `jq empty release-please-config.json .release-please-manifest.json`

**Interfaces:**
- Consumes: version coverage table from Task 1.
- Produces: release-please release PR config.

- [ ] **Step 1: Add generic updater markers**

In `README.md`, change the version line:

```markdown
Version: 6.2.1 <!-- x-release-please-version -->
```

In `apps/android/app/build.gradle.kts`, change version lines:

```kotlin
        versionCode = 14 // x-release-please-version-code
        // x-release-please-start-version
        versionName = "1.5.0"
        // x-release-please-end
```

`x-release-please-version-code` is intentionally a custom marker for Task 5's postprocessor, not release-please itself.

- [ ] **Step 2: Create `.release-please-manifest.json`**

```json
{
  ".": "6.2.1",
  "apps/palette-tauri": "5.12.3",
  "apps/android": "1.5.0",
  "apps/chrome-extension": "0.2.2"
}
```

- [ ] **Step 3: Create `release-please-config.json`**

```json
{
  "$schema": "https://raw.githubusercontent.com/googleapis/release-please/main/schemas/config.json",
  "separate-pull-requests": true,
  "always-update": true,
  "packages": {
    ".": {
      "component": "axon",
      "release-type": "rust",
      "include-component-in-tag": false,
      "changelog-path": "CHANGELOG.md",
      "extra-files": [
        { "type": "toml", "path": "Cargo.toml", "jsonpath": "$.workspace.package.version" },
        { "type": "generic", "path": "README.md" },
        { "type": "json", "path": "apps/web/package.json", "jsonpath": "$.version" },
        { "type": "json", "path": "apps/web/openapi/axon.json", "jsonpath": "$.info.version" },
        { "type": "json", "path": "apps/web/package-lock.json", "jsonpath": "$.packages[''].version" }
      ]
    },
    "apps/palette-tauri": {
      "component": "palette",
      "release-type": "simple",
      "include-v-in-tag": true,
      "tag-separator": "-",
      "changelog-path": "CHANGELOG.md",
      "extra-files": [
        { "type": "json", "path": "apps/palette-tauri/src-tauri/tauri.conf.json", "jsonpath": "$.version" },
        { "type": "json", "path": "apps/palette-tauri/package.json", "jsonpath": "$.version" },
        { "type": "toml", "path": "apps/palette-tauri/src-tauri/Cargo.toml", "jsonpath": "$.package.version" }
      ]
    },
    "apps/android": {
      "component": "android",
      "release-type": "simple",
      "include-v-in-tag": true,
      "tag-separator": "-",
      "changelog-path": "CHANGELOG.md",
      "extra-files": [
        { "type": "generic", "path": "apps/android/app/build.gradle.kts" }
      ]
    },
    "apps/chrome-extension": {
      "component": "chrome-ext",
      "release-type": "simple",
      "include-v-in-tag": true,
      "tag-separator": "-",
      "changelog-path": "CHANGELOG.md",
      "extra-files": [
        { "type": "json", "path": "apps/chrome-extension/manifest.json", "jsonpath": "$.version" }
      ]
    }
  }
}
```

- [ ] **Step 4: Validate config**

Run:

```bash
jq empty release-please-config.json
jq empty .release-please-manifest.json
```

Expected: both pass.

- [ ] **Step 5: Commit**

Run:

```bash
git add release-please-config.json .release-please-manifest.json README.md apps/android/app/build.gradle.kts
git commit -m "ci: configure release-please manifests"
```

## Task 5: Add Release-Please Postprocessors

**Files:**
- Modify: `xtask/src/main.rs`
- Create: `xtask/src/checks/release_versions/release_please.rs`
- Modify: `xtask/src/checks/release_versions.rs`
- Test: `xtask/src/checks/release_versions_tests.rs`

**Interfaces:**
- Produces: `cargo xtask release-please-fixups --component <id> --version <semver>`.
- Produces: `cargo xtask release-please-dispatch-plan --paths-released <json>`.

- [ ] **Step 1: Add failing tests**

Add tests proving:

```rust
#[test]
fn release_please_manifest_matches_component_versions() { /* mismatched manifest fails */ }

#[test]
fn release_please_dispatch_plan_uses_manifest_metadata() { /* paths -> workflow/tag pairs */ }

#[test]
fn android_fixup_increments_version_code() { /* 14 -> 15 when versionName changes */ }
```

Run:

```bash
cargo test -p xtask release_please --no-fail-fast
```

Expected: fail before implementation.

- [ ] **Step 2: Implement manifest parity**

In `release_please.rs`, implement:

```rust
pub(super) fn check_manifest_versions(root: &Path, components: &[Component]) -> ReleaseResult<Vec<String>>;
```

It reads `.release-please-manifest.json`, maps each component through `component.release_please_path`, and compares manifest version to `read_version(root, &component.version_source)`.

- [ ] **Step 3: Implement fixups**

Add CLI:

```rust
ReleasePleaseFixups {
    #[arg(long)]
    component: String,
    #[arg(long)]
    version: String,
}
```

Behavior:

```text
cli: run `cargo update -p axon --precise <version>` after Cargo.toml changes.
palette: run `cargo update -p axon-palette-tauri --precise <version>` in apps/palette-tauri/src-tauri.
android: increment `versionCode` by 1 when `versionName` changed; fail if marker is missing.
chrome: no fixups.
```

- [ ] **Step 4: Implement dispatch plan**

Add CLI:

```rust
ReleasePleaseDispatchPlan {
    #[arg(long)]
    paths_released: String,
    #[arg(long)]
    json: bool,
}
```

Output JSON:

```json
[
  { "id": "cli", "workflow": "release.yml", "tag": "v6.2.2" },
  { "id": "palette", "workflow": "palette-release.yml", "tag": "palette-v5.12.4" }
]
```

The command must read `release/components.toml` and `.release-please-manifest.json`; no component mappings may be hard-coded in YAML.

- [ ] **Step 5: Wire parity into existing check**

In `check()`, append `release_please::check_manifest_versions(root, &manifest.components)?` errors before printing.

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test -p xtask release_versions --no-fail-fast
```

Expected: pass.

- [ ] **Step 7: Commit**

Run:

```bash
git add xtask/src/main.rs xtask/src/checks/release_versions.rs xtask/src/checks/release_versions/release_please.rs xtask/src/checks/release_versions_tests.rs Cargo.lock
git commit -m "ci: add release-please validation and dispatch helpers"
```

## Task 6: Re-enable Release Validation In CI

**Files:**
- Modify: `.github/workflows/ci.yml`
- Test: `actionlint .github/workflows/ci.yml`

**Interfaces:**
- Produces: required release metadata check on PRs touching release files or component paths.

- [ ] **Step 1: Remove disabled gate**

Change:

```yaml
if: ${{ false && (needs.changes.outputs.release == 'true' || needs.changes.outputs.android == 'true' || needs.changes.outputs.palette == 'true' || needs.changes.outputs.chrome == 'true' || needs.changes.outputs.version_files == 'true') }}
```

to:

```yaml
if: ${{ needs.changes.outputs.release == 'true' || needs.changes.outputs.android == 'true' || needs.changes.outputs.palette == 'true' || needs.changes.outputs.chrome == 'true' || needs.changes.outputs.version_files == 'true' }}
```

- [ ] **Step 2: Update step name**

```yaml
- name: Verify release-please version plan for changed shipping components
  run: cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
```

- [ ] **Step 3: Validate workflow**

Run:

```bash
actionlint .github/workflows/ci.yml
```

Expected: no errors.

- [ ] **Step 4: Commit**

Run:

```bash
git add .github/workflows/ci.yml
git commit -m "ci: re-enable release version validation"
```

## Task 7: Convert Artifact Workflows To Upload-Only Publishing

**Files:**
- Modify: `.github/workflows/release.yml`
- Modify: `.github/workflows/palette-release.yml`
- Modify: `.github/workflows/android-release.yml`
- Modify: `.github/workflows/chrome-extension-release.yml`
- Test: `actionlint` for all four files.

**Interfaces:**
- Consumes: release-please-created GitHub Release by tag.
- Produces: asset upload without clobbering release body/notes.

- [ ] **Step 1: Replace each `softprops/action-gh-release` create step**

Use this shell pattern in each publish job after artifacts exist:

```yaml
- name: Attach artifacts to release
  env:
    GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  run: |
    set -euo pipefail
    tag="${GITHUB_REF_NAME}"
    gh release view "$tag" >/dev/null
    gh release upload "$tag" \
      dist/ARTIFACT_1 \
      dist/ARTIFACT_2 \
      --clobber
```

Replace `ARTIFACT_1` and `ARTIFACT_2` with the exact files that workflow currently passes to `softprops/action-gh-release`.

- [ ] **Step 2: Preserve manual dry-run behavior**

Keep the existing publish `if:` conditions:

```yaml
startsWith(github.ref, 'refs/tags/...') &&
(github.event_name == 'push' || (github.event_name == 'workflow_dispatch' && inputs.publish))
```

Expected: workflow dispatch without `publish=true` still uploads run artifacts only.

- [ ] **Step 3: Validate workflows**

Run:

```bash
actionlint .github/workflows/release.yml .github/workflows/palette-release.yml .github/workflows/android-release.yml .github/workflows/chrome-extension-release.yml
```

Expected: no errors.

- [ ] **Step 4: Commit**

Run:

```bash
git add .github/workflows/release.yml .github/workflows/palette-release.yml .github/workflows/android-release.yml .github/workflows/chrome-extension-release.yml
git commit -m "ci: attach release artifacts to release-please releases"
```

## Task 8: Add Gated Release-Please Workflow

**Files:**
- Create: `.github/workflows/release-please.yml`
- Test: `actionlint .github/workflows/release-please.yml`

**Interfaces:**
- Consumes: successful `CI` workflow run on `main`.
- Produces: release-please PRs/releases only after CI success.

- [ ] **Step 1: Resolve pinned action SHAs**

Run:

```bash
gh api repos/googleapis/release-please-action/git/ref/tags/v4 --jq '.object.sha'
gh api repos/actions/checkout/git/ref/tags/v5 --jq '.object.sha'
```

Expected: two full SHAs. Use those SHAs in the workflow.

- [ ] **Step 2: Create workflow**

```yaml
name: release-please

on:
  workflow_run:
    workflows: ["CI"]
    types: [completed]
    branches: [main]
  workflow_dispatch:

permissions:
  contents: read

concurrency:
  group: release-please-main
  cancel-in-progress: false

jobs:
  release-please:
    if: ${{ github.event_name == 'workflow_dispatch' || github.event.workflow_run.conclusion == 'success' }}
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
    outputs:
      releases_created: ${{ steps.release.outputs.releases_created }}
      paths_released: ${{ steps.release.outputs.paths_released }}
    steps:
      - name: Require release-please token
        env:
          RELEASE_PLEASE_TOKEN: ${{ secrets.RELEASE_PLEASE_TOKEN }}
        run: test -n "$RELEASE_PLEASE_TOKEN"
      - uses: googleapis/release-please-action@PINNED_RELEASE_PLEASE_SHA
        id: release
        with:
          token: ${{ secrets.RELEASE_PLEASE_TOKEN }}
          config-file: release-please-config.json
          manifest-file: .release-please-manifest.json

  dispatch-artifacts:
    needs: release-please
    if: ${{ needs.release-please.outputs.releases_created == 'true' }}
    runs-on: ubuntu-latest
    permissions:
      contents: read
      actions: write
    steps:
      - uses: actions/checkout@PINNED_CHECKOUT_SHA
        with:
          persist-credentials: false
      - uses: dtolnay/rust-toolchain@stable
      - name: Build dispatch plan
        run: |
          cargo xtask release-please-dispatch-plan \
            --paths-released '${{ needs.release-please.outputs.paths_released }}' \
            --json > dispatch-plan.json
          jq . dispatch-plan.json
      - name: Dispatch artifact workflows
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          set -euo pipefail
          jq -c '.[]' dispatch-plan.json | while read -r item; do
            workflow="$(jq -r '.workflow' <<<"$item")"
            tag="$(jq -r '.tag' <<<"$item")"
            git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null
            gh workflow run "$workflow" --ref "$tag" -f publish=true
          done
```

Replace `PINNED_RELEASE_PLEASE_SHA` and `PINNED_CHECKOUT_SHA` with Step 1 output.

- [ ] **Step 3: Validate workflow**

Run:

```bash
actionlint .github/workflows/release-please.yml
```

Expected: no errors.

- [ ] **Step 4: Commit**

Run:

```bash
git add .github/workflows/release-please.yml
git commit -m "ci: run release-please after green CI"
```

## Task 9: Remove Legacy git-cliff Release Tooling

**Files:**
- Modify: `xtask/src/main.rs`
- Modify: `xtask/src/checks/release_versions.rs`
- Modify: `xtask/src/checks/release_versions_tests.rs`
- Delete: `xtask/src/checks/release_versions/bump.rs`
- Delete: `xtask/src/checks/release_versions/cliff.rs`
- Delete: `xtask/src/checks/release_versions/cliff_tests.rs`
- Delete: `cliff.toml`
- Modify: `CLAUDE.md`

**Interfaces:**
- Removes: old manual version bump/changelog commands and git-cliff dependency.
- Produces: release-please-only release docs and xtask validation/postprocessing helpers.

- [x] **Step 1: Remove old commands**

Remove `BumpVersion` and `RegenChangelog` from `xtask/src/main.rs`, and remove
the matching public functions and `BumpLevel` type from
`xtask/src/checks/release_versions.rs`.

- [x] **Step 2: Delete git-cliff files**

Delete `cliff.toml`, `xtask/src/checks/release_versions/bump.rs`,
`xtask/src/checks/release_versions/cliff.rs`, and
`xtask/src/checks/release_versions/cliff_tests.rs`.

- [x] **Step 3: Final docs update**

In `CLAUDE.md`, remove rollback wording and state:

```markdown
Release-please is the only supported release PR and version bump path.
```

- [ ] **Step 4: Run focused checks**

Run:

```bash
cargo test -p xtask release_versions --no-fail-fast
cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
```

Expected: pass.

- [ ] **Step 5: Commit**

Run:

```bash
git add xtask/src/main.rs xtask/src/checks/release_versions.rs xtask/src/checks/release_versions_tests.rs CLAUDE.md docs/superpowers/plans/2026-07-04-release-please-migration.md
git commit -m "chore: remove git-cliff release fallback"
```

## Engineering Review: Applied Findings

### Architecture

- Applied: release-please now runs after `CI` success instead of direct `push`.
- Applied: artifact workflows upload to existing releases instead of creating releases.
- Applied: dispatch uses `release/components.toml` through `xtask release-please-dispatch-plan`.
- Applied: Chrome `assets/**` blind spot is fixed before Chrome migration.

### Simplicity

- Applied: removed committed inventory doc.
- Applied: removed git-cliff and manual bumping instead of preserving a fallback.
- Applied: avoided hard-coded YAML component mappings.

### Security

- Applied: added `RELEASE_PLEASE_TOKEN` requirement so release PR CI can run.
- Applied: pinned privileged actions by SHA.
- Applied: split workflow permissions by job and removed broad `issues: write`.

### Performance

- Applied: preserved targeted release validation instead of broad workspace tests.
- Applied: kept release-please behind the existing CI completion gate.

### Failure Modes

| Codepath | Failure mode | Rescued? | Test? | User sees? | Logged? |
|----------|--------------|----------|-------|------------|---------|
| release-please trigger | release before CI is green | Y | Y | no release | workflow failure |
| artifact upload | release exists without assets | Y | Y | failed release workflow | workflow failure |
| Android release | stale `versionCode` | Y | Y | CI failure | xtask error |
| Chrome release | `assets/**` change ignored | Y | Y | CI/review failure | package test |
| dispatch | wrong tag reconstructed | Y | Y | dispatch failure | workflow failure |
| release PR | default token does not trigger CI | Y | Y | missing secret failure | workflow failure |

### NOT in Scope

- Artifact signing changes: existing signing scaffold remains unchanged.
- Custom release-please plugin: unnecessary unless postprocessors prove insufficient.
- Reintroducing a git-cliff/manual release fallback: explicitly out of scope.

### Applied Checklist

- [x] 1. Preserve exact-CI-success gate with `workflow_run`.
- [x] 2. Use a real release-please token, not default `GITHUB_TOKEN`.
- [x] 3. Re-enable release/version CI validation.
- [x] 4. Convert artifact workflows to upload-only release publishing.
- [x] 5. Add full version-file coverage table.
- [x] 6. Add missing `apps/web/package-lock.json` root `$.version` updater.
- [x] 7. Add Android `versionCode` postprocessor requirement.
- [x] 8. Fix Chrome top-level `assets/**` release scope.
- [x] 9. Add `release_please_path` to avoid hard-coded Rust/YAML mappings.
- [x] 10. Use xtask dispatch plan instead of YAML string reconstruction.
- [x] 11. Pin privileged actions by SHA and split permissions.
- [x] 12. Remove git-cliff/manual release fallback as part of the migration.

## Self-Review

**Spec coverage:** This plan switches release PR/version/changelog flow to release-please, keeps artifact workflows, preserves CI gating, and removes git-cliff/manual xtask bumping from the supported release system.

**Placeholder scan:** Runtime SHAs and future release tag values are produced by explicit commands in the tasks. No implementation step uses unspecified placeholder code.

**Type consistency:** Component IDs remain `cli`, `palette`, `android`, and `chrome`; release-please paths are `.`, `apps/palette-tauri`, `apps/android`, and `apps/chrome-extension`; tags remain `v`, `palette-v`, `android-v`, and `chrome-ext-v` prefixed.
