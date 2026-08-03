---
title: "Testing"
created: 2026-02-26
updated: 2026-08-02
---

# Testing

Axon uses layered tests: domain units, fake-boundary contracts, service
orchestration, transport parity, durable runtime behavior, generated-contract
checks, and selected live provider smoke tests.

## Standard commands

```bash
cargo test --locked --workspace
cargo nextest run --locked --workspace
cargo clippy --all-targets --locked -- -D warnings
just precommit
```

Use the narrowest focused test while developing, then run the full repository
gate before pushing.

## Test locations

| Area | Common location |
|---|---|
| API DTOs and schemas | `crates/axon-api/src/**/*tests.rs` |
| HTTP safety/config/redaction | `crates/axon-core/src/**/*tests.rs` |
| Authorization | `crates/axon-authz/src/**/*tests.rs` |
| Source adapters/web engine | `crates/axon-adapters/src/**/*tests.rs` |
| Parsing/preparation | `crates/axon-parse/src/`, `crates/axon-document/src/` |
| Jobs/scheduler/watches | `crates/axon-jobs/src/**/*tests.rs` |
| Source orchestration | `crates/axon-services/src/source*tests.rs` and module tests |
| MCP | `crates/axon-mcp/src/**/*tests.rs`, `crates/axon-mcp/tests/` |
| REST/OpenAPI/server | `crates/axon-web/src/**/*tests.rs`, root parity tests |
| CLI | `crates/axon-cli/src/**/*tests.rs` |
| Generated contracts | `xtask/src/**/*tests.rs`, `xtask/tests/fixtures/` |
| Cross-surface contracts | root `tests/` and `tests/fixtures/cross-surface/` |

## Focused examples

```bash
cargo test -p axon-adapters web::acquire::tests --lib -- --nocapture
cargo test -p axon-jobs scheduler --lib -- --nocapture
cargo test -p axon-services source_web_reuse_tests --lib -- --nocapture
cargo test -p axon-mcp --lib
cargo test -p axon-web --lib
cargo test -p xtask generated_schema_inputs_never_reference_generated_artifacts
```

## Fake boundaries

External providers and stores should have deterministic fakes covering:

- success and empty results
- timeout/retry/cooling
- malformed provider responses
- partial/degraded completion
- authorization denial
- cancellation and recovery
- publication or commit failure

Prefer injecting a fake boundary over mocking an internal implementation detail.

## Durable job tests

Job tests must verify state transitions and durable side effects, not only
returned values. Important cases include:

- claim exclusivity
- attempt append on retry
- heartbeat and stale recovery
- cancellation observation
- provider reservations and cooling
- completed-degraded semantics
- immutable authorization snapshots
- watch execution through the same job store
- no second per-family queue or lifecycle

## Source pipeline tests

Source tests should distinguish:

- acquisition output
- manifest diff and generation reuse
- normalization/preparation
- embedding/vector writes
- publication and rollback
- graph and cleanup debt
- warnings, artifacts, events, and counts

Cache reuse and conditional refetch tests must assert exact warning counts and
preservation of refetch artifacts/content.

## Property tests

Current examples include:

- `crates/axon-core/src/http/proptest_tests.rs`: URL/SSRF inputs
- `crates/axon-adapters/src/web_engine/engine/url_utils_proptest_tests.rs`:
  discovered URL filtering and normalization

Property tests should be deterministic and bounded for CI.

## Transport tests

When a public operation changes, update and verify:

- CLI command registry/help snapshots
- MCP schema and golden fixture
- REST/OpenAPI route and schema inventories
- cross-surface operation/scope matrices
- auth and error-envelope behavior
- generated app bindings when their source contract changes

## Generated documentation and schema tests

```bash
cargo xtask schemas generate --check
cargo xtask docs generate --check
cargo xtask docs check
cargo xtask presentation generate --check
python3 scripts/generate_action_docs.py --check
cargo xtask check-public-api
cargo xtask check-dep-graph
```

Use `--update-fixtures` only for an intentional contract change and review
every generated diff.

## Network and live tests

Tests are offline by default. Live tests must be explicitly gated, bounded, and
safe to rerun. Never make ordinary unit tests depend on Qdrant, TEI, Chrome,
GitHub, or public network availability.

## Test-only escape hatches

Some HTTP tests use a test-only loopback allowance or local mock server. These
must remain scoped to test builds and must not weaken production SSRF behavior.

## Completion standard

A change is not complete until focused tests, formatting, warning-denied
Clippy, generated-contract checks, the full workspace suite, and remote CI all
pass for the final commit.
