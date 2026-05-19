# Session Log — 2026-02-26 — Qdrant startup health, watchdog reclaim analysis, and status reclaimed filter

## 1. Session overview
- Debugged a startup incident where `axon-qdrant` was reported unhealthy and blocked `axon-workers` startup.
- Investigated ingest watchdog reclaim behavior using code + Postgres job history for specific reclaimed job IDs.
- Implemented runtime hardening: ingest and extract DB heartbeats plus completion-race visibility logs.
- Implemented status UX change: hide reclaimed jobs by default and add `--reclaimed` mode to show only reclaimed jobs.

## 2. Timeline of major activities
- Collected live container state (`docker ps`, health inspect, qdrant logs) and confirmed qdrant readiness delay vs health budget mismatch.
- Patched qdrant health check window in compose to avoid false unhealthy during large collection recovery.
- Traced watchdog reclaim path from worker lanes to shared watchdog reclaim SQL and inspected affected ingest rows in Postgres.
- Added heartbeat logic to ingest and extract workers; added completion update `rows_affected == 0` warnings.
- Added status filtering for watchdog-reclaimed jobs and CLI/global config support for `--reclaimed`.

## 3. Key findings with path:line references when relevant
- Qdrant needed ~90s to finish collection recovery; previous health budget was too short and could fail startup sequencing.
  - `docker-compose.yaml:101`
- Watchdog reclaim is two-pass and keys off unchanged `updated_at`, then writes `failed` with reclaim message.
  - `crates/jobs/common/watchdog.rs:103`
  - `crates/jobs/common/watchdog.rs:128`
- Ingest jobs had no running heartbeat before this session, so long runs could be reclaimed despite active processing.
  - `crates/jobs/ingest.rs:324`
- Embed already had heartbeat behavior; ingest/extract did not.
  - `crates/jobs/embed/worker.rs:153`
- Status output previously included reclaimed failures in normal view; now filtered by reclaim marker.
  - `crates/cli/commands/status.rs:135`

## 4. Technical decisions and rationale
- Increased qdrant start budget (`retries` + `start_period`) instead of weakening readiness probe semantics; preserved `/readyz` check.
- Added heartbeat updates at worker level (not ingest source modules) to keep one owner for job-lifecycle state updates.
- Used marker-based reclaimed detection (`error_text` prefix) so filtering remains DB-schema compatible.
- Kept default `status` focused on actionable current/hard failures by excluding reclaimed rows; moved reclaimed visibility to explicit `--reclaimed`.

## 5. Files modified/created and purpose
- `docker-compose.yaml`: increased qdrant healthcheck timing budget (`retries`, `start_period`).
- `crates/jobs/ingest.rs`: added ingest heartbeat loop and completion-skip warning.
- `crates/jobs/extract/worker.rs`: added extract heartbeat loop and completion-skip warning.
- `crates/core/config/cli.rs`: added global `--reclaimed` flag.
- `crates/core/config/types.rs`: added config field `reclaimed_status_only` + defaults/debug/test assertion.
- `crates/core/config/parse.rs`: mapped CLI `--reclaimed` into runtime config.
- `crates/cli/commands/status.rs`: added reclaimed filter logic and tests.
- `README.md`: documented `--reclaimed` in global flags.
- `docs/sessions/2026-02-26-context-injection-cleanup.md`: this session log file.

## 6. Critical commands executed and outcomes
- `docker inspect --format '{{json .State.Health}}' axon-qdrant` -> qdrant currently healthy; health probe data confirmed runtime readiness behavior.
- `docker logs --tail 200 axon-qdrant` -> showed long collection recovery before HTTP listener became ready.
- `docker exec -i axon-postgres psql ... axon_ingest_jobs ...` -> confirmed reclaimed ingest rows and exact `error_text` marker values.
- `cargo check -q` -> passed after ingest/extract heartbeat and status filter changes.
- `cargo test -q ingest -- --nocapture` -> passed (`91 passed`).
- `cargo test -q extract -- --nocapture` -> passed (`20 passed`).
- `cargo test -q status -- --nocapture` -> passed (`20 passed`).
- `./scripts/axon status --json` + `./scripts/axon status --reclaimed --json` -> default view excluded reclaimed; reclaimed mode returned reclaimed rows including known ingest IDs.

