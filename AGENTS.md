# Axon Agent Instructions

## Architecture and ownership

CLI, MCP, and HTTP/web are projections over shared Rust services. Source work
follows one pipeline: resolve -> acquire -> ledger -> prepare -> embed ->
publish -> graph/cleanup.

DTOs belong in `axon-api`, domain logic in its crate, orchestration in
`axon-services`. Transports must not import domain-internal `::ops::*`.

Acquisition belongs to `axon-adapters`, preparation/chunking to `axon-document`,
embedding to `axon-embedding`, publication to `axon-vectors`, and queries to
`axon-retrieval`. `axon-ledger` owns source identity, generations, manifests,
document status, and cleanup debt; `axon-jobs` owns execution and reservations.

Read [crate ownership](docs/architecture/crate-ownership.md), the
[crate map](docs/architecture/crate-structure.md), and
[source pipeline](docs/architecture/source-pipeline.md).

## Adding ingestion sources

Reuse `SourceAdapter` and existing local/upload, web/feed, registry, CLI-tool,
or MCP-tool acquisition when the input fits. Do not add a parallel pipeline
for a new provider name.

Register `SourceAdapterSpec` in the family matrix: identity, scopes, options,
auth, parsing, chunking, metadata, and graph facts. `onboarding_status()`
checks declarations, not behavior.

Emit `SourceDocument` values, not persisted chunks, graph rows, vectors, or
transport responses. Use `materialize` for a consistent snapshot; clean
temporary data after failure and cancellation too.

Test added/modified/removed/unchanged items; stable hashes skip re-embedding.
Infer deletions only from complete snapshots or explicit tombstones, not
truncated pages or interrupted acquisition. Bound pagination and output.

CLI/MCP execution requires authorization, allowlists, timeouts, output caps,
and redaction; metadata discovery is not execution permission. Enforce SSRF
and filesystem access boundaries. See
[adapter onboarding](docs/development/adding-source-adapter.md),
[new sources](docs/development/adding-source.md),
[parsers](docs/development/adding-parser.md), and
[metadata](docs/reference/sources/metadata-payload.md).

## Jobs, retrieval, and storage

Keep one source job ID across stages. Watches and scrape/crawl/embed/ingest
projections reuse source jobs, not per-family queues or child embedding handoffs.

SQLite stores durable state; workers run in-process. A detached job needs
active workers; a job ID is not completion. Use `jobs` for lifecycle and
`--wait true` for foreground completion. Test cancellation, heartbeats,
recovery, and retries when changing orchestration.

Queries must respect committed generations and source/path/content filters.
Test refresh/removal, not just insertion. Retries/cooling stay with embedding
providers, upserts with `axon-vectors`, and cleanup debt in the ledger.

Use prune/reset plans and confirmation for destructive cleanup. Add SQLite
migrations instead of rewriting applied ones; test existing-state upgrades.

Read [jobs](docs/reference/runtime/jobs.md),
[ledger](docs/reference/runtime/ledger.md),
[pruning](docs/reference/runtime/pruning.md), and
[RAG](docs/guides/ask-rag.md) before changing those boundaries.

## Agent-friendly diagnostics

ALL errors, warnings, degraded results, and diagnostics must be AGENT-FRIENDLY:
enable diagnosis and course-correction. No bare "failed" messages, swallowed
exceptions, or unexplained fallbacks.

Use `axon-error` taxonomy and `ApiError` context consistently across
CLI/REST/MCP and job events. Keep stable codes and typed causes; do not
flatten provider failures into unclassified strings.

Include operation/stage, severity, cause, affected entity/provider, relevant
job/source/item IDs, and safe expected-versus-observed details. State
retryability, scope, delay, and prerequisites. Report completed work, possible
side effects, and whether repetition is safe. Explicitly mark unknown commit
status rather than inviting duplicate writes.

Give concrete recovery actions: fix a named argument/key, authorize a provider,
reduce a batch, inspect a job, or resume a stage. Name constraints without
exposing secrets.

Warnings must explain impact and the actual fallback. Distinguish partial
success, skipped work, degradation, and failure. Never hide a failure behind
a successful empty result.

Structured data goes to stdout, logs to stderr; correlate via `axon-observe`.
Test diagnostic fields, redaction, and recovery guidance, not merely that an
error occurred. Follow
[error contracts](docs/pipeline-unification/runtime/error-handling.md) and
[observability](docs/reference/runtime/observability.md).

## Configuration and deployment

Axon precedence is CLI > environment > TOML > defaults. Change config/env
values required by the task; retain unrelated keys and credentials. Do not
replace a configured file with a template. Explain renames/removals and
migration steps.

Verify effective values and identify required reloads/restarts. Test CLI,
environment, TOML, and default behavior; update parsing and contract coverage.

The supported deployment contract is native `axon serve` under systemd in
Incus or bare-metal Linux, with external Qdrant, TEI, and Chrome/CDP providers.
Compose is a development/reference surface. Actual installations can differ:
consult verified local deployment notes before choosing lifecycle commands.

Non-loopback HTTP requires `AXON_HTTP_TOKEN` or OAuth. Panel password/session
authorization is separate from API/MCP authentication; do not substitute API
tokens for panel unlocks. Test auth and allowed-origin checks across transports.

Use [configuration](docs/guides/configuration.md),
[deployment](docs/operations/deployment.md), and
[security](docs/operations/security.md) for these changes.

## MCP, REST, and plugins

Inspect `crates/axon-mcp/src/server.rs`: the primary action schema is not the
entire catalog. Include dashboard, resource, and task behavior in transport changes.

System/watch requests have separate MCP types; absence from the primary
action enum does not mean unsupported.

Align discovery, dispatch, schemas, authorization, tasks, and envelopes.
Return opaque artifact IDs, not server paths. Test real `tools/list` and calls
against a matching build, including invalid-input and auth failures.

`plugins/axon/` ships no binary; its manifest must not gain a `version` key.
Repository hooks do not imply automatic plugin provisioning.
Read [MCP development](docs/development/adding-mcp-action.md),
[REST routes](docs/development/adding-rest-route.md),
[Codex control](docs/guides/codex-control.md), and
[plugin guidance](plugins/axon/AGENTS.md).

## Development gates

No `mod.rs`. Preserve test sidecar names, cfg gates, and selectors;
`cargo check` does not compile them. Follow
[contributing](docs/development/contributing.md).

Rust files cap at 500 lines; functions warn at 80, fail at 120.
Respect checker exemptions and `.monolith-allowlist`.

    cargo build --bin axon
    cargo test -p <affected-crate> <test-filter>
    cargo fmt --all -- --check
    cargo xtask check-layering
    cargo xtask generated-contracts refresh
    cargo xtask generated-contracts check

Regenerate changed contracts, schemas before Markdown; test failure paths.
Use `just verify` for broad integration; for prose, use
`python3 scripts/test_operational_docs.py` and link checks.
See [testing](docs/development/testing.md) and
[documentation checks](docs/development/documentation.md).
