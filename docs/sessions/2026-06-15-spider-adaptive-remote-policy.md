---
date: 2026-06-15 17:26:07 EDT
repo: git@github.com:jmagar/axon.git
worktree: /home/jmagar/workspace/axon/.worktrees/spider-adaptive-remote-policy
branch: codex/spider-adaptive-remote-policy
head_before_session_log: 706d64f2
pr: https://github.com/jmagar/axon/pull/225
plan: docs/superpowers/plans/2026-06-15-spider-adaptive-remote-policy.md
bead_epic: axon_rust-zoet
status: implemented, reviewed, verified, pushed
---

# Spider Adaptive Concurrency And Remote Policy

## Request

Create a Superpowers implementation plan for wiring Spider adaptive concurrency and Chrome `remote-local-policy` support into Axon, run Lavra engineering review against the plan, update the plan for all review feedback, then execute the work-it flow.

## Result

- Created and updated the implementation plan at `docs/superpowers/plans/2026-06-15-spider-adaptive-remote-policy.md`.
- Created bead epic `axon_rust-zoet` plus implementation and review-finding child beads.
- Implemented Spider adaptive crawl concurrency behind TOML-only `[workers.adaptive-concurrency]`.
- Implemented Chrome `remote-local-policy` behind TOML-only `[chrome] remote-local-policy`.
- Preserved default crawl behavior when both features are disabled.
- Opened PR #225: https://github.com/jmagar/axon/pull/225.

## Implementation Summary

- Added typed config, TOML parsing, validation, debug output, and config snapshot replay for adaptive concurrency and Chrome remote policy.
- Added `src/crawl/engine/adaptive.rs` for Axon-owned adaptive crawl control.
- Attached Spider's adaptive semaphore only when explicitly enabled.
- Recorded adaptive pressure from `429`, `5xx`, and crawl broadcast lag.
- Treated `2xx` as success and ordinary `3xx`/`4xx` as neutral.
- Drained surplus returned permits under sustained pressure so shrink converges.
- Included adaptive telemetry in final crawl summaries and live progress summaries.
- Wired Chrome intercept config through both the normal Chrome crawl path and thin-page Chrome refetch.
- Carried Axon's SSRF blacklist patterns into Spider's Chrome intercept config.
- Treated Docker service Chrome endpoints as process-local during config snapshot replay so queued jobs fall back to the worker's current endpoint.
- Updated operator docs in `config.example.toml`, `CLAUDE.md`, `docs/guides/configuration.md`, `docs/operations/performance.md`, and `docs/reference/spider-feature-flags.md`.

## Reviews

The initial Lavra engineering review asked to narrow the plan and avoid misleading knobs. The plan was updated to remove CLI, palette, `.env`, screenshot, arbitrary `decrease-factor`, and sync-interval scope.

Implementation review found and fixed:

- Chrome intercept remote policy did not carry the SSRF blacklist.
- Docker Chrome endpoint snapshots could replay container-only hostnames on incompatible workers.
- Adaptive shrink needed to drain returned surplus permits under repeated pressure.
- Ordinary non-pressure statuses needed to be neutral instead of success.
- Thin-page Chrome refetch needed to use the same remote policy and SSRF blacklist wiring.

Follow-up review and work-it simplifier passes found no remaining actionable blockers.

## Verification

Focused checks passed:

```bash
AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test crawl::engine::adaptive -- --nocapture
AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test chrome_remote_local_policy -- --nocapture
AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test endpoint_snapshot -- --nocapture
AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo check --all-targets
cargo fmt --check
git diff --check
```

The branch also passed local hooks and pre-push verification, including clippy, version-sync, and nextest:

```text
3155 tests run, 3155 passed, 6 skipped
```

Expected local warning during Rust checks:

```text
apps/web/out is not a complete web build; embedding fallback web panel
```

Checks were run with `AXON_ALLOW_FALLBACK_WEB_ASSETS=1`.

## Closeout

- PR head before this session-log commit: `706d64f2`.
- Bead epic `axon_rust-zoet` closed with all 10 child beads closed.
- GitHub PR comments at closeout were bot/status comments only; no actionable PR review findings were present.
- GitHub CI was still finishing several long-running jobs when checked after the implementation push; completed checks were green at that point.
