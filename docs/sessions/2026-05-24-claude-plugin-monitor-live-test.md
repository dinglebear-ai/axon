---
date: 2026-05-24 16:56:08 EDT
repo: git@github.com:jmagar/axon.git
branch: main
head: 5a55276a
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust
---

# Claude Plugin Monitor Live Test

## User Request

The user asked to set up a Claude Code plugin monitor for Axon job lifecycle activity, specifically starts, failures, completions, and embedding activity. After an initial command-level test, the user required a real live test and then asked to address three PR-review findings.

## Session Overview

Implemented and live-tested `axon monitor jobs`, added a Claude plugin monitor entry, fixed live behavior gaps found during testing, and hardened the monitor after review. The final monitor emits JSONL lifecycle events for crawl, extract, embed, and ingest jobs, retries transient status failures in watch mode, uses safer monitor state handling, and reports cancellations distinctly from failures.

The work is committed on `main` as `5a55276a fix: harden job monitor state handling`. The earlier monitor implementation landed in merged PR #136 at `8540487e feat: add Tauri palette and harden search crawl (#136)`.

## Sequence of Events

1. Researched Claude Code plugin monitors with `axon ask`, confirming that a plugin can define persistent background monitors through `monitors/monitors.json`.
2. Inspected Axon job/status architecture and added `axon monitor jobs`.
3. Added `.claude-plugin/monitors/monitors.json` for the `axon-jobs` monitor.
4. Ran initial command/unit tests, then performed a real live test after the user asked for live verification.
5. Found that server-routed jobs were missed by local-only monitor reads; fixed monitor status loading to use `/v1/status` when `AXON_SERVER_URL` is active.
6. Found fast jobs could complete between monitor polls; fixed terminal-event detection for jobs created after monitor start.
7. Ran live crawl and embed tests and observed `started` and `completed` JSONL events.
8. Reviewed the monitor patch and found three issues: watch-mode hard exits on transient status errors, shared/direct state writes, and canceled jobs reported as failed.
9. Addressed all three review findings, reran focused tests, rebuilt/reinstalled the host binary, and committed the hardening patch.

## Key Findings

- `src/cli/commands/monitor.rs:80` now catches status-load errors in `--watch` mode, logs them to stderr, sleeps, and retries instead of exiting the Claude monitor process.
- `src/cli/commands/monitor.rs:210` now maps `canceled` job status to `event: "canceled"` instead of `event: "failed"`.
- `src/cli/commands/monitor.rs:371` writes monitor state through a temp file and rename instead of directly overwriting the state file.
- `.claude-plugin/monitors/monitors.json:4` now passes a per-session/per-process state file path using `${CLAUDE_SESSION_ID:-$$}`.
- `src/core/config/cli.rs:281` now documents start, completion, failure, and cancel events in help text.

## Technical Decisions

- The monitor uses the same status surface as ordinary CLI/server-mode status, avoiding a separate job-event source of truth.
- Watch mode is resilient to transient status failures; one-shot mode still exits nonzero so scripts get explicit failure.
- Canceled jobs are separate from failed jobs so downstream notifications can distinguish intentional cancellation from failure.
- The plugin monitor command still invokes `axon` from PATH so the installed host binary controls behavior; the rebuilt release binary was installed to `/home/jmagar/.local/bin/axon-4.5.0-monitor`.
- State file isolation is handled in the plugin command, while atomic replacement is handled in the CLI.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| created | `.claude-plugin/monitors/monitors.json` | | Claude plugin monitor definition for `axon-jobs` | Added in `8540487e`, hardened in `5a55276a` |
| created | `src/cli/commands/monitor.rs` | | `axon monitor jobs` implementation and event detection | Added in `8540487e`, hardened in `5a55276a` |
| modified | `src/core/config/cli.rs` | | Added monitor CLI subcommand and updated help text | `git show --name-status HEAD` |
| created | `tests/monitor_jobs.rs` | | Regression tests for monitor event detection | Added in `8540487e`, canceled-event test added in `5a55276a` |
| modified | `/home/jmagar/.local/bin/axon-4.5.0-monitor` | | Installed release binary used by `axon` symlink | `readlink -f /home/jmagar/.local/bin/axon` returned this path |

## Beads Activity

No bead activity observed for the Claude plugin monitor work in this session. `bd list --all --sort updated --reverse --limit 100 --json` and `.beads/interactions.jsonl` were inspected; recent interactions were unrelated prior PR/review closures and no monitor-specific bead was created, edited, or closed during this closeout.

## Repository Maintenance

- Plans: `find docs/plans -maxdepth 2 -type f` was inspected. No monitor-specific plan file existed, so no completed plan was moved.
- Beads: tracker state was read before writing this note. No monitor-specific bead action was needed because the work was already implemented, verified, and committed.
- Worktrees and branches: `git worktree list --porcelain`, `git branch -vv`, and `gh pr list --state open` were inspected. Active worktrees exist for PRs #133, #134, and #135; they were left untouched because they correspond to active branches/PRs.
- Main branch state: `git rev-list --left-right --count main...origin/main` returned `0 0`; `main` is aligned with `origin/main`.
- Stale docs: the user-visible CLI help text for `axon monitor jobs` was updated from failure-only wording to include cancel events. Broader docs were not changed because the monitor behavior is currently documented by CLI help and this session note.

## Tools and Skills Used

- Skill: `save-to-md` for this session note and repository maintenance pass.
- Shell commands: git, gh, bd, cargo, axon, docker compose, sqlite3, timeout, sed, rg, nl, wc, ls, readlink, install.
- File tools: `apply_patch` for source and session-note edits.
- Web/knowledge tooling: `axon ask` was used earlier to retrieve Claude Code plugin monitor details from indexed docs.
- GitHub CLI: used to inspect PR #136 and open PR state.
- No subagents were spawned.
- No browser tools were used.

