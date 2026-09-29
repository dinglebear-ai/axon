---
title: "Release Checklist — Axon"
created: 2026-07-07
updated: 2026-09-27
---

# Release Checklist — Axon

Pre-release checklist for the current per-component release-driver pipeline. See the
`release/components.toml` for component ownership and the detailed release
contract below for version, tag, and artifact responsibilities.

Releases are per-component (`cli`, `palette`, `android`, `chrome`) and
selective. Release-please owns release PRs, version bumps, changelogs, tags,
and GitHub Releases for `palette`, `android`, and `chrome`. The primary CLI is
Axon-native-driven: `xtask` synchronizes its version, then auto-tag creates its
tag and GitHub Release after exact-main CI before dispatching artifacts.

## Before merging a change that ships in a release

- [ ] CLI shipping changes include an `xtask bump-version` result and all CLI
      version-bearing files are in sync.
- [ ] Ordinary `palette`/`android`/`chrome` feature PRs do **not** edit version
      files; release-please owns those edits in the generated release PR.
- [ ] `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`
      passes. It requires a CLI bump, defers managed feature bumps, rejects
      mixed managed shipping/version edits, and fully validates generated
      release PR version parity.
- [ ] Conventional commit prefixes are correct: `feat!`/`BREAKING CHANGE` →
      major, `feat` → minor, `fix` → patch. `perf`/`refactor` show in the
      Changed changelog section; `chore`/`ci`/`docs`/`test`/`build`/`style`
      are hidden from release notes.
- [ ] `plugins/axon/.claude-plugin/plugin.json` has **no** `version` key
      (`just validate-plugin`, part of `just verify`, hard-fails on this).

### Component version-bearing files

| Component | Files that must move together | Version source |
|---|---|---|
| **cli** | `Cargo.toml` (`[package]` and `[workspace.package]` versions), `Cargo.lock`, `README.md`, `CHANGELOG.md`, `apps/web/package.json`, `apps/web/package-lock.json`, `apps/web/openapi/axon.json` | `Cargo.toml` |
| **palette** | `apps/palette-tauri/src-tauri/tauri.conf.json`, `apps/palette-tauri/package.json`, `apps/palette-tauri/src-tauri/Cargo.toml` | `tauri.conf.json` |
| **android** | `apps/android/app/build.gradle.kts` (`versionName` + `versionCode`) | `build.gradle.kts` |
| **chrome** | `apps/chrome-extension/manifest.json` | `manifest.json` |

## Build and test

- [ ] `just verify` passes for shipping changes (the Justfile owns the complete gate sequence)
- [ ] `just precommit` passes for shipping changes (including secret and structural checks)
- [ ] Web panel builds: `cd apps/web && npm run build`
- [ ] `axon doctor` reports all required services healthy (Qdrant, TEI)
- [ ] `cargo xtask check-layering` passes (no forbidden crate-dependency reaches)
- [ ] `cargo xtask check-no-mod-rs` passes (no `mod.rs` reintroduced)

## Security

- [ ] No credentials in code, docs, or git history
- [ ] `.gitignore`/`.dockerignore` include `.env`, `*.secret`, `.git/`
- [ ] Docker containers run as non-root (`user: "1000:1000"`)
- [ ] No baked environment variables in Docker images
- [ ] MCP/action auth uses `AXON_HTTP_TOKEN` or OAuth for non-loopback binds

