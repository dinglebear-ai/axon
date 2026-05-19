# Session: GitHub Issues + Auth Hardening + Pulse Workspace + v0.14.0 Release
**Date**: 2026-03-10
**Branch**: `feat/github-code-aware-chunking`
**Commit**: `69f673c0`
**Version**: `0.13.2` → `0.14.0` (minor bump)

## Session Overview

Two-phase session: (1) authored 10 GitHub issues (#29 update, #32–41) covering ingest progress, metadata audit, sessions revival, reranker, GraphRAG, OpenAI endpoints, ACP settings, scheduled refresh, remote ACP, and refresh+ingest unification; (2) committed and pushed a large changeset covering SSH key auth, dual-auth mode, Pulse workspace panes, and CLI cleanup as v0.14.0.

## Timeline

1. **Issue authoring** (continued from prior context) — created issues #38 (ACP settings page), #39 (scheduled refresh), #40 (remote ACP via thin client) with parallel haiku explore agents for codebase research
2. **Issue #41** — `feat(refresh): extend scheduled refresh to support Reddit/YouTube/GitHub re-ingestion` — dispatched two parallel haiku explore agents to map refresh system (`crates/jobs/refresh/`) and ingest system (`crates/jobs/ingest.rs`, `crates/ingest/`)
3. **Version bump** — `0.13.2` → `0.14.0` in `Cargo.toml`, `cargo check` to update `Cargo.lock`
4. **Changelog update** — added v0.14.0 highlight block + 5 new commit rows
5. **Pre-commit fixes** — biome (unused `RightPanelId` import in `pulse-mobile-pane-switcher.tsx`), clippy (empty line after doc comment in `classify.rs`, unnecessary `axum::http::HeaderMap` qualification in `ssh_auth.rs`, `base64_encode` after test module in `ssh_auth.rs`)
6. **Push** — `feat/github-code-aware-chunking` branch pushed to origin

## Key Findings

- **Refresh system is URL-only**: `RefreshJobConfig` holds `urls: Vec<String>`, `url_processor.rs` fetches via HTTP with conditional headers (ETag/Last-Modified). No support for ingest source re-ingestion — issue #41 addresses this gap.
- **Ingest dispatch in `process_ingest_job()`** (`crates/jobs/ingest.rs:288-314`): matches on `IngestSource` enum variants, dispatches to source-specific handlers. YouTube playlists get special `ingest_youtube_playlist_with_pool` path.
- **`RefreshSchedule` table** has `seed_url`/`urls_json` — issue #41 proposes adding `ingest_source` JSONB + `cursor_json` for delta-aware re-ingestion.
- **`classify_target()`** (`crates/ingest/classify.rs`) auto-detects Reddit/YouTube/GitHub from input string — reusable for `--ingest-target` flag on refresh schedules.

## Technical Decisions

- **Minor version bump** (not patch): SSH auth + dual-auth + Pulse panes are new features, not just fixes
- **Issue #41 proposes `ingest_source` column on `axon_refresh_schedules`** rather than a separate ingest scheduling table — reuses existing schedule infrastructure (claim, tick, worker)
- **Delta cursors as freeform JSONB** (`cursor_json`) rather than typed columns — each source has different cursor semantics (Reddit post ID, YouTube date, GitHub commit SHA)

## Files Modified

| File | Purpose |
|------|---------|
| `Cargo.toml` | Version bump `0.13.2` → `0.14.0` |
| `CHANGELOG.md` | v0.14.0 highlight + commit rows |
| `apps/web/components/pulse/pulse-mobile-pane-switcher.tsx` | Remove unused `RightPanelId` import |
| `crates/vector/ops/input/classify.rs` | Remove empty line after doc comment (clippy) |
| `crates/web/ssh_auth.rs` | Fix unnecessary qualifications + move `base64_encode` into test module |
| 34 files total in commit | Auth, Pulse, CLI cleanup (see `git diff --stat`) |

## Commands Executed

| Command | Result |
|---------|--------|
| `cargo check` | Compiled `axon v0.14.0` — Cargo.lock updated |
| `git push -u origin feat/github-code-aware-chunking` | New branch pushed successfully |
| `gh issue create` (#41) | https://github.com/jmagar/axon/issues/41 |

## Behavior Changes

| Before | After |
|--------|-------|
| `axon refresh` only supports URL re-crawling | Issue #41 proposes extending to Reddit/YouTube/GitHub re-ingestion (not yet implemented) |
| Version `0.13.2` | Version `0.14.0` |
| `ssh_auth.rs` had items after test module | `base64_encode` moved inside `mod tests` |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Compiles clean | `Finished dev profile` | PASS |
| Pre-commit hooks | All pass | All pass (biome, clippy, rustfmt, monolith) | PASS |
| `git push` | Branch pushed | New branch created on origin | PASS |

## GitHub Issues Created This Session

| Issue | Title |
|-------|-------|
| #38 | ACP settings page in Reboot UI |
| #39 | Scheduled `axon refresh` for keeping docs fresh |
| #40 | Remote ACP via thin client |
| #41 | Extend scheduled refresh to support Reddit/YouTube/GitHub re-ingestion |

## Complete Issue Registry (Full Session, Including Prior Context)

#29 (updated), #32, #33, #34, #35, #36, #37, #38, #39, #40, #41

## Risks and Rollback

- **Low risk**: This commit is on a feature branch, not main. Rollback: `git reset --hard 0401eaa0` on the branch.
- **Pre-commit clippy fixes** are trivial (import cleanup, code movement) — no logic changes.

## Decisions Not Taken

- **Separate ingest scheduler table**: Rejected — reusing `axon_refresh_schedules` with an `ingest_source` column avoids a new worker and duplicated schedule infrastructure.
- **Typed cursor columns per source**: Rejected — freeform JSONB `cursor_json` avoids coupling refresh schema to ingest internals.

## Open Questions

- How to handle delta detection for YouTube channels that add videos mid-playlist? (yt-dlp `--dateafter` vs Qdrant `yt_video_id` diffing)
- Should `--auto-refresh` on `axon ingest` auto-create a schedule? (deferred to follow-up)

## Next Steps

- No pending tasks from this session — all requested issues created and code pushed
- Issue #41 implementation is the next logical step for the refresh+ingest unification
