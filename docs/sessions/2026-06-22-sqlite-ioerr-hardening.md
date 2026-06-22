---
date: 2026-06-22 08:00:49 EDT
repo: git@github.com:jmagar/axon.git
branch: codex/debug-disk-io-ingest
head: 7682316a
working directory: /home/jmagar/workspace/axon/.worktrees/debug-disk-io-ingest
worktree: /home/jmagar/workspace/axon/.worktrees/debug-disk-io-ingest
beads: axon_rust-na6u, axon_rust-7zly, axon_rust-ehdl
---

# SQLite IOERR hardening session

## User Request

The session began from an Axon log attachment showing `(code: 522) disk I/O error` failures in SQLite job tables. The user asked to use systematic debugging and then explicitly requested a new worktree first.

## Session Overview

Created and worked in `/home/jmagar/workspace/axon/.worktrees/debug-disk-io-ingest` on `codex/debug-disk-io-ingest`. Diagnosed the SQLite IOERR pattern, implemented recovery guards and diagnostics, added regression tests including a cross-process recovery-lock test, rebuilt the Axon debug binary, and restarted the dev container onto the worktree build.

## Sequence of Events

1. Created the isolated worktree and branch before touching code.
2. Read the pasted log attachment and verified live DB/runtime state with SQLite, Docker, process, and log commands.
3. Found that automatic corruption recovery could rename `jobs.db` while another Axon process still held the database open.
4. Added active-owner/recovery lock handling and regression coverage.
5. Added follow-up diagnostics: runtime SQLite IOERR tracking, status/doctor SQLite diagnostics, `/readyz` SQLite readiness, and sidecar recency by modified time.
6. Built and restarted the live dev container using the worktree `target/debug/axon`.
7. Created a follow-up docs bead and saved this session artifact.

## Key Findings

- Live evidence showed `~/.axon/jobs.db` currently passed `PRAGMA quick_check`, while prior `.corrupted.*` files existed, including `/home/jmagar/.axon/jobs.db.corrupted.1782102561`.
- The Axon container bind-mounted `/home/jmagar/.axon` into `/home/axon/.axon`, so host and container recovery behavior can affect the same job DB.
- The original recovery path only guarded corruption recovery at open time; a second process could opportunistically rename the DB while a long-lived service had live handles.
- Active-owner locks and recovery locks are now implemented around `jobs.db.active.lock` in `src/jobs/store.rs:51`.
- Runtime worker claim errors now feed the SQLite IOERR marker at `src/jobs/workers.rs:257`.

## Technical Decisions

- Used an advisory sidecar lock (`jobs.db.active.lock`) instead of changing SQLite schema or job table layouts, keeping the fix local to DB ownership/recovery behavior.
- Kept active DB locks process-lifetime and idempotent by path, avoiding duplicate lock handles on repeated status/doctor calls.
- Kept diagnostics read-only: `sqlite_diagnostics` uses a read-only quick_check and never triggers recovery or migrations.
- Surfaced SQLite health through existing degraded/status contracts instead of adding a separate status API.
- Selected latest `.corrupted.*` sidecar by file modified time because existing sidecars use mixed suffix formats.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `src/jobs/store.rs` | - | Active-owner and recovery locks, runtime IOERR marker, read-only SQLite diagnostics | `src/jobs/store.rs:51`, `src/jobs/store.rs:145`, `src/jobs/store.rs:173` |
| modified | `src/jobs/store_tests.rs` | - | Regression tests for idempotent locks, same-process and cross-process recovery refusal, diagnostics sidecars | `cargo test jobs::store::tests --lib` passed |
| modified | `src/jobs/workers.rs` | - | Record non-busy worker claim errors as possible SQLite runtime IOERRs | `src/jobs/workers.rs:257` |
| modified | `src/services/system/status.rs` | - | Include SQLite diagnostics in status payload and degraded errors | `src/services/system/status.rs:20` |
| modified | `src/services/system/status_tests.rs` | - | Status degradation test for runtime IOERR diagnostics | `cargo test full_status_includes_sqlite_diagnostics_and_degrades_on_runtime_ioerr --lib` passed |
| modified | `src/services/system.rs` | - | Re-export status helper/payload builder | `cargo check --lib` passed |
| modified | `src/cli/commands/status.rs` | - | Human/JSON status paths include SQLite degraded state | `cargo check --lib` passed |
| modified | `src/core/health/doctor/sqlite.rs` | - | Doctor uses shared SQLite diagnostics and includes SQLite in `all_ok` | `src/core/health/doctor/sqlite.rs:41` |
| modified | `src/cli/commands/doctor/render.rs` | - | Human doctor line prints quick_check, sidecar count, runtime IOERR count | `./target/debug/axon --json doctor` verified JSON diagnostics |
| modified | `src/web/health.rs` | - | `/readyz` now includes SQLite readiness | `src/web/health.rs:21`, live curl verified |
| created | `docs/sessions/2026-06-22-sqlite-ioerr-hardening.md` | - | Session artifact | This file |

