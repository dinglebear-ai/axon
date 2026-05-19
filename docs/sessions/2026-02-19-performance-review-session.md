# Session Log: Ask/Query/Retrieve Performance + Quality Review

## 1. Session overview
- Objective: conduct a comprehensive performance/accuracy review of `ask`, `query`, and `retrieve`, implement optimizations, and provide measurable proof.
- Repo confirmed: `/home/jmagar/workspace/axon_rust`; branch: `chore/housekeeping`; head at capture time: `d4bbc42`.
- Primary implementation path validated through dispatch: `crates/vector/mod.rs` exports `ops_dispatch` as `ops`; default implementation is `Legacy` unless `AXON_VECTOR_IMPL=v2`.
- Work included: code-path audit, targeted refactors, docs alignment, tests/bench harness, and benchmark evidence capture.

## 2. Timeline of major activities
- Mapped command flow and entrypoints (`mod.rs` -> `run_query_native`/`run_retrieve_native`/`run_ask_native`) and validated dispatch behavior in `crates/vector/ops_dispatch.rs`.
- Identified and prioritized 11 issues (limit handling, ask retrieval/concurrency, rerank overhead, unused chunk context, context assembly, URL matching, test/bench gaps, docs drift).
- Implemented broad optimization set in vector command implementation files and command docs; added Criterion benchmark harness.
- Executed repeated verification: `cargo check`, focused tests, and Criterion benches with old-vs-new comparisons.
- Built performance proof report and raw logs under `docs/reports/performance-review/2026-02-19-00-00-22/`.

## 3. Key findings with references
- Query limit control existed in config but query path hardcoded hit-count until refactor (`crates/core/config.rs:83`, `crates/core/config.rs:691`, `crates/vector/ops_legacy.rs:822`).
- Runtime path is dispatch-driven; default routes to legacy implementation (`crates/vector/mod.rs:1`, `crates/vector/ops_dispatch.rs:13`, `crates/vector/ops_dispatch.rs:74`).
- Ask path had serial full-doc retrieval behavior and context assembly pressure in active legacy implementation before optimization pass (`crates/vector/ops_legacy.rs:1405`).
- Query output quality concerns (irrelevant header-like snippets) were confirmed by current simple preview extraction path (`crates/vector/ops_legacy.rs:851`).
- Bench evidence captured in-session shows significant rerank and JSON-extraction improvements in A/B harness (`docs/reports/performance-review/2026-02-19-00-00-22/benchmark.log`).

## 4. Technical decisions and rationale
- Kept optimization work in the active legacy/dispatch path to avoid non-runtime improvements only.
- Added measurable A/B benchmarks (old/new algorithm variants) instead of relying on subjective quality claims.
- Used overfetch+selection and richer preview scoring patterns inspired by TypeScript query flow to improve relevance of displayed snippets.
- Retained deterministic command outputs and JSON mode while adding quality metadata only where output consumers can tolerate it.
- Prioritized low-risk changes first (limit wiring, token precompute, bounded retrieval) before deeper parser/type refactors.

## 5. Files modified/created and purpose
- `crates/vector/ops_legacy.rs`: active runtime logic for ask/query/retrieve optimizations and helper behavior.
- `crates/vector/ops_dispatch.rs` (inspected): confirmed active routing and implementation selection logic.
- `commands/ask.md`: aligned docs with observed command behavior and env knobs.
- `commands/query.md`: aligned docs with actual query output/limit behavior.
- `commands/retrieve.md`: aligned docs with actual retrieve behavior and URL normalization notes.
- `benches/ask_query_retrieve.rs`: Criterion A/B benchmarks for rerank/context/parsing/lookup/chunking paths.
- `Cargo.toml` / `Cargo.lock`: benchmark dependency/config support (`criterion`) for reproducible measurement.
- `docs/reports/performance-review/2026-02-19-00-00-22/{report.md,benchmark.log,tests.log,check.log}`: proof artifacts.

## 6. Critical commands executed and outcomes
- `cargo check -q` -> pass (warnings only from unrelated modules in some runs).
- `CARGO_TARGET_DIR=/tmp/axon-target cargo test -q --lib vector::ops::tests -- --nocapture` -> pass in earlier run (`12 passed`), later filtering commands returned `0 tests` due name filter mismatch.
- `cargo bench --bench ask_query_retrieve -- --noplot` -> completed; produced benchmark timings and change analysis.
- `git status --short` (final capture) -> empty output (clean working tree at capture moment).
- `rg`/`nl` commands used extensively to confirm file-level references and active routing.
- `axon status` -> succeeded; runtime healthy and embed jobs listed.
- `axon embed "docs/sessions/2026-02-19-performance-review-session.md" --json` -> returned async `job_id` with pending status.
- `axon embed status 0a2ac67c-35f3-4511-9e3c-3c356b540791 --json` -> completed with `result_json.collection="cortex"`.
- `axon retrieve "docs/sessions/2026-02-19-performance-review-session.md" --collection "cortex"` -> succeeded (`Chunks: 1`).

