---
date: 2026-05-04 16:11:22 EST
repo: git@github.com:jmagar/axon.git
branch: bd-1d2.3/ssh-remote-deployment
head: 8da9f0f1
agent: Codex
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust  8da9f0f1 [bd-1d2.3/ssh-remote-deployment]
pr: none; gh reported no pull requests found for branch "bd-1d2.3/ssh-remote-deployment"
---

# MCP Auth and ACP Quick Wins

## User Request

The session began with listing open Beads, then working through quick-win Lavra/Beads items. The final request was `quick-push`.

## Session Overview

- Closed a batch of quick-win Beads from the repo-wide review backlog.
- Implemented and closed `axon_rust-pkl.5` by adding direct MCP HTTP token middleware tests and updating the OAuth smoke script.
- Deleted fully-stacked local branches below the current branch after confirming branch ancestry.
- Committed and pushed the resulting work as `8da9f0f1 fix: harden MCP auth and ACP quick wins`.

## Sequence of Events

1. Listed open Beads and identified quick-win candidates.
2. Worked through and closed small documentation, config, validation, and test coverage Beads.
3. Selected `axon_rust-pkl.5` as the next candidate and implemented direct MCP HTTP auth middleware coverage.
4. Verified local branch stack ancestry and removed lower local feature branches.
5. Staged the full dirty worktree, bumped version metadata, committed, rebased, attempted Beads Dolt push, pushed git, and verified branch status.

## Key Findings

- The Beads Dolt backend has no remote configured; `bd dolt push` skipped with "No remote is configured".
- `docs/sessions/` is ignored in this repo, so this post-push session note is intentionally local unless force-added later.
- The current branch has no GitHub PR according to `gh pr view`.

## Technical Decisions

- MCP auth tests use a real Axum listener so the middleware is exercised directly rather than only through helper-level assertions.
- The smoke script now checks missing token rejection, invalid token rejection, bearer token acceptance, and `x-api-key` acceptance.
- Version was bumped from `1.3.0` to `1.3.1` because AGENTS.md requires a version bump for feature branch pushes.

## Files Modified

- `Cargo.toml`, `Cargo.lock`, `.claude-plugin/plugin.json`, `CHANGELOG.md`: version bump and changelog entry for `1.3.1`.
- `config/docker-compose.services.yaml`: loopback-bound exposed service ports.
- `crates/core/http/client.rs`: DNS rebinding comment clarification.
- `crates/core/paths.rs`: shared HOME validation and safer fallback data directory behavior.
- `crates/mcp/auth.rs`: direct token middleware coverage.
- `crates/mcp/server/handlers_acp.rs`: unsupported ACP subactions return MCP errors.
- `crates/services/acp/mapping/validation.rs`: path-style ACP adapter validation requires existing executable files.
- `crates/services/acp/persistent_conn/turn.rs`, `crates/services/acp/session.rs`, `docs/ACP.md`: ACP timeout/spec documentation updates.
- `docs/MCP.md`, `README.md`, `docs/TESTING.md`, `docs/repo/REPO.md`, `docs/repo/SCRIPTS.md`: documentation refreshes.
- `scripts/audit_compose_images.py`: new compose image audit helper.
- `scripts/test-mcp-oauth-protection.sh`: strengthened MCP token smoke coverage.
- `tests/services_acp_security.rs`: included in the final full-tree quick-push staging.

## Commands Executed

- `cargo check --locked`: passed before commit.
- `cargo test --locked mcp::auth -- --nocapture`: passed, 9 tests.
- `rustfmt --edition 2024 --check crates/mcp/auth.rs`: passed.
- `bash -n scripts/test-mcp-oauth-protection.sh`: passed.
- `git diff --check -- crates/mcp/auth.rs scripts/test-mcp-oauth-protection.sh`: passed.
- `scripts/audit_compose_images.py`: passed; Qdrant and TEI image checks were OK and Chrome was build-only.
- `python3 -m py_compile scripts/audit_compose_images.py`: passed.
- `docker compose -f config/docker-compose.services.yaml config`: passed and showed loopback host IPs.
- `git commit`: pre-commit hooks passed `mcp-http-only`, `no-mod-rs`, `unwrap-warn`, `env-guard`, `monolith`, `claude-symlinks`, `rustfmt`, `check`, `test`, and `clippy`.
- `git pull --rebase`: branch was up to date.
- `bd dolt push`: skipped because no Dolt remote is configured.
- `git push`: pushed `7d9dabff..8da9f0f1` to `origin/bd-1d2.3/ssh-remote-deployment`.

## Errors Encountered

- `bd dolt push` reported no remote configured and an auto-export `git add` warning. Git status after push remained clean and aligned with origin.

## Behavior Changes

- Before: MCP token middleware did not have direct end-to-end listener coverage in tests.
- After: Middleware behavior is covered for missing, invalid, bearer, and `x-api-key` token flows.
- Before: Compose service ports were public by default in the checked compose config.
- After: Checked compose config binds exposed ports to loopback.
- Before: ACP unsupported subactions could return an OK payload.
- After: Unsupported ACP subactions return MCP errors.

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `cargo check --locked` | Typecheck passes | Passed | Pass |
| `cargo test --locked mcp::auth -- --nocapture` | Auth tests pass | 9 passed | Pass |
| `bash -n scripts/test-mcp-oauth-protection.sh` | Script parses | Passed | Pass |
| `docker compose -f config/docker-compose.services.yaml config` | Compose resolves | Passed with loopback bindings | Pass |
| pre-commit `test` hook | Full test hook passes | 1435 lib tests passed, 5 ignored; integration/doctests passed in hook output | Pass |
| pre-commit `clippy` hook | Clippy passes | Passed | Pass |
| `git status --short --branch` | Clean and aligned with origin | `## bd-1d2.3/ssh-remote-deployment...origin/bd-1d2.3/ssh-remote-deployment` | Pass |

## Risks and Rollback

- The commit included the full staged dirty tree by user request. Roll back with `git revert 8da9f0f1` if the whole batch needs to be undone.
- The Beads Dolt store was not pushed to a remote because none is configured; issue state remains local and versioned in the repo.

## Open Questions

- Whether a Dolt remote should be configured for Beads synchronization.
- Whether to open a PR for `bd-1d2.3/ssh-remote-deployment`.

## Next Steps

- Highest-priority remaining candidates observed after the push include `axon_rust-1d2.3.1`, `axon_rust-1d2.3.2`, `axon_rust-1d2.3.3`, `axon_rust-pkl.2`, `axon_rust-pkl.3`, and `axon_rust-pkl.4`.
