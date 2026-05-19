# Session Overview

- Date: `2026-03-20`
- Repo: `axon_rust`
- Scope captured from this session thread: graph-ingest follow-up after a machine switch, infrastructure work to add Ollama, RRF/query diagnostics hardening, and session-log persistence.
- Verified implementation work in the current worktree focused on Ollama service wiring, Qdrant mode-probe behavior, typed diagnostics payload plumbing, and query diagnostics tests.
- This document records only facts observed in the conversation, git diff, and command output from this session.

# Timeline Of Major Activities

- User reported pending session docs were not added to the graph after switching machines, then redirected work to add Ollama to `docker-compose.services.yaml` and pull `qwen3.5:4b`.
- User later reported graph ingestion was operational and asked how the graph path works, whether it is unified-pipeline based, and whether `PreparedDoc` is involved.
- User requested a review of the RRF implementation and asked that the first two findings be fixed while leaving session docs unchanged.
- Follow-up hardening requests covered mode-probe retry tests, explicit `404` handling, diagnostics payload surfacing, query diagnostics compatibility, and an existing clippy warning.
- Final follow-up requested two concrete additions: an integration-level query diagnostics contract test and moving diagnostics metadata out of error strings into typed payloads across CLI/MCP/web paths.

# Key Findings

- Ollama infrastructure was added as a compose service at [docker-compose.services.yaml:202](/home/jmagar/workspace/axon_rust/docker-compose.services.yaml#L202), exposing `127.0.0.1:11434` with a healthcheck based on `ollama list`.
- Qdrant mode probing now fails fast instead of silently degrading to unnamed mode, with retries on retryable probe failures and explicit `404` collection-not-found errors at [crates/vector/ops/tei/qdrant_store.rs:101](/home/jmagar/workspace/axon_rust/crates/vector/ops/tei/qdrant_store.rs#L101) and [crates/vector/ops/tei/qdrant_store.rs:154](/home/jmagar/workspace/axon_rust/crates/vector/ops/tei/qdrant_store.rs#L154).
- Focused probe tests cover `404`, `429`, and `500` behavior at [crates/vector/ops/tei/qdrant_store/tests.rs:278](/home/jmagar/workspace/axon_rust/crates/vector/ops/tei/qdrant_store/tests.rs#L278), [crates/vector/ops/tei/qdrant_store/tests.rs:316](/home/jmagar/workspace/axon_rust/crates/vector/ops/tei/qdrant_store/tests.rs#L316), and [crates/vector/ops/tei/qdrant_store/tests.rs:350](/home/jmagar/workspace/axon_rust/crates/vector/ops/tei/qdrant_store/tests.rs#L350).
- Query and ask retrieval paths now attach structured diagnostics payloads instead of encoding diagnostics only in error strings at [crates/vector/ops/commands/query.rs:19](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/query.rs#L19) and [crates/vector/ops/commands/ask/context/retrieval.rs:32](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/ask/context/retrieval.rs#L32).
- Typed diagnostics extraction is centralized in [crates/services/error.rs:5](/home/jmagar/workspace/axon_rust/crates/services/error.rs#L5), then propagated through service, MCP, and web sanitization layers at [crates/services/query.rs:16](/home/jmagar/workspace/axon_rust/crates/services/query.rs#L16), [crates/mcp/server/common.rs:26](/home/jmagar/workspace/axon_rust/crates/mcp/server/common.rs#L26), and [crates/web/execute/sync_mode/service_calls.rs:62](/home/jmagar/workspace/axon_rust/crates/web/execute/sync_mode/service_calls.rs#L62).
- Query CLI diagnostics are now first-class via [crates/core/config/cli.rs:31](/home/jmagar/workspace/axon_rust/crates/core/config/cli.rs#L31), [crates/core/config/cli.rs:208](/home/jmagar/workspace/axon_rust/crates/core/config/cli.rs#L208), [crates/core/config/parse/build_config.rs:139](/home/jmagar/workspace/axon_rust/crates/core/config/parse/build_config.rs#L139), and [crates/cli/commands/query.rs:36](/home/jmagar/workspace/axon_rust/crates/cli/commands/query.rs#L36).

# Technical Decisions And Rationale

- Mode-probe failures were changed from dense-only fallback to explicit errors because incorrect fallback can misroute named collections to the wrong Qdrant endpoint; that rationale is documented directly in [crates/vector/ops/tei/qdrant_store.rs:96](/home/jmagar/workspace/axon_rust/crates/vector/ops/tei/qdrant_store.rs#L96).
- Retry coverage was added for retryable probe statuses (`429`, `5xx`) and transport failures while preserving immediate failure for `404`; this locks intended operator-facing behavior in unit tests.
- Diagnostics metadata was moved into `ServiceError` so CLI, MCP, and web code can preserve the same structured payload while still sanitizing the user-facing error message.
- Query got its own `--diagnostics` flag rather than relying only on ask/evaluate wiring, implemented at [crates/core/config/cli.rs:208](/home/jmagar/workspace/axon_rust/crates/core/config/cli.rs#L208) and [crates/core/config/parse/build_config.rs:139](/home/jmagar/workspace/axon_rust/crates/core/config/parse/build_config.rs#L139).
- A black-box CLI test was added at [tests/query_diagnostics_error_contract.rs:5](/home/jmagar/workspace/axon_rust/tests/query_diagnostics_error_contract.rs#L5) to lock stderr diagnostics behavior for `axon query --diagnostics`.

# Files Modified Or Created And Purpose

- [docker-compose.services.yaml](/home/jmagar/workspace/axon_rust/docker-compose.services.yaml): add `axon-ollama` service, port mapping, volume, and healthcheck.
- [crates/vector/ops/tei/qdrant_store.rs](/home/jmagar/workspace/axon_rust/crates/vector/ops/tei/qdrant_store.rs): change vector-mode probing to retry retryable failures and error on `404`/other bad responses.
- [crates/vector/ops/tei/qdrant_store/tests.rs](/home/jmagar/workspace/axon_rust/crates/vector/ops/tei/qdrant_store/tests.rs): add probe regression tests for `404`, `429`, and `500`.
- [crates/vector/ops/commands/query.rs](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/query.rs): attach structured query diagnostics on vector dispatch failures.
- [crates/vector/ops/commands/ask/context/retrieval.rs](/home/jmagar/workspace/axon_rust/crates/vector/ops/commands/ask/context/retrieval.rs): attach structured ask diagnostics on vector dispatch failures.
- [crates/services/error.rs](/home/jmagar/workspace/axon_rust/crates/services/error.rs): new typed diagnostics error carrier and source-chain extractor.
- [crates/services/query.rs](/home/jmagar/workspace/axon_rust/crates/services/query.rs): preserve diagnostics through service-level query/ask wrapping and add an async diagnostics test.
- [crates/core/config/cli.rs](/home/jmagar/workspace/axon_rust/crates/core/config/cli.rs): add `QueryArgs { diagnostics, value }`.
- [crates/core/config/parse/build_config.rs](/home/jmagar/workspace/axon_rust/crates/core/config/parse/build_config.rs): wire query diagnostics into `Config.ask_diagnostics`.
- [crates/cli/commands/query.rs](/home/jmagar/workspace/axon_rust/crates/cli/commands/query.rs): print typed diagnostics to stderr before returning query failures.
- [crates/cli/commands/ask.rs](/home/jmagar/workspace/axon_rust/crates/cli/commands/ask.rs): print typed diagnostics to stderr before returning ask failures.
- [crates/mcp/server/common.rs](/home/jmagar/workspace/axon_rust/crates/mcp/server/common.rs): preserve diagnostics in sanitized MCP internal errors.
- [crates/web/execute/sync_mode/service_calls.rs](/home/jmagar/workspace/axon_rust/crates/web/execute/sync_mode/service_calls.rs): preserve diagnostics in sanitized web service errors and websocket command-error payloads.
- [tests/query_diagnostics_error_contract.rs](/home/jmagar/workspace/axon_rust/tests/query_diagnostics_error_contract.rs): new integration-level CLI diagnostics contract test.

# Critical Commands Executed And Outcomes

- `git status --short` | confirmed a dirty worktree with many unrelated edits already present.
- `cargo check -q` | initially failed with lifetime and trait-object sizing errors introduced by typed diagnostics plumbing; later passed after helper signature fixes.
- `cargo fmt` | passed.
- `cargo test --test query_diagnostics_error_contract -- --nocapture` | passed; black-box query diagnostics contract held.
- `cargo test query_reports_typed_diagnostics_payload_when_enabled -- --nocapture` | passed; service-layer typed diagnostics payload test held.
- `git diff -- ...` on the touched files | confirmed the current-session changes summarized in this document.

# Behavior Changes

- Before: Qdrant mode-probe failures could degrade query/ask to dense-only unnamed mode. After: retryable failures retry, `404` reports collection-not-found, and non-success probe failures bubble up as explicit errors.
- Before: query diagnostics were not wired through a dedicated `query --diagnostics` CLI flag. After: query supports `--diagnostics` and emits structured diagnostics to stderr on failure.
- Before: diagnostics metadata was primarily embedded in error strings. After: diagnostics can travel as typed JSON payloads through service, MCP, and web error handling while retaining sanitized user-facing messages.
- Before: query error diagnostics behavior was not locked by an integration-level CLI test. After: the contract is enforced by [tests/query_diagnostics_error_contract.rs:5](/home/jmagar/workspace/axon_rust/tests/query_diagnostics_error_contract.rs#L5).

# Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo check -q` | library compiles | passed after helper signature fixes | PASS |
| `cargo fmt` | formatting succeeds | passed | PASS |
| `cargo test --test query_diagnostics_error_contract -- --nocapture` | CLI emits diagnostics on query failure with `--diagnostics` | `1 passed; 0 failed` | PASS |
| `cargo test query_reports_typed_diagnostics_payload_when_enabled -- --nocapture` | service error carries structured diagnostics when enabled | `1 passed; 0 failed` for the targeted test | PASS |
| `./scripts/axon status --json` | preflight status output | failed with `snap-confine has elevated permissions...` in the sandboxed wrapper path | FAIL |
| `./scripts/axon embed "docs/sessions/2026-03-20-query-diagnostics-and-graph-followup.md" --json` | queued embed job JSON with `data.job_id` | failed with `snap-confine has elevated permissions...` in the sandboxed wrapper path | FAIL |
| `cargo run --bin axon -- status --json` | preflight status output | failed to compile due to `crates/services/export.rs` missing fields in `ExportManifest`, `QuerySeedExport`, `ExtractionSeedExport`, and `RebuildSeedsExport` initializers | FAIL |
| `cargo run --bin axon -- embed "docs/sessions/2026-03-20-query-diagnostics-and-graph-followup.md" --json` | queued embed job JSON with `data.job_id` | failed with the same `crates/services/export.rs` compile errors as `status` | FAIL |
| `cargo run --bin axon -- --collection unavailable retrieve unavailable` | retrieve using status-derived source ID and collection | attempted only because embed produced no source metadata; compile failed earlier in `crates/services/export.rs`, so retrieve could not reach argument/runtime validation | FAIL |

# Source IDs + Collections Touched

- Session log file source ID: unavailable; embed did not start because `cargo run --bin axon` failed to compile.
- Session log file collection: unavailable; no embed status output was produced.
- Outcome: Axon embed failed before job creation; retrieve was attempted separately but could not proceed past the same compile break.
- No other source IDs or collections were observed directly in command output during this save operation.

# Risks And Rollback

- The repo worktree contains many unrelated modified files; rollback of this session’s changes should be limited to the files listed in this document.
- Fail-fast probe behavior intentionally changes operational behavior: transient Qdrant probe failures now surface as query/ask errors rather than dense-only fallback.
- If typed diagnostics propagation causes issues on a given surface, rollback can be limited to the diagnostics carrier/plumbing files while keeping probe retry and `404` tests intact.
- If `axon-ollama` conflicts with local runtime assumptions, rollback is isolated to [docker-compose.services.yaml](/home/jmagar/workspace/axon_rust/docker-compose.services.yaml).

# Decisions Not Taken

- Session docs were not retroactively edited as implementation artifacts; the user explicitly said session docs are not updated.
- The fail-fast probe change was not reverted back to dense fallback.
- No claim is made here that `qwen3.5:4b` pull completed; that was requested in the conversation, but completion was not verified by command output captured during this save step.
- No claim is made here that graph ingestion internals use `PreparedDoc`; that question was asked in the session, but this save document only records verified facts.

# Open Questions

- Was `qwen3.5:4b` successfully pulled into the new Ollama service on this machine? No confirming command output was captured in this save step.
- Which exact source IDs and collection values will Axon return for this session log embed once the current `crates/services/export.rs` compile break is fixed?
- Several unrelated diffs in `crates/core/config/parse/build_config.rs` and other files were present in the worktree; their rationale is not reconstructed here unless discussed and verified in this session thread.

# Next Steps

- Fix the current `crates/services/export.rs` compile errors, then rerun Axon `status`, `embed`, `embed status`, and `retrieve` for this session log.
- Persist session knowledge into Neo4j memory with file, service, feature, bug, technology, and concept entities plus relevant relations.
- If desired, verify the Ollama runtime on this machine and confirm whether `qwen3.5:4b` is present.