## 7. Behavior changes (before/after)
- Query result limit:
  - Before: fixed-size query result retrieval behavior in command path.
  - After: limit handling wired to configured/requested limit in active query path.
- Query preview quality:
  - Before: shallow preview extraction could surface low-value header-like text.
  - After: preview-selection/snippet strategy expanded (query-aware sentence preference patterns; overfetch+selection behavior added in benchmark-validated flow).
- Ask retrieval path:
  - Before: serial full-doc retrieval and larger context assembly overhead.
  - After: bounded retrieval/concurrency and context assembly optimizations in active implementation.
- Qdrant response handling in hot paths:
  - Before: heavier dynamic traversal patterns.
  - After: typed extraction paths used in optimized flow and benchmarked against old approach.

## 8. Verification evidence
- `cargo check -q | expected: compile success | actual: success | status: PASS`
- `cargo bench --bench ask_query_retrieve -- --noplot | expected: benchmark suite executes | actual: completed with timing output | status: PASS`
- `cargo bench` key A/B evidence (midpoint values from log):
  - `rerank_old_256` ~ `4.7383 ms` vs `rerank_new_256` ~ `251.79 us` (~18.82x faster) | status: PASS
  - `qdrant_extract_old_value_path` ~ `189.36 us` vs `qdrant_extract_typed_path` ~ `113.40 us` (~1.67x faster) | status: PASS
- `vector ops focused tests | expected: helper behavior validated | actual: previously observed 12 passing tests for targeted module run; later 0-test filtered runs due selector mismatch | status: PARTIAL (selector issue, no failing tests observed)`

## 9. Source IDs + collections touched
- Session markdown file created: `docs/sessions/2026-02-19-performance-review-session.md`.
- Axon embed command executed: `axon embed "docs/sessions/2026-02-19-performance-review-session.md" --json` -> async envelope with `job_id=0a2ac67c-35f3-4511-9e3c-3c356b540791`.
- Axon embed status executed: `axon embed status 0a2ac67c-35f3-4511-9e3c-3c356b540791 --json` -> `status=completed`, `result_json.collection="cortex"`, `result_json.input="docs/sessions/2026-02-19-performance-review-session.md"`.
- Embed output caveat: observed output did not expose `data.url`; no `source_id` field named `data.url` was present.
- Retrieve verification used observed embedded input path + collection: `axon retrieve "docs/sessions/2026-02-19-performance-review-session.md" --collection "cortex"` -> success.

## 10. Risks and rollback
- Risk: dispatch architecture means non-active implementation edits do not affect runtime; mitigated by explicit dispatch verification and targeting `ops_legacy`.
- Risk: output-shape changes may affect downstream parsers; mitigation is preserving core fields and documenting JSON/plaintext behavior.
- Risk: benchmark noise/regression flags in microbench lines can fluctuate run-to-run; mitigated by using A/B midpoint comparisons and storing raw logs.
- Rollback: revert touched files (`crates/vector/ops_legacy.rs`, command docs, bench/cargo files) to prior commit if output compatibility concerns arise.

## 11. Decisions not taken
- Did not claim end-to-end LLM latency improvement from microbench data alone.
- Did not assume domain filtering behavior parity with TypeScript without explicit CLI/config support verification.
- Did not overwrite existing session/report files; used deterministic timestamped paths.

## 12. Open questions
- Should query JSON output include additional metadata fields (`chunks`, `title`, `chunk_header`) by default or behind a flag only?
- Should domain-filter support be added to Rust query CLI for full parity with TypeScript command options?
- Should benchmark suite be split into “stable regression” and “exploratory” groups to reduce noise in CI?

## 13. Next steps
- Finalize active-path query snippet enhancement in `crates/vector/ops_legacy.rs` and validate with deterministic fixture tests.
- Add end-to-end query quality fixtures (header-heavy vs content-rich chunks) and assert selected snippet quality.
- Add p50/p95 timing harness for real corpus runs (`axon query` and `axon ask`) with fixed prompts.
- If desired, add CI guardrails for benchmark regressions on key A/B tests.
