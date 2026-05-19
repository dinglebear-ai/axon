# Session: CLI Live Test Execution and Script Check-In

## 1. Session overview
- Executed full live CLI validation across top-level commands and subcommand families, excluding `dedupe` as requested.
- Captured run artifacts in TSV/log format from live execution: `/tmp/axon-live-test/run-20260222-164321/report.tsv`.
- Identified three command-level failures during execution and preserved exact failure logs.
- Added harness scripts into repo for reproducibility: `scripts/live-test-all-commands.sh` and three phase scripts.
- Updated script documentation in `docs/live-test-scripts.md`.

## 2. Timeline of major activities
- Performed preflight checks (`doctor`, env presence, Docker status); confirmed services healthy and missing ingest creds (`GITHUB_TOKEN`, Reddit OAuth).
- Ran phase 1 harness (`run_all_commands.sh`), collected top-level and partial subcommand results.
- Ran phase 2/3 harnesses to complete remaining subcommands and worker invocations.
- Consolidated results into one report (`report.tsv`) and analyzed pass/fail counts.
- Moved scripts into repo and documented locations.

## 3. Key findings with `path:line` references
- `crawl <url>` fails by treating URL as subcommand fallback path in runtime behavior; observed failures in report and logs (`/tmp/axon-live-test/run-20260222-164321/report.tsv:6`, `/tmp/axon-live-test/run-20260222-164321/logs/T05.log:1`).
- `scrape https://neverssl.com` failed with request-send error (`/tmp/axon-live-test/run-20260222-164321/logs/T03.log:14`).
- Live run aggregate: `95 PASS`, `3 FAIL`, `1 SKIP` (`/tmp/axon-live-test/run-20260222-164321/report.tsv:1`).
- Collection delta during run was `+7` points (`2569388 -> 2569395`) from Qdrant count responses observed in run outputs.
- In code, `run_crawl` routes through `maybe_handle_subcommand` first (`crates/cli/commands/crawl.rs:22`, `crates/cli/commands/crawl.rs:34`).

## 4. Technical decisions and rationale
- Preserved `dedupe` exclusion because it deletes Qdrant data (explicit user constraint).
- Used daemon-safe worker validation (`timeout`) to execute worker commands without indefinite hang.
- Retained all original phase scripts in repo for traceability and reproducibility.
- Added a consolidated in-repo harness to avoid multi-phase manual orchestration.
- Kept outputs as TSV + per-command logs to support exact, auditable outcomes.

## 5. Files modified/created and purpose
- `scripts/live-test-all-commands.sh`: consolidated end-to-end live test harness.
- `scripts/live-test-run-all-commands-phase1.sh`: original phase 1 script copied into repo.
- `scripts/live-test-run-subcommands-phase2.sh`: original phase 2 script copied into repo.
- `scripts/live-test-run-phase3-remaining.sh`: original phase 3 script copied into repo.
- `docs/live-test-scripts.md`: script inventory and usage/output notes.

## 6. Critical commands executed and outcomes
- `./scripts/axon doctor` -> PASS (services/pipelines healthy; webdriver optional-not-configured).
- `./scripts/axon scrape https://neverssl.com` -> FAIL (`error sending request`).
- `./scripts/axon crawl https://neverssl.com` -> FAIL (`unknown crawl subcommand`).
- `./scripts/axon sources` and `./scripts/axon domains` -> PASS (long-running on large collection).
- Worker commands (`crawl|batch|extract|embed|github|reddit|youtube worker`) executed under timeout and recorded as PASS with expected timeout termination.

## 7. Behavior changes (before/after)
- Before: live test scripts existed only under `/tmp/axon-live-test/`.
- After: scripts are available in repo under `scripts/` with executable bits set.
- Before: docs pointed to temp script locations.
- After: docs include in-repo canonical script references (`docs/live-test-scripts.md:1`).
- Before: no single in-repo full harness.
- After: `scripts/live-test-all-commands.sh` provides one-command execution path.

## 8. Verification evidence (`command | expected | actual | status`)
| command | expected | actual | status |
|---|---|---|---|
| `awk` result tally on report | Counts available | `PASS=95, FAIL=3, SKIP=1` | PASS |
| `./scripts/axon scrape https://neverssl.com` | scrape + embed path works | `error sending request` | FAIL |
| `./scripts/axon crawl https://neverssl.com` | enqueue/execute crawl | `unknown crawl subcommand` | FAIL |
| `./scripts/axon sources` | list indexed sources | completed successfully | PASS |
| `./scripts/axon domains` | list domains/facets | completed successfully | PASS |

## 9. Source IDs + collections touched (embed/retrieve IDs, collections, outcomes)
- Collection observed in live run: `cortex` (from command outputs and report rows).
- Job/source-style IDs observed during command execution include:
  - `1c1400d4-f9fc-44fe-8222-2831bbe78f41` (batch enqueue)
  - `eb87a75c-d7dc-4bc9-8444-41bed16c60e6` (embed enqueue)
  - `355a7440-ee9f-44a4-ad17-64222ac9431f` (github enqueue)
  - `abc0cd42-dff3-4ea6-84bf-0ac8ca1244c4` (reddit enqueue)
  - `0a525717-7e26-4c32-b658-02cdbf1a3d90` (youtube enqueue)
- Live report path containing IDs and outcomes: `/tmp/axon-live-test/run-20260222-164321/report.tsv`.
- Session-document embed/retrieve verification is recorded after this file write (see final response for runtime values).

## 10. Risks and rollback
- Risk: harness currently records embed verification by immediate count delta; async processing can under-report true embedding outcomes.
- Risk: `crawl <url>` path is failing; live test automation cannot validate crawl enqueue until fixed.
- Risk: ingest queues may remain pending when ingest workers/credentials are not available.
- Rollback for script additions: remove the four `scripts/live-test-*.sh` files and revert `docs/live-test-scripts.md`.

## 11. Decisions not taken
- Did not run `dedupe` (explicitly excluded due Qdrant data deletion).
- Did not add optional tuning flags to command invocations; defaults/minimum required inputs only.
- Did not force-start external credential providers for GitHub/Reddit; recorded observed behavior.
- Did not overwrite any existing session doc filename; used collision-safe naming.

## 12. Open questions
- Should `crawl <url>` treat first positional URL as URL (not subcommand) in all non-subcommand cases?
- Should ingest `errors`/`doctor` subcommands for `github|reddit|youtube` be implemented explicitly instead of enqueue-like behavior?
- Should embed verification in harness wait for terminal job state before checking Qdrant delta for async families?

## 13. Next steps
- Fix crawl positional/subcommand resolution and re-run full live harness.
- Improve harness verification logic for async embedding confirmation.
- Add optional markdown report generator from TSV outputs.
- Keep one canonical script (`live-test-all-commands.sh`) and mark phase scripts as legacy if no longer needed.

## Post-save embed/retrieve outcome
- Embed command executed: `./scripts/axon --json embed "docs/sessions/2026-02-22-cli-live-test-execution-v2.md"`.
- Embed output: `{"job_id":"7468c354-3493-454d-afb8-e4216e8eae92","status":"pending","source":"rust"}`.
- Embed status result: `failed`, with `error_text` = `HTTP status client error (400 Bad Request) for url (http://axon-qdrant:6333/collections/cortex)`.
- Retrieve verification attempted using embed-derived values (`source_id=''`, `collection=''`) and failed with 404 on `collections//points/scroll`.
- Re-attempted on final file state: embed job `0e605c8f-1147-477d-910f-70d8bf4a4181` also failed with same HTTP 400 endpoint error.
