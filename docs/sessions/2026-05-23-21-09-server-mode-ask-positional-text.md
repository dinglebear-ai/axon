---
date: 2026-05-23 21:09:08 EDT
repo: git@github.com:jmagar/axon.git
branch: main
head: 6dbb46a7
agent: Codex
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust
beads: none
---

# Server-Mode Ask Positional Text Fix

## User Request

The user showed repeated failures from `axon ask "tell me everything you know about mcp primitive prompts"` returning `Error: ServerPlanError("ask requires text")`, then asked to execute systematic debugging.

## Session Overview

Diagnosed the failure as a CLI server-mode planning bug: local `ask` accepted positional text, but server-mode planning only read `cfg.query`. Fixed the planner to use the shared input resolver, added regression coverage, rebuilt the release binary, and verified the original command shape from `/home/jmagar/workspace/syslog-mcp`.

## Sequence of Events

1. Loaded the systematic debugging skill and checked the current repo, installed binary, and relevant `ask`/server-mode code.
2. Reproduced the failure with `AXON_LOG=debug axon ask ... --json`, confirming it failed before HTTP execution with `ServerPlanError("ask requires text")`.
3. Compared local `run_ask()` input resolution against server-mode `query_text()` and identified the divergence.
4. Patched `src/cli/server_mode/plan.rs` to call `cli::commands::resolve_input_text(cfg)`.
5. Added `ask_server_mode_accepts_positional_text` to `src/cli/server_mode_tests.rs`.
6. Ran targeted tests, rebuilt `target/release/axon`, and verified `axon ask ... --no-stream` returned a real answer.

## Key Findings

- Local CLI command handlers use `resolve_input_text()` to prefer `--query` and fall back to joined positional words.
- Server-mode planning used a narrower helper that only checked `cfg.query`, so ordinary quoted positional text was treated as missing.
- The installed `axon` on PATH was `/home/jmagar/.local/bin/axon`, a symlink to `/home/jmagar/workspace/axon_rust/target/release/axon`.
- The REST `/v1/ask` body contract already expects `query`, so the server endpoint was not the failing layer.
- Relevant code locations:
  - `src/cli/server_mode/plan.rs:343`
  - `src/cli/server_mode_tests.rs:242`
  - `src/cli/commands.rs:94`
  - `src/web/server/types.rs:92`

## Technical Decisions

- Reused `resolve_input_text()` instead of duplicating fallback logic in server mode.
- Kept `--query` behavior unchanged because the shared resolver still prefers `cfg.query`.
- Added a focused unit regression test at the server-mode planner boundary because the failure happened before network I/O.
- Rebuilt release after tests because the shell binary resolves to this repo's release artifact.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `src/cli/server_mode/plan.rs` | | Use shared text resolver for server-mode query-like commands | `query_text()` now calls `cli::commands::resolve_input_text(cfg)` |
| modified | `src/cli/server_mode_tests.rs` | | Pin positional `ask` text in server-mode planning | Added `ask_server_mode_accepts_positional_text` |
| created | `docs/sessions/2026-05-23-server-mode-ask-positional-text.md` | | Save this debugging session | This file |

## Beads Activity

No bead activity observed. `bd list --all --sort updated --reverse --limit 20 --json` returned recent closed historical issues, but none were directly relevant to this narrow `ask` parser regression. No beads were created, edited, claimed, or closed.

## Repository Maintenance

- Plans: inspected `docs/plans`; no clearly completed active plan related to this session was found, so no plan files were moved.
- Beads: inspected recent tracker state; no directly relevant open bead was observed, so no tracker mutation was made.
- Worktrees and branches: `git worktree list --porcelain` showed only `/home/jmagar/workspace/axon_rust` on `main`; local and remote branch listings showed `main` aligned with `origin/main`, so no branch or worktree cleanup was needed.
- Stale docs: searched docs for `axon ask`, `/v1/ask`, and server-mode mentions. No doc was contradicted by this fix; `docs/ASK.md` already documents positional `axon ask "<question>"` and server-mode behavior.
- Skipped cleanup: left `?? apps/palette-tauri/` untouched because it was unrelated to this session.

## Tools and Skills Used

