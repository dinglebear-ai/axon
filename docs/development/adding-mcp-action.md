---
title: "Adding an MCP Action"
created: 2026-07-07
updated: 2026-09-28
---

# Adding an MCP action

Axon has one canonical MCP dispatcher and three startup-static projections:
`legacy` (the default `axon` aggregate), `atomic` (unprefixed leaf operations),
and `both`. A new action must not create a second dispatcher or an independently
maintained tool-name list. See the [MCP overview](../reference/mcp/overview.md),
[crate guide](../../crates/axon-mcp/src/AGENTS.md), and
[service boundary](../pipeline-unification/surfaces/tool-contract.md).

## 1. Define the actual request and finite selectors

Reuse the shared `axon-api` request DTO when it expresses the MCP contract.
Transport-only envelopes and intentionally different MCP requests belong in
`crates/axon-mcp/src/schema.rs` or `server/system_requests.rs`, not in another
copy of the shared domain model. Derive `JsonSchema` from the real deserialized
type. Use a serde enum for a finite `subaction` family. Do not invent a separate
list of strings merely to generate tools.

Add the action to `AxonRequest` or `McpSystemRequest`. Direct actions project as
`query`; a finite family projects as `jobs_get` and `jobs_cancel`. The namespace
is supplied by the client/gateway, so do not add an `axon_` prefix. The existing
`axon_status_dashboard` is an explicitly preserved auxiliary route, not a leaf
naming example.

Legacy string-backed selectors must be validated against a canonical local enum
at the real route boundary before the same enum is used for schema projection.
See `PruneSubaction` and `ProvidersSubaction` in `server/system_requests.rs`.
Unbounded selectors fail construction rather than advertising invented leaves.

## 2. Route through services once

Put the handler in the existing domain-focused `server/handlers_*.rs` module.
It builds/reuses `ServiceContext`, calls the appropriate `axon-services`
entrypoint, maps errors to the shared envelope, and returns the existing MCP
response shape. It must not reach into provider/database internals or duplicate
business logic. Add the exhaustive request-variant arm to the existing dispatch
path in `server.rs`.

All tool projections reuse that handler. `server/projection.rs` clones the
canonical aggregate route's handler and attaches the focused leaf schema.
`server/projection_call.rs` resolves the presented leaf to canonical routing
fields before authorization, tasks or side effects. Never add handwritten
`#[tool]` wrappers for each leaf, and never call aggregate MCP internally.

## 3. Extend the live action metadata and policy

Add one entry to `MCP_ACTION_SPECS` in `server/action_specs.rs` (re-exported from
`server/authz.rs`). Supply its name, real request-schema callback, DTO label,
description, cost, coarse policy scope and asynchronous-job behavior. This is
the runtime allowlist; `schema_registry.rs` and xtask import it instead of
maintaining another inventory.

Review `required_scope_for` and the existing explicit-elevation helper in
`server/authz.rs`. Ordinary read/write compatibility is intentional; explicit
write elevation for `search` and `research` must not be weakened to the ordinary
scope check. Preserve admin scope and confirmation as separate checks. Do not
replace caller-derived prune/reset/memory contexts or remote visibility ceilings
with trusted-system values.

Review leaf effect hints in `server/operations.rs`. Read-only, destructive and
idempotent hints are not synonymous with scope and never grant permission.
Mixed-safety families must have honest per-leaf hints. Plans that persist state
are not read-only merely because execution requires later confirmation.

A durable job does not automatically support MCP tasks. Update
`tasks::supports_operation` only alongside a real task lifecycle implementation.
It is shared by operation metadata and admission. Keep task-capability refusal
before enqueue and preserve request metadata, progress tokens and caller identity.

## 4. Let the canonical schema generate the projections

`server/tool_schema.rs` compares the typed request variants with the runtime
allowlist in both directions. It invokes each real request-schema callback.
`server/operation_schema.rs` specializes fixed action/subaction constraints while
preserving unions, required fields, nested payloads and reachable definitions.
Missing branches, unresolved references and duplicate/colliding names fail
construction. Do not introduce an empty-schema fallback or merge alternatives
into an incorrectly permissive property list.

`server/operations.rs` is the canonical leaf descriptor view. It drives tool
routes, help, the operation resource and generated schema exports. Legacy
`axon://schema/mcp-tool` remains compatible;
`axon://schema/mcp-operations` publishes canonical schemas and active names.
Adding a leaf must not require changing `xtask/src/schemas/mcp_action_registry.rs`.

Regenerate checked-in outputs with the repository generators:

```bash
cargo xtask generated-contracts refresh
cargo xtask schemas generate --check
just gen-mcp-schema
```

Review the generated diffs, including scope/safety/task metadata and reference
closure. Update the canonical operation examples and operator docs in the same
change. Do not manually patch generated JSON, golden snapshots or reference tables.

## 5. Validate runtime, contracts and actual clients

Keep focused sidecar tests for request parsing, handler/error semantics, scopes,
fixed-discriminator refusal and schema-valid/invalid payloads. Operation inventory
tests must catch missing, extra, duplicate and auxiliary-name collisions, not
just absence of previously removed names. Preserve dashboard metadata verbatim.

```bash
cargo test -p axon-mcp
cargo test -p xtask --tests --bins
cargo xtask check-api-parity
cargo xtask check
```

On macOS the known full-crate stack-sensitive test may require
`RUST_MIN_STACK=16777216` and `--test-threads=1`; report the exact invocation.
Do not present an adjusted command as an unmodified default run.

Extend the existing `scripts/e2e/adapters/mcp.py`, mcporter sweep and raw task
wire harness. Exercise legacy, atomic and both over stdio and HTTP; in both mode
actually invoke both forms. Compare complete actual inventories without hiding
extras, and verify a remote HTTP server's observed mode. Keep business success,
expected denial, handled dependency error, skip and failure distinct. Use owned
`axon_e2e_*` collections and disposable local state, never production reset or
cleanup. Verify focused discovery/callability through Labby as well.

When retiring an action, remove its live spec and make its normal schema absent
while preserving intentional error guidance. Do not mark supported focused
source projections (`scrape`, `crawl`, `embed`, `ingest` and `code_search`) as
removed: they remain live service-backed actions.