## Beads Activity

| bead | title | action(s) | final status | why it mattered |
|---|---|---|---|---|
| `axon_rust-na6u` | Guard Axon SQLite recovery against active DB owners | Created, claimed, closed | closed | Tracked the initial recovery-lock fix and regression coverage |
| `axon_rust-7zly` | Harden Axon SQLite IOERR diagnostics and recovery | Created, claimed, closed | closed | Tracked follow-up diagnostics, readiness/status surfacing, cross-process test, and dev-container restart |
| `axon_rust-ehdl` | Document SQLite readiness and diagnostics fields | Created | open | Captures remaining docs update for changed `doctor`, `status`, and `/readyz` behavior |

## Repository Maintenance

### Plans

Reviewed `docs/plans/` and `docs/plans/complete/`. No plan was moved: no file in the active plan list was clearly tied to this SQLite IOERR session, and `.claude/current-plan` pointed at `/home/jmagar/workspace/axon_rust/docs/plans/2026-05-27-android-phase2-stubbed-modes.md`, outside this worktree and unrelated to the session.

### Beads

Read focused bead state with `bd show axon_rust-na6u --json`, `bd show axon_rust-7zly --json`, and `bd show axon_rust-ehdl --json`. Closed completed beads only after verification had passed. Created `axon_rust-ehdl` for stale docs follow-up.

### Worktrees and branches

Inspected `git worktree list --porcelain`, `git branch -vv`, and `git branch -r -vv`. No worktrees or branches were removed. Several sibling worktrees exist (`android-full-app-qa`, `palette-full-qa`, `suggest-500-fix`) and ownership/status was not proven safe for cleanup.

### Stale docs

Did not update docs in this save-session pass because the save-to-md workflow commits only the generated artifact, and the implementation changes are still uncommitted. Created `axon_rust-ehdl` to update docs for SQLite diagnostics and readiness fields.

### Transparency

No transcript file was found under the probed Claude/Codex session paths; this note is based on observed tool output and current repo state in the Codex thread. The branch remains dirty with implementation changes; this artifact commit intentionally stages only the generated session note.

## Tools and Skills Used

- **Skills.** `superpowers:using-superpowers`, `superpowers:using-git-worktrees`, `superpowers:systematic-debugging`, `superpowers:test-driven-development`, `superpowers:requesting-code-review`, and `vibin:save-to-md`.
- **Shell commands.** Used `git`, `bd`, `sqlite3`, `docker`, `curl`, `jq`, `cargo`, and `date` for diagnosis, implementation validation, deployment, and maintenance evidence.
- **File tools.** Used patch-based file edits for Rust source and this session artifact.
- **MCP tools.** Used Lumen semantic search for initial code discovery. It later returned transient embedding `429` overloads, so exact-string shell lookups were used for known symbols/messages.
- **External CLIs/services.** Docker Compose restarted the Axon dev container; `bd` tracked task state. No subagents were spawned because the available multi-agent tool required an explicit user request for subagents.

## Commands Executed