- Skills: `systematic-debugging` for root-cause-first debugging; `save-to-md` for this session capture.
- Shell commands: `rg`, `sed`, `nl`, `git`, `cargo`, `axon`, `bd`, `gh`, `ps`, `ls`, `date`.
- File edits: `apply_patch` only.
- MCP/app tools: none used for the implementation.
- Subagents: none used.
- Issues encountered: one Cargo invocation used two test filters and failed because Cargo accepts only one positional test filter per run; rerun as separate commands.

## Commands Executed

| command | result |
|---|---|
| `rg -n "ask requires text\|ServerPlanError\|..." src tests Cargo.toml` | Found relevant ask/server-mode surfaces; one broad search also warned that `crates` did not exist |
| `which axon && axon --version` | Confirmed `/home/jmagar/.local/bin/axon`, version `axon 4.4.2` |
| `AXON_LOG=debug axon ask "...prompts" --json` | Reproduced `Error: ServerPlanError("ask requires text")` |
| `cargo test ask_server_mode_accepts_positional_text` | Passed |
| `cargo test query_server_mode_uses_direct_rest_path` | Passed |
| `cargo fmt --check` | Passed |
| `cargo build --release --bin axon` | Passed; rebuilt release artifact used by PATH symlink |
| `timeout 90s axon ask "tell me everything you know about mcp primitive prompts" --no-stream` | Passed; returned an answer with sources and timing |

## Errors Encountered

- `axon ask` failed with `ServerPlanError("ask requires text")`.
  - Root cause: server-mode `query_text()` ignored `cfg.positional`.
  - Resolution: use shared `resolve_input_text()` in server-mode planning.
- `cargo test ask_server_mode_accepts_positional_text query_server_mode_uses_direct_rest_path` failed with `unexpected argument`.
  - Root cause: Cargo accepts one positional test filter.
  - Resolution: ran each test filter separately.
- `sed -n '1,260p' src/core/config/parse/args.rs` failed because that path does not exist.
  - Resolution: located the current parse and dispatch files with `rg`.

## Behavior Changes

| before | after |
|---|---|
| `axon ask "tell me everything..."` in server mode failed during planning with `ask requires text` | Positional ask text is accepted and sent to `/v1/ask` as `query` |
| Server-mode query-like commands had narrower text resolution than local commands | Server-mode ask/query/search/research/evaluate planning now uses the same shared text resolver |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test ask_server_mode_accepts_positional_text` | Regression test passes | `1 passed` | pass |
| `cargo test query_server_mode_uses_direct_rest_path` | Existing query server-mode test remains green | `1 passed` | pass |
| `cargo fmt --check` | Formatting unchanged/valid | exited 0 | pass |
| `cargo build --release --bin axon` | Release binary rebuilds | finished release build | pass |
| `timeout 90s axon ask "tell me everything you know about mcp primitive prompts" --no-stream` | No `ServerPlanError`; returns answer | returned JSON answer with sources | pass |

## Risks and Rollback

- Risk is low: the change centralizes existing local CLI text resolution behavior into server-mode planning.
- Rollback path: revert the change in `src/cli/server_mode/plan.rs` and remove `ask_server_mode_accepts_positional_text` from `src/cli/server_mode_tests.rs`.

## Decisions Not Taken

- Did not change `/v1/ask` request parsing because the endpoint already accepts `query`.
- Did not add a new user-facing flag or workaround because the quoted positional form is already documented and should work.
- Did not touch unrelated committed docs in the current HEAD or untracked `apps/palette-tauri/`.

## References

- `docs/ASK.md`
- `docs/specs/server-mode-routing-contract.md`
- `src/cli/commands.rs`
- `src/cli/server_mode/plan.rs`
- `src/web/server/handlers/ask.rs`

## Open Questions

- The current `HEAD` commit includes additional documentation files (`CLAUDE.md`, `src/web/AGENTS.md`, `src/web/CLAUDE.md`, `src/web/GEMINI.md`) beyond the two server-mode code files. They were observed in `git show HEAD`, but were not edited during this session.

## Next Steps

- Commit/push state already appears aligned with `origin/main` at `6dbb46a7`.
- If this change is promoted through CI, run the broader server-mode test set or `just verify` as the next confidence step.
- Decide separately what to do with the unrelated untracked `apps/palette-tauri/` directory.
