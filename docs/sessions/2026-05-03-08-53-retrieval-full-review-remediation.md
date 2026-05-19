# 2026-05-03 Retrieval Full Review Remediation

## Repo Snapshot

- Repo: `/home/jmagar/workspace/axon_rust`
- Branch: `obs/p0-tracing-bundle`
- HEAD at start of remediation: `ab5c12a8fc8efc0f885873aeca30e624cffcc5f0`
- Scope: retrieval code only: URL retrieve, MCP retrieve, Qdrant search/retrieve helpers, retrieve docs.
- Worktree: shared and dirty before this work started. Do not assume every modified file belongs to this session.

## User Request

The session started with:

```text
comprehensive:full-review
scope to retrieval code
```

The user then asked to continue, create Beads for all final-report issues, and run `lavra-work em all`.

## Review Artifacts

Generated comprehensive review artifacts under `.full-review/`:

- `.full-review/00-scope.md`
- `.full-review/01-quality-architecture.md`
- `.full-review/02-security-performance.md`
- `.full-review/03-testing-documentation.md`
- `.full-review/04-best-practices.md`
- `.full-review/05-final-report.md`

The final report found 11 retrieval-scoped issues: Qdrant collection validation gaps, missing URL retrieve regression coverage, MCP retrieve parity gaps, vector search retry behavior, stale retrieve docs, and duplicated Qdrant endpoint/validation policy.

## Beads Created And Closed

Created epic:

- `axon_rust-s7n` - `[EPIC] Retrieval scoped full-review remediation (2026-05-03)`

Created and later closed all 11 child Beads:

- `axon_rust-s7n.1` - Validate Qdrant collection names on URL retrieve and full-doc fetch paths
- `axon_rust-s7n.2` - Add URL retrieve regression tests for unsafe collection names
- `axon_rust-s7n.3` - Unify URL retrieval with shared Qdrant validation boundary
- `axon_rust-s7n.4` - Add MCP retrieve collection and time-filter parity with query and ask
- `axon_rust-s7n.5` - Retry transient Qdrant failures for vector search POSTs
- `axon_rust-s7n.6` - Update vector search tests for retry-once-then-success behavior
- `axon_rust-s7n.7` - Update retrieve command docs to remove stale Postgres Redis AMQP requirements
- `axon_rust-s7n.8` - Consolidate MCP and vector-dispatch collection validation policy
- `axon_rust-s7n.9` - Centralize Qdrant endpoint construction for retrieval paths
- `axon_rust-s7n.10` - Allow MCP retrieve to preserve filtered query data-isolation semantics
- `axon_rust-s7n.11` - Refresh retrieve docs after MCP parity and Qdrant boundary fixes

Final Beads state:

- `bd show axon_rust-s7n --json` reported the epic `closed`.
- `epic_total_children`: 11
- `epic_closed_children`: 11
- `bd ready --json | rg 's7n'` returned no remaining ready work.

`bd dolt commit -m "Close retrieval full-review remediation epic"` succeeded.

`bd dolt push` was attempted and failed because no Dolt remote is configured:

```text
fatal: remote 'origin' not found
```

`bd dolt remote list` returned:

```text
No remotes configured.
```

## Implementation Summary

### Qdrant Boundary

Added a shared Qdrant collection boundary in `crates/vector/ops/qdrant/utils.rs`.

Key behavior:

- `validate_collection_name()` rejects empty names, names over 255 chars, leading/trailing dots, `.` and `..`, embedded `..`, path separators, query/fragment delimiters, spaces, and percent-encoded traversal-like strings.
- `validate_config_collection()` turns collection validation failures into contextual `anyhow` errors.
- `qdrant_collection_endpoint()` validates the collection before constructing `/collections/{collection}/...` endpoints and trims duplicate suffix slashes.
- Existing delete/scroll/facet/retrieve Qdrant paths now use the shared endpoint helper.

Primary files:

- `crates/vector/ops/qdrant/utils.rs`
- `crates/vector/ops/qdrant/client.rs`
- `crates/vector/ops/qdrant/commands/dispatch.rs`
- `crates/vector/ops/qdrant.rs`

### URL Retrieve

Hardened URL retrieve and full-doc fetch paths.

Key behavior:

- `qdrant_retrieve_by_url()` rejects unsafe collection names before endpoint construction or network I/O.
- Full-doc ask context fetches benefit from the same path because they call URL retrieve.
- Added regression coverage for path traversal, path separators, query/fragment delimiters, leading/trailing dots, embedded `..`, percent-encoded traversal-like names, and oversize names.

Primary files:

- `crates/vector/ops/qdrant/client.rs`
- `crates/vector/ops/qdrant/tests.rs`
- `crates/vector/ops/qdrant/utils.rs`

### MCP Retrieve Parity

Added MCP retrieve parity with query and ask request-local controls.

Key behavior:

