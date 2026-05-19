# Session: GitHub Clone Performance + Progress Display + Qdrant Tuning

**Date:** 2026-03-10
**Branch:** `feat/github-code-aware-chunking`
**Version:** 0.14.0 → 0.14.1

## Session Overview

Continued the `feat/github-code-aware-chunking` feature branch. Major work: (1) committed documentation updates for the feature, (2) added live progress tracking for GitHub ingest jobs, (3) replaced catastrophically slow per-file HTTP API fetches with `git clone --depth=1`, (4) fixed progress display for task-level and phase-level states, (5) added Qdrant on-disk tuning config and memory limits, (6) cleaned up `ssh_auth.rs` tests. Investigated but did not resolve the Tailscale dual-auth WebSocket denial for localhost connections.

## Timeline

1. **Documentation commit** (`d29b1f4a`) — 13 files: CLAUDE.md, crates/core/CLAUDE.md, crates/ingest/CLAUDE.md, crates/vector/CLAUDE.md, docs/SCHEMA.md, docs/SERVE.md, docs/WS-PROTOCOL.md, docs/commands/{github,ingest,refresh}.md, docs/ingest/{github,ingest}.md, .env.example
2. **Progress tracking** (`fa11b4a3`) — `UnboundedSender<serde_json::Value>` channel wired from `embed_files()` through `ingest_github()` to `process.rs` DB writer task; `ingest_progress()` in `ingest_common.rs` extended for GitHub file/task/phase states
3. **Git clone rewrite** (`17782382`) — Complete rewrite of `crates/ingest/github/files.rs`: removed all HTTP API fetching code, added `clone_repo()` (git clone --depth=1), `collect_indexable_files()` (recursive walk), `read_and_embed_file()` (disk read + embed). Auth via `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_0`/`GIT_CONFIG_VALUE_0` env vars.
4. **Progress display fix** (`81e6a874`) — Added task-level (`tasks_done/tasks_total`) and phase (`enumerating`) progress rendering to both `ingest_metrics_suffix()` in `status/metrics.rs` and `ingest_progress()` in `ingest_common.rs`
5. **Qdrant tuning + push** (`0c8f2b57`) — Added `docker/qdrant/production.yaml` with on-disk storage config, docker-compose memory limits, ssh_auth test cleanup, version bump to 0.14.1

## Key Findings

- **Per-file HTTP fetching was the bottleneck**: biomejs/biome (10,477 files) took 30+ minutes via individual `raw.githubusercontent.com` requests. `git clone --depth=1` gets all files in seconds.
- **Progress display had three phases**: (1) initial `{"phase": "ingesting", "tasks_total": 5, "tasks_done": 0}`, (2) file-level `{"files_done": N, "files_total": M, "chunks_embedded": K}`, (3) final completion. Display functions initially only checked for phase 2.
- **Tailscale dual-auth blocks localhost**: `AXON_REQUIRE_DUAL_AUTH=true` (default) requires `Tailscale-User-Login` header, which is only injected by `tailscale serve` reverse proxy. Direct localhost connections from Next.js dev server or browser lack this header. The `/shell` endpoint already has a loopback bypass (`shell_ws_upgrade` at `crates/web.rs:394-403`) but the main `/ws` endpoint does not.

## Technical Decisions

- **git clone over API**: Single subprocess call vs 10K+ HTTP requests. Auth token passed via git config env vars (never in process args) for security.
- **UnboundedSender for progress**: Chosen over bounded channel because progress updates are lightweight JSON values and backpressure would slow the embed pipeline unnecessarily. The receiver spawns a dedicated DB writer task.
- **`progress_tx: Option<&UnboundedSender>`**: Passed as reference, not owned — the sender is created in `process.rs` and the receiver task owns the other end. CLI sync path passes `None`.
- **Qdrant on-disk config**: `on_disk_payload: true`, `on_disk: true` for vectors and HNSW, `memmap_threshold_kb: 20000` — reduces RAM for the 2.57M point `cortex` collection.

## Files Modified

| File | Purpose |
|------|---------|
| `crates/ingest/github/files.rs` | **Complete rewrite** — git clone replaces HTTP API fetches |
| `crates/ingest/github.rs` | Added `progress_tx` parameter to `ingest_github()` |
| `crates/jobs/ingest/process.rs` | Channel creation + DB writer task for GitHub progress; renamed `update_playlist_progress_with_pool` → `update_ingest_progress` |
| `crates/services/ingest.rs` | Pass `None` for progress_tx in CLI sync path |
| `crates/cli/commands/ingest_common.rs` | Renamed `playlist_progress` → `ingest_progress`; added GitHub file/task/phase handling |
| `crates/cli/commands/status/metrics.rs` | Added task-level and phase progress to `ingest_metrics_suffix()` |
| `docker/qdrant/production.yaml` | **New** — Qdrant on-disk storage config |
| `docker-compose.yaml` | Qdrant config mount + memory limits (1G-4G) |
| `crates/web/ssh_auth.rs` | Test cleanup — `base64_encode` moved inside test module |
| `Cargo.toml` | Version 0.14.0 → 0.14.1 |
| `CHANGELOG.md` | Updated with 10 new commit entries |

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| GitHub ingest speed (10K files) | 30+ minutes (per-file HTTP) | Seconds (git clone) |
| `axon ingest list` (GitHub running) | No progress shown | Shows `files_done/files_total` or `phase (tasks_done/tasks_total)` |
| `axon status` (GitHub running) | No progress shown | Shows file/task/phase metrics with accent styling |
| Qdrant memory | Unbounded (all in RAM) | 1G reserved, 4G limit, on-disk payload/vectors/HNSW |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Compiles | Compiled in 18s | PASS |
| `cargo test` (pre-commit) | All pass | 1130 tests passed | PASS |
| `git push` | Pushed | 69f673c0..0c8f2b57 pushed | PASS |

## Risks and Rollback

- **git clone requires `git` in PATH/container** — Docker image already has git installed (added for wiki clone). If git is missing, `clone_repo()` returns a clear error.
- **Qdrant config change** — `production.yaml` only applies on container restart. Rollback: remove the volume mount line from docker-compose.yaml.
- **Rollback path**: `git revert 0c8f2b57` for this commit; `git revert 17782382` for the git clone rewrite specifically.

## Decisions Not Taken

- **Tailscale dual-auth loopback bypass**: Investigated but not implemented. The fix would add loopback detection to `ws_upgrade` (similar to `shell_ws_upgrade`) and override `require_dual_auth=false` for localhost. Deferred to avoid scope creep in a chore commit.
- **Bounded channel for progress**: Considered `channel(64)` per project convention, but progress updates are tiny JSON values emitted at file-completion rate — backpressure would slow embedding for no benefit.

## Open Questions

- **Tailscale dual-auth for localhost**: Should the main `/ws` endpoint get the same loopback bypass as `/shell`? The security model says localhost-only binding is the trust boundary — loopback connections are inherently local. But this changes the dual-auth invariant.
- **GitHub ingest resume**: The git clone approach doesn't support resume (clone is all-or-nothing). If a large repo clone fails mid-way, the entire clone restarts. For repos with 100K+ files this could matter.

## Next Steps

1. Fix Tailscale dual-auth WebSocket denial for localhost connections
2. Run end-to-end GitHub ingest test with new binary to verify progress display
3. Final integration check (Task 14 from feature plan)
4. `superpowers:finishing-a-development-branch` after all tasks complete