| command | result |
|---|---|
| `git worktree add -b codex/debug-disk-io-ingest ...` | Created isolated worktree before edits |
| `sqlite3 /home/jmagar/.axon/jobs.db 'PRAGMA quick_check; PRAGMA journal_mode; PRAGMA wal_checkpoint(PASSIVE);'` | Reported `ok`, `wal`, and checkpoint state |
| `docker inspect axon --format ...` | Confirmed `/home/jmagar/.axon` bind mount and later confirmed worktree `target/debug` mount |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=true cargo test jobs::store::tests --lib` | Passed, including cross-process recovery test |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=true cargo test full_status_includes_sqlite_diagnostics_and_degrades_on_runtime_ioerr --lib` | Passed |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=true cargo check --lib` | Passed |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=true cargo build --bin axon` | Built debug binary for dev container |
| `docker compose --env-file /home/jmagar/.axon/.env -f docker-compose.yaml restart axon` | Restarted Axon dev container |
| `curl -H 'Host: axon.tootie.tv' http://127.0.0.1:40090/readyz` | Returned `{"ok":true,"sqlite":"ready","qdrant":"ready","tei":"ready"}` |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=true ./target/debug/axon --json doctor \| jq '.services.sqlite, .all_ok'` | Showed `quick_check: ok`, `runtime_ioerr_count: 0`, and `all_ok: true` |

## Errors Encountered

- Initial plain `cargo build --bin axon` was blocked by pre-existing empty `apps/web/out`; used `AXON_ALLOW_FALLBACK_WEB_ASSETS=true`, matching prior repo behavior for this environment.
- A first lock implementation used unsafe `libc::flock`; the crate denies unsafe code, so it was replaced with safe `File::try_lock_shared` / `File::try_lock`.
- Cargo test filters with multiple names failed because Cargo accepts a single test filter; reran module or single-test filters.
- Lumen semantic search returned `HTTP 429` from its embedding backend on follow-up searches; switched to exact known string lookups.
- `/readyz` was first checked on host port `8001`, which belonged to a separate host process. The container publishes to `40090`, and host-header validation requires `Host: axon.tootie.tv`.
- CLI doctor emitted an unrelated Codex app-server usage-limit warning while SQLite/Qdrant/TEI readiness remained clean.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| SQLite corruption recovery | A short-lived Axon process could rename `jobs.db` during recovery without proving no active owner existed | Recovery refuses while another Axon process holds the active-owner lock |
| Runtime IOERR visibility | Worker claim `SQLITE_IOERR` was logged but not reflected in health/status | Runtime IOERR count and last error are recorded and surfaced |
| `axon status` | No SQLite diagnostics block | Includes `sqlite` diagnostics and degraded errors |
| `axon doctor` | SQLite service line only reported path/existence | Reports quick_check, active lock, sidecar metadata, runtime IOERR count |
| `/readyz` | Reported Qdrant and TEI only | Reports SQLite, Qdrant, and TEI readiness |
| Corrupted sidecar recency | Latest sidecar selection could be lexical and wrong with mixed suffixes | Latest sidecar is selected by modified time |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo fmt` | Formatting succeeds | Exit 0 | pass |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=true cargo test jobs::store::tests --lib` | Store tests pass | 11 passed, 1 ignored child helper | pass |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=true cargo test full_status_includes_sqlite_diagnostics_and_degrades_on_runtime_ioerr --lib` | Status IOERR test passes | 1 passed | pass |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=true cargo check --lib` | Library check succeeds | Finished dev profile | pass |
| `AXON_ALLOW_FALLBACK_WEB_ASSETS=true cargo build --bin axon` | Debug binary builds | Finished dev profile | pass |
| `curl -H 'Host: axon.tootie.tv' http://127.0.0.1:40090/readyz` | SQLite included and ready | `{"ok":true,"sqlite":"ready","qdrant":"ready","tei":"ready"}` | pass |
| `./target/debug/axon --json doctor \| jq '.services.sqlite, .all_ok'` | SQLite diagnostics healthy | `quick_check: ok`, `runtime_ioerr_count: 0`, `all_ok: true` | pass |

## Risks and Rollback

- Risk: Advisory locks only protect processes running this new code. Old long-lived Axon processes must be restarted to participate.
- Risk: `runtime_ioerr_count` is process-local, so restart clears it. Persistent history comes from logs and sidecar files.
- Risk: `/readyz` now fails if SQLite diagnostics fail; this is intended but may change orchestration behavior.
- Rollback: revert the modified source files and restart Axon from the previous binary or main checkout target. Existing `jobs.db` data is not migrated by this change.

## Decisions Not Taken

- Did not implement automatic pool reopen on runtime IOERR; readiness degradation and restart behavior are safer than continuing with uncertain DB handles.
- Did not remove any worktrees or branches; sibling worktrees were not proven safe to delete.
- Did not update docs inline in this pass; created `axon_rust-ehdl` because the save-session commit must include only the generated session artifact.

## Open Questions

- Whether the separate host process listening on port `8001` should be stopped or aligned with the containerized dev service.
- Whether runtime IOERR health should be persisted across restarts in a small sidecar or metrics store.

## Next Steps

- Commit or PR the implementation changes on `codex/debug-disk-io-ingest`; this session artifact commit intentionally excludes them.
- Address `axon_rust-ehdl` by updating docs for new SQLite `doctor`, `status`, and `/readyz` fields.
- Verify whether the host Axon process on `8001` is expected; if not, stop or reconfigure it to avoid confusion with the container on `40090`.
- After merge, redeploy/restart every Axon process that touches `~/.axon/jobs.db`.