- `RetrieveRequest` now accepts `collection`, `since`, and `before`.
- `handle_retrieve()` clones the server config per request, applies the validated collection override, and applies time filters before calling the shared retrieve service.
- MCP collection validation now reuses the shared Qdrant collection-name policy instead of a separate MCP-only rule.
- Schema test coverage proves retrieve requests with collection/time filters parse correctly.

Primary files:

- `crates/mcp/schema.rs`
- `crates/mcp/schema/tests.rs`
- `crates/mcp/server/common.rs`
- `crates/mcp/server/handlers_query.rs`

### Qdrant Search Retry Behavior

Added shared retry behavior for vector search POSTs.

Key behavior:

- `qdrant_post_json_with_retry()` retries transient Qdrant 429, 5xx, and transport failures with the existing exponential backoff and jitter shape.
- Legacy dense search, named dense search, and hybrid search use the shared retry helper.
- Non-retryable 4xx responses still fail fast.
- Retry tests use one stateful `httpmock` responder per test so the first request returns 500/429 and the second returns success deterministically.

Primary files:

- `crates/vector/ops/qdrant/search.rs`
- `crates/vector/ops/qdrant/hybrid.rs`
- `crates/vector/ops/qdrant/utils.rs`

### Retrieve Documentation

Refreshed retrieve command docs.

Key behavior:

- Removed stale Postgres, Redis, and AMQP required-environment claims.
- Documented that URL retrieve reads from Qdrant and does not call TEI.
- Documented default Lite-mode dependency behavior.
- Added `--since` and `--before` examples.
- Added a note that MCP `retrieve` accepts `collection`, `since`, `before`, `max_points`, and `response_mode`.

Primary file:

- `docs/commands/retrieve.md`

## Verification Evidence

Fresh checks:

```bash
cargo fmt
cargo check --lib
cargo check --lib --no-default-features
cargo test collection_name --lib --no-default-features
cargo test qdrant_collection_endpoint_validates_and_trims_suffix --lib --no-default-features
cargo test parse_retrieve_action_with_collection_and_time_filters --lib --no-default-features
cargo test qdrant_search_recovers_after_retryable_500 --lib --no-default-features
cargo test qdrant_named_dense_search_recovers_after_retryable_500 --lib --no-default-features
cargo test qdrant_hybrid_search_recovers_after_retryable_429 --lib --no-default-features
cargo test qdrant_retrieve_by_url_rejects_invalid_collection_before_scroll --lib --no-default-features
cargo test qdrant_search_fails_fast_on_non_retryable_400 --lib --no-default-features
cargo test qdrant_hybrid_search_fails_fast_on_non_retryable_400 --lib --no-default-features
```

Observed results:

- `cargo fmt`: passed.
- `cargo check --lib`: passed.
- `cargo check --lib --no-default-features`: passed.
- Focused tests listed above passed.

Default-feature `cargo test collection_name --lib` was attempted and blocked by unrelated existing dirty ingest/lite-worker compile errors before retrieval tests ran. The observed errors included:

```text
future cannot be sent between threads safely
crates/jobs/lite/workers.rs:144:42
trait std::marker::Send is not implemented for dyn std::error::Error
```

A later focused MCP validation test under `--no-default-features` forced recompilation and hit another unrelated dirty lite-ingest signature mismatch:

```text
this function takes 3 arguments but 4 arguments were supplied
crates/jobs/lite/workers.rs:293:26
run_ingest_job_lite(&pool, &cfg, id, Some(cancel_token))
```

Those blockers are outside the retrieval scope and came from unrelated dirty ingest/lite-worker files.

## Current Worktree Caveat

The worktree remains dirty and includes many modifications that predated this scoped retrieval remediation. This session did not commit or push, because staging everything would risk bundling unrelated work.

Files intentionally touched for this scoped run include:

- `.full-review/00-scope.md`
- `.full-review/01-quality-architecture.md`
- `.full-review/02-security-performance.md`
- `.full-review/03-testing-documentation.md`
- `.full-review/04-best-practices.md`
- `.full-review/05-final-report.md`
- `crates/mcp/schema.rs`
- `crates/mcp/schema/tests.rs`
- `crates/mcp/server/common.rs`
- `crates/mcp/server/handlers_query.rs`
- `crates/vector/ops/qdrant.rs`
- `crates/vector/ops/qdrant/client.rs`
- `crates/vector/ops/qdrant/commands/dispatch.rs`
- `crates/vector/ops/qdrant/hybrid.rs`
- `crates/vector/ops/qdrant/search.rs`
- `crates/vector/ops/qdrant/tests.rs`
- `crates/vector/ops/qdrant/utils.rs`
- `docs/commands/retrieve.md`

This handoff note was added at:

- `docs/sessions/2026-05-03-08-53-retrieval-full-review-remediation.md`

## Open Questions

- The Beads Dolt backend has no configured remote, so `bd dolt push` cannot succeed until a remote is added with `bd dolt remote add`.
- The unrelated ingest/lite-worker dirty changes should be resolved before using default-feature `cargo test --lib` as a clean repo-level gate.
