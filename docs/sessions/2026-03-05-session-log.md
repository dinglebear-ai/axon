# Session Log — 2026-03-05

## 1. Session overview
- Investigated a reported stuck crawl job for `https://docs.docker.com/` and verified crawl vs embed state transitions.
- Diagnosed why auto-retry did not recover a failed embed and traced it to retry-scope limitations in TEI client error handling.
- Implemented TEI retry hardening, policy split (retry transient, fail-fast hard 4xx), timeout control, and retry observability logs.
- Updated runtime/config docs and env template; added the same TEI knobs to local `.env` per request.

## 2. Timeline of major activities
- 2026-03-05 ~03:54 UTC: Verified crawl job `b9ac5012-628c-461e-b2c3-ca567c364a8b` status = `completed` and no crawl error.
- 2026-03-05 ~03:54 UTC: Identified linked embed job `f074b58a-34cb-4f36-805d-3c07d90ff918` failed with transport error to TEI endpoint.
- 2026-03-05 ~03:54 UTC: Ran `./scripts/axon doctor --json`; TEI, Qdrant, AMQP, Postgres, Redis reported healthy at check time.
- 2026-03-05 later: Patched TEI retry logic, added tests, and passed targeted test suite repeatedly.
- 2026-03-05 later: Audited `docker-compose.yaml` env usage against `env_file: .env` policy.

## 3. Key findings with path:line references when relevant
- TEI transport failures bypassed retry loop because `.send().await?` exited early on error: `crates/vector/ops/tei/tei_client.rs` (before patch; now resolved).
- Retry policy and timeout controls now implemented via env + helper functions at [`crates/vector/ops/tei/tei_client.rs:10`](crates/vector/ops/tei/tei_client.rs:10), [`crates/vector/ops/tei/tei_client.rs:21`](crates/vector/ops/tei/tei_client.rs:21), [`crates/vector/ops/tei/tei_client.rs:25`](crates/vector/ops/tei/tei_client.rs:25), [`crates/vector/ops/tei/tei_client.rs:46`](crates/vector/ops/tei/tei_client.rs:46), [`crates/vector/ops/tei/tei_client.rs:56`](crates/vector/ops/tei/tei_client.rs:56).
- Structured retry logs were added for transport/decode/status retries at [`crates/vector/ops/tei/tei_client.rs:65`](crates/vector/ops/tei/tei_client.rs:65), [`crates/vector/ops/tei/tei_client.rs:89`](crates/vector/ops/tei/tei_client.rs:89), [`crates/vector/ops/tei/tei_client.rs:119`](crates/vector/ops/tei/tei_client.rs:119).
- Retry behavior tests now cover 429, 500, 413 split, and 404 fail-fast at [`crates/vector/ops/tei/tests.rs:20`](crates/vector/ops/tei/tests.rs:20), [`crates/vector/ops/tei/tests.rs:62`](crates/vector/ops/tei/tests.rs:62), [`crates/vector/ops/tei/tests.rs:102`](crates/vector/ops/tei/tests.rs:102), [`crates/vector/ops/tei/tests.rs:141`](crates/vector/ops/tei/tests.rs:141).
- Env/documentation knobs added at [`/home/jmagar/workspace/axon_rust/.env.example:73`](/home/jmagar/workspace/axon_rust/.env.example:73) and [`/home/jmagar/workspace/axon_rust/README.md:657`](/home/jmagar/workspace/axon_rust/README.md:657).

## 4. Technical decisions and rationale
- Decision: classify retries by failure mode.
- Rationale: retrying only 429/503 misses common transient transport failures; retrying all 4xx is wasteful/noisy.
- Implemented policy: retry `transport + 429 + 5xx`, fail fast on hard `4xx` except 429, preserve `413` split.
- Added per-attempt timeout to prevent indefinite/slow socket waits from stalling progress.
- Added structured retry logs to make TEI incidents diagnosable from logs without code spelunking.

## 5. Files modified/created and purpose
- [`/home/jmagar/workspace/axon_rust/crates/vector/ops/tei/tei_client.rs`](/home/jmagar/workspace/axon_rust/crates/vector/ops/tei/tei_client.rs): broadened retry coverage, introduced retry policy helper, env-based max retries, request timeout, structured retry logs.
- [`/home/jmagar/workspace/axon_rust/crates/vector/ops/tei/tests.rs`](/home/jmagar/workspace/axon_rust/crates/vector/ops/tei/tests.rs): added regression tests for retry-on-500 and fail-fast-on-404.
- [`/home/jmagar/workspace/axon_rust/.env.example`](/home/jmagar/workspace/axon_rust/.env.example): documented `TEI_MAX_RETRIES` and `TEI_REQUEST_TIMEOUT_MS`.
- [`/home/jmagar/workspace/axon_rust/README.md`](/home/jmagar/workspace/axon_rust/README.md): updated TEI retries/timeouts behavior documentation.
- `/home/jmagar/workspace/axon_rust/.env`: added `TEI_MAX_RETRIES=10` and `TEI_REQUEST_TIMEOUT_MS=30000` per user request.