## 7. Behavior changes (before/after)
- Before: `status` included watchdog-reclaimed failures in normal output.
- After: `status` excludes watchdog-reclaimed failures by default.
- Before: no direct status mode to inspect reclaimed failures only.
- After: `status --reclaimed` shows only watchdog-reclaimed rows across crawl/extract/embed/ingest.
- Before: ingest/extract long-running jobs could look stale if `updated_at` did not move during processing.
- After: ingest/extract send periodic `updated_at` heartbeat while running.

## 8. Verification evidence (`command | expected | actual | status`)
| command | expected | actual | status |
|---|---|---|---|
| `cargo check -q` | workspace compiles | no compile errors | ✅ |
| `cargo test -q ingest -- --nocapture` | ingest tests pass | `91 passed; 0 failed` | ✅ |
| `cargo test -q extract -- --nocapture` | extract tests pass | `20 passed; 0 failed` | ✅ |
| `cargo test -q status -- --nocapture` | status tests (incl filter tests) pass | `20 passed; 0 failed` | ✅ |
| `./scripts/axon status --json` filtered for known reclaimed IDs | reclaimed IDs not shown in default status | no matches found | ✅ |
| `./scripts/axon status --reclaimed --json` filtered for known reclaimed IDs | reclaimed IDs visible in reclaimed mode | matched `2cdc4d78-...` and `247ead64-...` plus reclaim errors | ✅ |

## 9. Source IDs + collections touched (embed/retrieve source IDs, collections, outcomes)
- Pre-existing evidence used during investigation:
  - Reclaimed job IDs examined in Postgres: `2cdc4d78-b0de-43df-ac9f-96c1daedd0bd`, `d4e0e3b4-e526-459d-ab20-0ee235a2200f`, `247ead64-992a-47c9-91e7-d5d14efa8032`, `ccf028f6-3c32-4d6b-a3b6-2ac715014ac3`.
- Axon embed execution for this session file:
  - Async embed command output: job `f856a1da-f2eb-476f-9c64-d29a80e9f7bc` accepted and completed (`status=completed`).
  - Observed source ID used for retrieval: `docs/sessions/2026-02-26-context-injection-cleanup.md` (`input_text` / `result_json.input`).
  - Observed collection: `cortex` (`result_json.collection`).
- Axon retrieve verification:
  - Command: `./scripts/axon retrieve "docs/sessions/2026-02-26-context-injection-cleanup.md" --collection "cortex" --json`.
  - Outcome: success with `chunks=5` and `url` matching the source ID path.

## 10. Risks and rollback
- Risk: reclaimed filter currently relies on `error_text` prefix match; changing reclaim message format can break filtering semantics.
- Risk: if heartbeat task dies unexpectedly, reclaim risk returns; warning logs were added for heartbeat task panic visibility.
- Rollback path:
  - Revert status filter/flag changes in `status.rs` + config files.
  - Revert heartbeat additions in ingest/extract worker paths.
  - Revert qdrant health budget adjustments in `docker-compose.yaml`.

## 11. Decisions not taken
- Did not implement automatic retry/requeue for reclaimed jobs.
- Did not add `--include-reclaimed` combined mode (normal + reclaimed together).
- Did not add source-type-specific watchdog thresholds (single global thresholds retained).

## 12. Open questions
- Should reclaim filtering be marker-based via structured `result_json` data instead of `error_text` prefix matching?
- Should reclaimed jobs be auto-retried with bounded retries for idempotent sources (`sessions`, `github`)?
- Should stale timeout/confirm be per job family or per ingest source type?

## 13. Next steps
- Add bounded retry policy for reclaimed/transient failures and include retry metadata in job rows.
- Add docs updates for heartbeat behavior in `docs/JOB-LIFECYCLE.md` and `docs/PERFORMANCE.md`.
- Optionally add `status --include-reclaimed` for merged visibility.