See [`contributing.md`](contributing.md#security-guardrails) for the full
guardrail set.

## Infrastructure

- [ ] The supported deployment runs native `axon serve` under systemd,
      directly or inside the documented Incus system container.
- [ ] Qdrant, TEI, and Chrome endpoints are reachable from that deployment;
      provider containers are not confused with the Axon process lifecycle.
- [ ] `axon serve` owns the unified SQLite job worker runtime and watch
      scheduler; source work does not use per-family worker services.
- [ ] Required migrations apply cleanly through their owning stores; consult
      `docs/reference/runtime/database-schema.json` for migration provenance.

## Documentation

- [ ] Canonical `AGENTS.md` files match the current implementation; sibling
      `CLAUDE.md` and `GEMINI.md` remain direct relative symlinks.
- [ ] Changed CLI/MCP/REST/config contracts are regenerated through
      `cargo xtask generated-contracts refresh` and verified with
      `cargo xtask generated-contracts check`.
- [ ] Current usage guides and generated registries describe new commands;
      dated pipeline-unification delivery records remain historical.
- [ ] Documentation-only maintenance follows the scoped checks in
      [documentation.md](documentation.md), not the full shipping checklist.

## Monolith policy

- [ ] No changed `.rs` files exceed 500 lines (except allowlisted in
      `.monolith-allowlist`)
- [ ] No changed functions exceed 120 lines
- [ ] `python3 scripts/enforce_monoliths.py --staged` passes locally

See [`contributing.md`](contributing.md#monolith-policy) for the full policy.

## SQLite

- [ ] New migrations are append-only — never edit an already-applied
      migration; add a new one instead
- [ ] New migrations are recorded with
      `cargo xtask update-sqlite-migration-checksums` (per-crate migration
      checksums, e.g. `crates/axon-ledger/src/migration-checksums.txt`)
- [ ] Schema changes are reflected in the relevant store's read/list/recover
      paths (`axon-jobs`, `axon-ledger`, `axon-memory`)
- [ ] Migration upgrade path works against an existing `~/.axon/jobs.db`

## Web panel

- [ ] `apps/web` builds without errors
- [ ] Panel routes still require panel password/session or MCP/action auth as
      appropriate
- [ ] No `NEXT_PUBLIC_*` variables leak server-side secrets

## Cutting a release-please-driven release (`palette`, `android`, `chrome`)

1. Let release-please open or refresh the component release PR after green
   `CI` on `main`.
2. Review that the release PR updates `.release-please-manifest.json`, the
   component version files, and its changelog together.
3. Run `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`.
4. Merge only after the release/version gate and CI are green.
5. Confirm release-please created the component tag and GitHub Release.
6. Confirm `palette-release.yml`, `android-release.yml`, or
   `chrome-extension-release.yml` attached the signed/checksummed artifacts to
   that existing Release.

## Cutting a CLI release

1. Run `cargo xtask bump-version patch|minor|major --component cli` and review
   every CLI version-bearing file.
2. Include the bump with the shipping PR and run the PR release-version gate.
3. Merge only after CI is green.
4. Confirm auto-tag selected only the Axon-native CLI, waited for exact-main CI,
   created the `vX.Y.Z` tag and GitHub Release, then dispatched `release.yml`.
5. Confirm the Linux and Windows assets and checksums were attached.

Direct tags for release-please-driven components are break-glass incident
operations. Do not use them as a normal hotfix path or create a second owner
for managed version files, tags, or GitHub Releases.

## Detailed release contract

### How releases work

Releases are **per-component and selective**. **Three of four components**
(`palette`, `android`, `chrome`) are release-please-driven: release PRs are
generated by release-please after `CI` succeeds on `main`, and release-please
owns version edits, changelog edits, tags, and GitHub Release records for
them. Axon's artifact workflows attach binaries/APKs/zips to the
release-please-created release by tag.

**The `cli` component is Axon-native-driven.** It is the primary application,
and Axon's own `xtask` + `auto-tag` pipeline owns its version validation,
tag, GitHub Release, and artifact dispatch end to end. release-please is not
its driver because release-please's Cargo workspace plugin cannot handle
`version.workspace = true`, the standard Cargo workspace-inheritance pattern
used throughout this repository. The active upstream limitation is
[googleapis/release-please#2111](https://github.com/googleapis/release-please/issues/2111).
The candidate-PR build for `.` failed identically across every
release-please-action major version tested (v4 and v5, i.e. release-please
core 17.6.0 and 17.10.2); no config flag (`release-type`, `extra-files`,
`always-link-local`) works around it. `.` therefore remains outside
`release-please-config.json` and `.release-please-manifest.json`. Bump the
CLI with **`cargo xtask bump-version patch|minor|major --component cli`**; after
merge, `auto-tag` performs the automated release. `release/components.toml`
records this explicitly as `release_driver = "axon-native"`.

`release/components.toml` remains Axon's release-control source of truth. It
defines component shipping paths, tag prefixes, release workflows, version
sources, version-bearing files, release-please package paths, and the explicit
release driver for every component:

| Component | Shipping paths | Version source | Tag prefix | Release workflow | Release driver |
|-----------|----------------|----------------|-----------|------------------|----------------|
| **cli** (Linux + Windows; web panel bundled in) | `src`, `crates`, `Cargo.toml`/`Cargo.lock`, `build.rs`, `apps/web`, `rust-toolchain.toml`, `vendor` | `Cargo.toml` `[package]` version | `v` | `release.yml` | **Axon native (xtask + auto-tag)** |
| **palette** (Linux + Windows) | `apps/palette-tauri` | `apps/palette-tauri/src-tauri/tauri.conf.json` | `palette-v` | `palette-release.yml` | release-please |
| **android** (APK) | `apps/android` | `apps/android/app/build.gradle.kts` `versionName` | `android-v` | `android-release.yml` | release-please |
| **chrome** (extension zip) | `apps/chrome-extension` | `apps/chrome-extension/manifest.json` `version` | `chrome-ext-v` | `chrome-extension-release.yml` | release-please |

For each component, the shared release checker diffs that component's shipping
paths against its most recent tag and verifies the component version source
and all version-bearing files agree (plus, for release-please-driven
components, that the release-please manifest agrees too). After
release-please creates a release for `palette`/`android`/`chrome`,
`.github/workflows/release-please.yml` dispatches that component's artifact
workflow. For the Axon-native `cli`, `.github/workflows/auto-tag.yml` selects
only components whose `release_driver` plan field is `"axon-native"`, waits
for exact-main CI, creates the `vX.Y.Z` tag and GitHub Release, then dispatches
`release.yml` to attach the artifacts.

**Implications:**

- A change touching only one component releases only that component — e.g. an
  `apps/android/**`-only change cuts an Android release and nothing else; it
  does **not** rebuild the CLI.
- Dev-only trees (`xtask`, `xtask-release`, `benches`, `.github`, `docs`, and
  non-shipping repo policy/config files) are
  **not** in any component's shipping paths, so a tooling/docs-only merge cuts
  no release and needs no version bump. `Cargo.toml`, `Cargo.lock`, and
  `rust-toolchain.toml` are CLI shipping paths — but a `Cargo.lock`-only diff
  whose changed package sections are all dev-only workspace members stays in
  the carve-out.
- An ordinary `palette`/`android`/`chrome` feature PR changes shipping files
  without touching version files. The PR gate validates current parity and
  defers the bump to release-please. It rejects a feature PR that mixes those
  shipping changes with manual version edits. The generated release PR is the
  only normal place those managed version files move.
- A `cli` shipping PR must include the manual `cargo xtask bump-version ...`
  result. The PR gate rejects an unchanged or already-tagged CLI version.
- Direct tags for release-please-driven components are break-glass incident
  operations, not a normal release path. Do not use them to bypass the managed
  release PR, tag, or GitHub Release owner.

### Version bumping rules

Release-please is the release PR, version bump, changelog, tag, and GitHub
Release path for `palette`/`android`/`chrome`. Do not run or reintroduce
git-cliff-backed release bumping. `cli` is the one exception — bump it with
`cargo xtask bump-version patch|minor|major --component cli`, choosing the level yourself
(no commit-message parsing, since there's no release-please PR to compute it
from):

```bash
cargo xtask bump-version patch --component cli   # or minor / major
```

This writes every one of `cli`'s version-bearing files below in one shot
(including running `cargo update -p axon --precise X.Y.Z` to refresh
`Cargo.lock`, and every workspace member's own `Cargo.lock` entry that
inherits via `version.workspace = true`), inserts a dated `## [X.Y.Z]`
`CHANGELOG.md` heading, and is idempotent — a second run at the same version
is a no-op. It is a pure text-substitution writer (deliberately not a
`serde_json`/full-manifest round-trip — that reformatted
`apps/web/openapi/axon.json` wholesale in testing), so it only ever touches
the specific version field, byte-for-byte, everywhere else in the file
untouched.

Release-please (for `palette`/`android`/`chrome`) determines the bump type
from conventional commits:

- `feat!:` or `BREAKING CHANGE` → **major** (X+1.0.0)
- `feat` or `feat(...)` → **minor** (X.Y+1.0)
- `fix` or `fix(...)` → **patch** (X.Y.Z+1)
- `perf` and `refactor` appear in the **Changed** changelog section when they
  are part of a release.
- `chore`, `ci`, `docs`, `test`, `build`, and `style` are hidden from
  generated release notes by `release-please-config.json` so non-user-facing
  maintenance commits do not bury the release signal.

For `cli`, pick the equivalent level yourself using the same rules when
choosing `patch`/`minor`/`major`.

**CLI component — all of these MUST move together (Cargo.toml is the source of truth; `cargo xtask bump-version patch --component cli` handles all of it):**
- `Cargo.toml` — `version = "X.Y.Z"` in both `[package]` and `[workspace.package]` (Cargo.lock follows automatically)
- `README.md` — version header (no longer carries the `x-release-please-version` marker — release-please doesn't touch this file anymore)
- `CHANGELOG.md` — new entry under the bumped version
- `apps/web/package.json` + `apps/web/package-lock.json` — root package `"version": "X.Y.Z"`
- `apps/web/openapi/axon.json` — `"info.version": "X.Y.Z"`

**Palette component — all three MUST move together (release-please-driven):**
- `apps/palette-tauri/src-tauri/tauri.conf.json`, `apps/palette-tauri/package.json`,
  `apps/palette-tauri/src-tauri/Cargo.toml`

**Android component (release-please-driven):** `apps/android/app/build.gradle.kts` `versionName` (bump
`versionCode` too). **Chrome component (release-please-driven):** `apps/chrome-extension/manifest.json`.

`plugins/axon/.claude-plugin/plugin.json` must **NOT** carry a `version` key —
`just validate-plugin` (part of `just verify`) hard-fails on it; the plugin is
versioned by the marketplace, not the manifest.

**Changelogs are generated, not hand-stamped.** Each component has its own
`CHANGELOG.md` (`CHANGELOG.md`, `apps/palette-tauri/CHANGELOG.md`,
`apps/android/CHANGELOG.md`, `apps/chrome-extension/CHANGELOG.md`).
For `palette`/`android`/`chrome`, release-please owns changelog updates and
uses the native `changelog-sections` policy in `release-please-config.json`
to mirror Axon's old release-note shape: user-facing `feat`, `fix`, `perf`,
and `refactor` entries are shown; routine maintenance types are hidden.
Release PRs also carry release-please labels and a PR header/footer
explaining that CI may append derived-file fixups before merge. For `cli`,
`cargo xtask bump-version patch --component cli` inserts the dated heading itself (no
generated commit-message-derived body — write the entry by hand if you want
one beyond the heading).

Release tooling lives in the **`xtask-release`** package, not in `xtask`. It
depends on no axon crate, so CI jobs that only do release bookkeeping build
`-p xtask-release` in seconds instead of compiling the whole product (building
`xtask` pulls in twelve axon crates and, transitively, OpenSSL via
`axon-services -> git2`). `xtask` flattens the same command surface, so every
`cargo xtask <release-command>` invocation below is unchanged; the standalone
`xtask-release` binary accepts identical arguments and is what the release
workflows run.

It is a validation, manual-bump, and release-please postprocessing
helper: `check-release-versions` verifies component parity and changed
shipping paths, defers ordinary managed-app feature bumps to release-please,
and rejects mixed manual version edits (while skipping the
release-please-manifest check for `cli`, since it has no manifest entry to
check against). `bump-version` is the manual
writer for `cli`, `release-please-fixup-plan`/`release-please-fixups` handle
derived files release-please cannot update directly for
`palette`/`android`/`chrome`, and `release-please-dispatch-plan` translates
release-please outputs into artifact workflow dispatches. **Editing a
`CHANGELOG.md` never triggers a release** — change detection ignores it, so
documenting a release can't recursively cut another. `Cargo.lock` churn
confined to workspace members outside every shipping path (`xtask`,
`xtask-release`) does not require a CLI bump.

The PR gate is:

```bash
cargo xtask check-release-versions --base origin/main --head HEAD --mode pr
```

Short release checklist:

- **`palette`/`android`/`chrome`:**
  1. Let release-please open the release PR after green `CI` on `main`.
  2. Review that the release PR updates `.release-please-manifest.json`, component version, and changelog.
  3. Run `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`.
  4. Merge only after the release/version gate and CI are green.
- **`cli`:**
  1. Run `cargo xtask bump-version patch|minor|major --component cli` locally, picking the level yourself.
  2. Review the diff — every file listed above should move together, nothing else.
  3. Include the bump in your PR (or a dedicated bump PR) and run `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`.
  4. Merge; `.github/workflows/auto-tag.yml` detects the bumped-but-untagged Axon-native `cli` version, waits for exact-main CI, creates the `vX.Y.Z` tag and GitHub Release, then dispatches `release.yml` to attach artifacts.

The compatibility command `cargo xtask check-version-sync` still enforces
**CLI** version parity across `Cargo.toml`, `README.md`, `CHANGELOG.md`,
`apps/web/package.json`, and `apps/web/openapi/axon.json`, and checks that
`plugins/axon/.claude-plugin/plugin.json` has no `version` key. The full
multi-component gate is `cargo xtask check-release-versions`.