## Commands Executed

| command | result |
|---|---|
| `axon ask how can i create a claude plugin monitor` | Returned plugin monitor guidance and sources from indexed Claude docs |
| `cargo test --test monitor_jobs` | Passed; final run reported 4 tests passing |
| `cargo fmt --check` | Passed |
| `cargo build --bin axon` | Passed |
| `cargo build --release --bin axon` | Passed |
| `timeout 3s env AXON_SERVER_URL=http://127.0.0.1:9 ./target/debug/axon monitor jobs --watch --jsonl --interval-secs 1 --state-file /tmp/axon-monitor-retry-state.json` | Exited with timeout code 124 after repeated retry messages, proving watch mode stayed alive |
| `env AXON_SERVER_URL=http://127.0.0.1:9 ./target/debug/axon monitor jobs --jsonl --interval-secs 1 --state-file /tmp/axon-monitor-retry-state.json` | Exited nonzero, preserving one-shot failure behavior |
| `axon monitor jobs --help` | Showed `Emit crawl/extract/embed/ingest start, completion, failure, and cancel events` |
| `readlink -f /home/jmagar/.local/bin/axon && /home/jmagar/.local/bin/axon --version` | Reported `/home/jmagar/.local/bin/axon-4.5.0-monitor` and `axon 4.5.0` |
| `git rev-list --left-right --count main...origin/main` | Returned `0 0` |

## Errors Encountered

- Live monitor initially missed jobs submitted through server mode because it read local SQLite status while CLI commands were using `AXON_SERVER_URL`. Fix: server-aware monitor status loading via `/v1/status`.
- A server `/v1/status` 500 during live testing killed the monitor process. Fix: watch mode now catches status-load errors and retries.
- Fast jobs could complete between monitor polls and miss a `running -> completed` transition. Fix: terminal events are emitted for jobs created after monitor start.
- `cargo fmt --check` initially reported a formatting diff for the temp state path expression. Fix: ran `cargo fmt`.
- Running `cargo test --test monitor_jobs` and `cargo build --release --bin axon` concurrently caused Cargo lock waiting. It completed successfully without intervention.

## Behavior Changes

| before | after |
|---|---|
| Claude monitor command existed but used a shared default state file | Plugin monitor passes a per-session/per-process state file |
| Watch-mode monitor exited on transient status failures | Watch mode logs and retries |
| One-shot monitor and watch-mode monitor had identical hard-error behavior | One-shot still fails nonzero; watch is resilient |
| Canceled jobs emitted `event: "failed"` | Canceled jobs emit `event: "canceled"` |
| CLI help mentioned failure events only | CLI help mentions start, completion, failure, and cancel events |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test --test monitor_jobs` | Monitor event regression tests pass | 4 passed, 0 failed | pass |
| `cargo fmt --check` | Formatting clean | exited 0 | pass |
| `cargo build --release --bin axon` | Release binary builds | finished release build | pass |
| Dead-server watch-mode timeout test | Monitor retries and stays alive until `timeout` | exit code 124 and repeated retry stderr lines | pass |
| Dead-server one-shot test | Monitor exits nonzero | exited code 1 with server status error | pass |
| `axon monitor jobs --help` | Help mentions cancel events | help text includes cancel events | pass |
| `axon monitor jobs --jsonl --state-file /tmp/axon-monitor-installed-state.json` | One-shot installed binary runs and writes valid state | exited 0 and wrote state JSON | pass |

## Risks and Rollback

- Risk: monitor state files can accumulate under `~/.axon/logs` because the plugin now uses a session/process-specific state file. Rollback: remove the `--state-file` argument from `.claude-plugin/monitors/monitors.json` or add a later cleanup policy.
- Risk: retrying forever in watch mode can hide persistent server failure unless stderr is inspected. Rollback or follow-up: emit structured degraded/error JSONL events if Claude should surface monitor-health failures directly.
- Rollback path: revert `5a55276a` to undo the hardening patch, or revert `8540487e` changes for the entire monitor feature if needed.

## Decisions Not Taken

- Did not create a separate long-running event bus or heartbeat integration; the monitor polls existing status surfaces.
- Did not add file locking around state writes; atomic replacement plus per-session state files addressed the observed race risk without adding cross-platform locking complexity.
- Did not delete or clean active worktrees because they correspond to active PR branches.

## References

- PR #136: `https://github.com/jmagar/axon/pull/136`
- Commit `8540487e feat: add Tauri palette and harden search crawl (#136)`
- Commit `5a55276a fix: harden job monitor state handling`
- Claude plugin docs surfaced by `axon ask`: `https://code.claude.com/docs/en/plugins-reference`, `https://code.claude.com/docs/en/plugins`, `https://code.claude.com/docs/en/tools-reference`

## Open Questions

- Whether Claude exposes a stable `CLAUDE_SESSION_ID` for monitors in every runtime; the plugin command falls back to `$$` when it is absent.
- Whether monitor-health failures should become JSONL events instead of stderr-only diagnostics.
- Whether a scheduled cleanup policy should remove old `claude-monitor-jobs-*.state.json` files.

## Next Steps

- If packaging is needed, ensure `5a55276a` is included in the release branch/tag that ships the Claude plugin monitor.
- Run a live Claude plugin session to confirm Claude itself starts the `axon-jobs` monitor and displays JSONL events as expected.
- Consider a follow-up cleanup task for old monitor state files under `~/.axon/logs`.
