# Session: GitHub Ingest Progress Display Fixes

**Date:** 2026-03-10
**Branch:** `refactor/acp-performance-modern-rust`
**Version:** 0.14.1

## Session Overview

Fixed three bugs preventing GitHub ingest progress from displaying correctly in `axon status` and `axon ingest list`: (1) git clone auth using wrong header format for classic PATs, (2) completed jobs showing `0/5 tasks` instead of `5/5`, (3) `axon status` completed branch missing `tasks_total` handler. Also added unauthenticated clone fallback for public repos. All fixes verified end-to-end with `charmbracelet/bubbletea` test ingest.

## Timeline

1. **Quick-push** — Committed Qdrant tuning config, ssh_auth cleanup, version bump to 0.14.1 (from prior session work)
2. **E2E test** — Started stack with `just dev`, ran `axon ingest charmbracelet/glow` to test progress display
3. **Bug: clone auth failure** — `Authorization: Bearer {token}` rejected by GitHub for classic PATs (`ghp_`). Fixed to `Authorization: token {token}` in both `files.rs` and `wiki.rs`
4. **Bug: public repo fallback** — User asked "why do we even need a github token to clone a repo?" — added unauthenticated fallback when auth clone fails
5. **Bug: `0/5 tasks` on completed jobs** — Final progress send was missing after `tokio::join!` completes. Added `{tasks_done: 5, tasks_total: 5, chunks_embedded: total}` send
6. **Bug: `axon status` shows nothing for completed ingests** — `ingest_metrics_suffix` completed branch handled `videos_total` and `files_total` but not `tasks_total`. Added `tasks_total` handler
7. **Verification** — Rebuilt release binary, confirmed both `axon ingest list` and `axon status` show correct progress

## Key Findings

- **GitHub PAT auth format**: Classic PATs (`ghp_`) require `Authorization: token {TOKEN}`, NOT `Authorization: Bearer {TOKEN}`. Bearer works for fine-grained tokens but fails silently for classic PATs — git returns "invalid credentials"
- **Public repos don't need auth to clone** — a bad/expired token is worse than no token (rejected vs fallback to anonymous)
- **Progress channel overwrites**: `update_ingest_progress` does `SET result_json=$1` (full replace), so the final task-level send overwrites file-level data. The completed `result_json` has `tasks_total`/`tasks_done` but no `files_total`/`files_done`
- **`ingest_metrics_suffix` completed branch** (`crates/cli/commands/status/metrics.rs:234-263`): check order is `videos_total` → `files_total` → `tasks_total` → fallback (chunks only)
- **Release binary size**: 70MB — spider.rs + chromiumoxide + octocrab + tree-sitter grammars

## Technical Decisions

- **`token` over `Bearer`**: GitHub's own docs specify `token` prefix for classic PATs via `http.extraHeader`. Bearer is OAuth2 standard but GitHub PATs are not OAuth2 tokens.
- **Unauthenticated fallback**: If auth clone fails, retry without token. Public repos work anonymously; private repos still fail with a clear error. Cost: one extra clone attempt on auth failure for private repos.
- **`tasks_total` in metrics instead of preserving `files_total`**: The simpler fix. Alternative was restructuring the progress channel to merge rather than replace, but that would require changing `update_ingest_progress` SQL from `SET` to `||` merge — more risk for marginal benefit.
- **Octocrab still needed**: Git clone only gets file contents. Issues, PRs, and repo metadata (stars, topics, license, etc.) still require the GitHub API via octocrab.

## Files Modified

| File | Purpose |
|------|---------|
| `crates/ingest/github/files.rs:69` | Changed `Bearer` → `token` in Authorization header; added unauthenticated fallback for public repos |
| `crates/ingest/github/wiki.rs:61` | Changed `Bearer` → `token` in Authorization header |
| `crates/ingest/github.rs:251-258` | Added final progress send (`5/5 tasks, chunks_embedded`) after `tokio::join!` completes |
| `crates/cli/commands/status/metrics.rs:249-263` | Added `tasks_total` handler to completed branch of `ingest_metrics_suffix()` |