## 6. Critical commands executed and outcomes
- `./scripts/axon crawl status b9ac5012-628c-461e-b2c3-ca567c364a8b --json` | crawl reported `completed`.
- `./scripts/axon embed list --json` | linked embed job `f074...` recorded `failed` with TEI request transport error.
- `./scripts/axon doctor --json` | services healthy at sample time; TEI check returned HTTP 200.
- `cargo test tei_embed --package axon` (run multiple times) | final result: `5 passed; 0 failed` for TEI test group.
- `docker compose` env audit commands (`sed/rg`) | confirmed most inline env values are intentional container overrides.

## 7. Behavior changes (before/after)
- Before: TEI transport errors exited immediately; retry loop effectively covered only HTTP response statuses handled in-loop.
- After: TEI transport failures are retried with exponential backoff and jitter.
- Before: retry criteria were narrower.
- After: retry criteria = transient classes (`transport`, `429`, `5xx`), with fail-fast on hard `4xx` (except 429).
- Before: no per-attempt TEI request timeout knob.
- After: `TEI_REQUEST_TIMEOUT_MS` controls per-attempt timeout.

## 8. Verification evidence (`command | expected | actual | status`)
- `./scripts/axon crawl status b9ac... --json | crawl should be terminal | status=completed, finished_at set | PASS`
- `./scripts/axon embed list --json | identify why it looked stuck | f074... status=failed, error_text indicates TEI request send failure | PASS`
- `./scripts/axon doctor --json | check current infra health | all_ok=true; TEI detail http 200 | PASS`
- `cargo test tei_embed --package axon | all TEI tests pass after patch | 5 passed, 0 failed | PASS`

## 9. Source IDs + collections touched (embed/retrieve source IDs, collections, outcomes)
- Historical in-session embed: job `93c1932b-81ad-4018-84b2-d461883beb4d` | collection `cortex` | completed (`chunks_embedded: 1`) | later MCP retrieve attempts for local path previously hit permission-denied.
- Crawl/Embed pair analyzed: crawl `b9ac5012-628c-461e-b2c3-ca567c364a8b` completed; subsequent embed `f074b58a-34cb-4f36-805d-3c07d90ff918` failed with TEI transport error.
- Additional active embed jobs observed during investigation: `53ffaec9-...` completed; `04165b16-...` and `3daf2ee1-...` were running at sample time.
- This session-log file embed/retrieve details: embed job `868100ec-d1a2-4851-aa2f-4f3b8b49a6f2` completed; status reported collection `cortex`, input `docs/sessions/2026-03-05-session-log.md`; retrieve succeeded for source ID `docs/sessions/2026-03-05-session-log.md` with `--collection cortex` and returned 1 chunk.

## 10. Risks and rollback
- Risk: broader retries can lengthen wall-clock time on persistent TEI failures.
- Mitigation: bounded `TEI_MAX_RETRIES` (clamped to 20) and per-attempt timeout limit (clamped to 600000 ms).
- Risk: fail-fast on hard 4xx may surface configuration errors faster (intended) but can increase visible failures until misconfig is fixed.
- Rollback: revert TEI client/test/doc/env changes in the listed files and restore previous retry semantics.

## 11. Decisions not taken
- Did not add docker-compose overrides for new TEI knobs; kept `.env` as the single source for non-override config.
- Did not classify specific transport-error subtypes differently (single policy currently covers all transport errors).
- Did not alter unrelated dirty working-tree files outside the TEI/env/doc scope.

## 12. Open questions
- Should retry logs include chunk cardinality for better load correlation, or is URL/attempt/status sufficient?
- Should `TEI_MAX_RETRIES` upper bound remain 20 or be increased for long TEI outages?
- Should hard-4xx fail-fast be expanded with explicit special-case handling for 408 if encountered?

## 13. Next steps
- Optionally run an end-to-end real embed against TEI under induced transient failure to validate operational behavior in staging.
- If desired, add a small metric counter for retry attempts by failure class.
- Monitor logs for `tei_embed retry ...` patterns and tune `TEI_MAX_RETRIES` / `TEI_REQUEST_TIMEOUT_MS` based on observed latency/error rates.
