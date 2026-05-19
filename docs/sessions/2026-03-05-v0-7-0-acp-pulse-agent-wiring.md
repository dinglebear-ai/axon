# Session Capture — v0.7.0 ACP Pulse Agent Wiring
Date: 2026-03-05
Branch: feat/services-layer-refactor
Repository: git@github.com:jmagar/axon_rust.git

## Summary
- Completed safe stage/commit/push workflow for current branch state.
- Bumped Rust package version from `0.6.0` to `0.7.0` in `Cargo.toml` and refreshed lock metadata via `cargo check`.
- Updated `CHANGELOG.md` with undocumented commits since `4e5144a3` and added current release entry.
- Landed frontend-to-backend Pulse agent selection plumbing for `claude` and `codex` plus ACP adapter routing and tests.

## Commit
- SHA: `de90c3379f2278ba074a000b3cf209a7e3038bf3`
- Message: `feat(release): v0.7.0 — ACP pulse agent routing, frontend wiring, and scrape/embed hardening`
- Co-author trailer included: `Co-authored-by: Claude <noreply@anthropic.com>`
- Files changed in commit: 82

## Verification Run During Commit Hooks
- `cargo check --all-targets --locked` passed.
- `cargo clippy --all-targets --locked -- -D warnings` passed.
- `cargo test --all --locked` passed in pre-commit hook run.
- Biome completed with warning-only diagnostics (no blocking errors).
- Monolith guard passed after allowlist updates.

## Notable Technical Changes
- Pulse agent selector (`claude`/`codex`) persisted through web workspace state.
- `/api/pulse/chat` forwards `agent` through WS flags.
- `crates/web/execute/sync_mode.rs` resolves per-agent ACP adapter env overrides:
  - `AXON_ACP_CLAUDE_ADAPTER_CMD`, `AXON_ACP_CLAUDE_ADAPTER_ARGS`
  - `AXON_ACP_CODEX_ADAPTER_CMD`, `AXON_ACP_CODEX_ADAPTER_ARGS`
  - fallback to `AXON_ACP_ADAPTER_CMD`, `AXON_ACP_ADAPTER_ARGS`
- Replay cache key now includes agent to avoid cross-agent replay collisions.

## Release Metadata
- Version bump: `0.6.0 -> 0.7.0` (minor; `feat`)
- Push destination: `origin feat/services-layer-refactor`

## Axon/Neo4j Capture
- This session document is intended to be embedded via Axon and stored in Neo4j memory with commit/repository/session relations.