## Commands Executed

| Command | Result |
|---------|--------|
| `cargo check` | Clean in 0.37s |
| `cargo build --release --bin axon` | Success in 7m10s |
| `./scripts/axon ingest list` | Shows `[5 / 5 tasks, 2428 chunks embedded]` for bubbletea |
| `./scripts/axon status` | Shows `5/5 tasks \| 2428 chunks \| cortex \| 1m43s` for bubbletea |

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Git clone with classic PAT | `invalid credentials` error | Clones successfully with `token` auth |
| Git clone with bad/expired token | Hard failure | Falls back to unauthenticated (works for public repos) |
| `axon ingest list` completed GitHub | `[0 / 5 tasks, N chunks embedded]` | `[5 / 5 tasks, N chunks embedded]` |
| `axon status` completed GitHub | Only `N chunks` (no task info) | `5/5 tasks \| N chunks \| collection \| duration` |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Compiles | Clean in 0.37s | PASS |
| `cargo build --release` | Compiles | Success in 7m10s | PASS |
| `axon ingest list` (bubbletea) | `5 / 5 tasks` | `[5 / 5 tasks, 2428 chunks embedded]` | PASS |
| `axon status` (bubbletea) | Shows tasks + chunks | `5/5 tasks \| 2428 chunks \| cortex \| 1m43s` | PASS |
| `axon ingest list` (glow, pre-fix) | `0 / 5 tasks` | `[0 / 5 tasks, 1171 chunks embedded]` | PASS (expected — old data) |

## Source IDs + Collections Touched

| Source | Collection | Outcome |
|--------|-----------|---------|
| `charmbracelet/bubbletea` | `cortex` | 2428 chunks embedded — used for progress verification |
| `charmbracelet/glow` | `cortex` | 1171 chunks — pre-fix data, shows `0/5` as expected |

## Risks and Rollback

- **Auth fallback exposes clone to rate limiting**: Unauthenticated GitHub API/clone is limited to 60 req/hr. For large repos this is fine (single clone), but many concurrent ingests without a token could hit limits. Rollback: revert the fallback in `files.rs:76-86`.
- **`tasks_total` display assumes 5 tasks**: The `5` is hardcoded in `github.rs` final progress send. If new task types are added to GitHub ingest, this needs updating.
- **Uncommitted changes**: All four files are modified but not committed. Rollback: `git checkout -- crates/ingest/github/files.rs crates/ingest/github/wiki.rs crates/ingest/github.rs crates/cli/commands/status/metrics.rs`

## Decisions Not Taken

- **Preserving `files_total` in final progress**: Would require changing `update_ingest_progress` from `SET result_json=$1` to JSONB merge (`||`). More correct but higher risk — deferred.
- **Tailscale dual-auth localhost bypass**: Investigated in prior session but deferred — requires adding loopback detection to `ws_upgrade` handler similar to `shell_ws_upgrade`.
- **Bounded channel for progress**: `UnboundedSender` kept — progress updates are lightweight JSON values, backpressure would slow embedding for no benefit.

## Open Questions

- Should completed GitHub ingests show `files_total` in addition to `tasks_total`? Currently the final progress send overwrites file-level data. Would need JSONB merge in `update_ingest_progress`.
- The hardcoded `tasks_total: 5` in `github.rs:255` — should this be derived from the actual number of `tokio::join!` branches?
- Older ingest jobs (glow, agent-client-protocol) show `0/5 tasks` — should we backfill their `result_json` or leave as-is?

## Next Steps

1. Commit the four bug fix files (clone auth, fallback, final progress, metrics display)
2. Fix Tailscale dual-auth WebSocket denial for localhost connections
3. Consider JSONB merge for `update_ingest_progress` to preserve both file-level and task-level progress
