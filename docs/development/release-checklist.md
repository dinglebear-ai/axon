---
title: "Release Checklist — Axon"
created: 2026-07-07
updated: 2026-08-03
---

# Release Checklist — Axon

Pre-release checklist for the current release-please-driven pipeline. See the
root `CLAUDE.md` "Release Pipeline" section for the full component/version
model this checklist enforces; this file is the short operational checklist
version of it.

Releases are per-component (`cli`, `palette`, `android`, `chrome`) and
selective. Release-please owns release PRs, version bumps, changelogs, tags,
and GitHub Releases for `palette`, `android`, and `chrome`. The CLI is the only
unmanaged component: its version is bumped with `xtask`, then auto-tag creates
its tag and GitHub Release after exact-main CI before dispatching artifacts.

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
| **cli** | `Cargo.toml` (`[package] version`), `README.md`, `CHANGELOG.md`, `apps/web/package.json`, `apps/web/openapi/axon.json` | `Cargo.toml` |
| **palette** | `apps/palette-tauri/src-tauri/tauri.conf.json`, `apps/palette-tauri/package.json`, `apps/palette-tauri/src-tauri/Cargo.toml` | `tauri.conf.json` |
| **android** | `apps/android/app/build.gradle.kts` (`versionName` + `versionCode`) | `build.gradle.kts` |
| **chrome** | `apps/chrome-extension/manifest.json` | `manifest.json` |

## Build and test

- [ ] `just verify` passes (fmt-check + clippy + check + test)
- [ ] `just precommit` passes (monolith check + verify)
- [ ] Web panel builds: `cd apps/web && npm run build`
- [ ] `axon doctor` reports all required services healthy (Qdrant, TEI)
- [ ] `cargo xtask check-layering` passes (no forbidden crate-dependency reaches)
- [ ] `cargo xtask check-no-mod-rs` passes (no `mod.rs` reintroduced)

## Security

- [ ] No credentials in code, docs, or git history
- [ ] `.gitignore`/`.dockerignore` include `.env`, `*.secret`, `.git/`
- [ ] Docker containers run as non-root (`user: "1000:1000"`)
- [ ] No baked environment variables in Docker images
- [ ] MCP/action auth uses `AXON_MCP_HTTP_TOKEN` or OAuth for non-loopback binds

See [`contributing.md`](contributing.md#security-guardrails) for the full
guardrail set.

## Infrastructure

- [ ] `docker-compose.prod.yaml` starts cleanly with `--env-file ~/.axon/.env`
      (Axon server, Qdrant, TEI, Chrome)
- [ ] `axon serve` starts and owns in-process crawl/embed/extract/ingest
      workers
- [ ] SQLite job/ledger migrations apply cleanly (`crates/axon-jobs/src/migrations`,
      `crates/axon-ledger/src/migrations`, `crates/axon-memory/src/migrations`)

## Documentation

- [ ] Root `CLAUDE.md` matches current architecture (crate layering, commands,
      env vars)
- [ ] `docs/reference/mcp/tool-schema.md` regenerated if the MCP tool surface
      changed
- [ ] New/changed CLI commands are reflected in
      `docs/pipeline-unification/surfaces/command-contract.md` and
      `docs/pipeline-unification/surfaces/axon-help.md` if they are covered
      by the pipeline-unification docs tree

## Monolith policy

- [ ] No changed `.rs` files exceed 500 lines (except allowlisted in
      `.monolith-allowlist`)
- [ ] No changed functions exceed 120 lines
- [ ] `python3 ~/.claude/hooks/enforce_monoliths.py --staged` passes locally

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

## Cutting a managed release (`palette`, `android`, `chrome`)

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
4. Confirm auto-tag selected only the unmanaged CLI, waited for exact-main CI,
   created the `vX.Y.Z` tag and GitHub Release, then dispatched `release.yml`.
5. Confirm the Linux and Windows assets and checksums were attached.

Direct tags for release-please-managed components are break-glass incident
operations. Do not use them as a normal hotfix path or create a second owner
for managed version files, tags, or GitHub Releases.
