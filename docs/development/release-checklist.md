# Release Checklist — Axon

Pre-release checklist for the current release-please-driven pipeline. See the
root `CLAUDE.md` "Release Pipeline" section for the full component/version
model this checklist enforces; this file is the short operational checklist
version of it.

Releases are per-component (`cli`, `palette`, `android`, `chrome`) and
selective — release-please owns release PRs, version bumps, changelogs, tags,
and GitHub Release records after `CI` is green on `main`. `xtask` only
validates and postprocesses; it does not cut releases itself.

## Before merging a change that ships in a release

- [ ] Component version-bearing files are in sync for every component whose
      shipping paths changed (see the table below).
- [ ] `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`
      passes — this is the PR gate and fails with the affected component name
      if code changed but the version did not move.
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

## Cutting the release

1. Let release-please open the release PR after green `CI` on `main`.
2. Review that the release PR updates `.release-please-manifest.json`,
   component versions, and changelogs correctly.
3. Run `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`.
4. Merge only after the release/version gate and CI are green.
5. Confirm the per-component artifact workflow (`release.yml`,
   `palette-release.yml`, `android-release.yml`,
   `chrome-extension-release.yml`) ran and attached signed/checksummed
   assets to the GitHub Release release-please created.

To cut a release manually (hotfix/re-release without a code change), push the
component's tag directly instead of waiting on release-please:

```bash
git tag vX.Y.Z             && git push origin vX.Y.Z              # cli
git tag palette-vX.Y.Z     && git push origin palette-vX.Y.Z      # palette
git tag android-vX.Y.Z     && git push origin android-vX.Y.Z      # android
git tag chrome-ext-vX.Y.Z  && git push origin chrome-ext-vX.Y.Z   # chrome
```
