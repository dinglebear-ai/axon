# MCP Tool Contract

Last reviewed: 2026-09-29

Axon has one canonical dispatcher with startup-static `legacy`, `atomic`, and
`both` projections. Legacy mode accepts the aggregate `axon` tool with
`action`/`subaction`; atomic mode advertises canonical leaf names and rejects
those fixed routing fields in leaf arguments. `both` publishes both forms.
The auxiliary `axon_status_dashboard` tool is a separate preserved route.

The [generated tool reference](tool-schema.md),
[machine-readable contract](tool-schema.json), and
[projection guide](overview.md) describe the current catalog. Discovery must
be checked against the actual running build, not a remembered count or only
the primary request enum.

## Request and dispatch contract

Shared domain DTOs belong to `axon-api`. Transport-only request envelopes and
intentionally distinct system/watch requests live in `axon-mcp`; handlers
convert to public contracts and call shared services. The primary
`AxonRequest` enum is not the whole catalog. See
[server.rs](../../../crates/axon-mcp/src/server.rs),
[system requests](../../../crates/axon-mcp/src/server/system_requests.rs), and
[operation registry](../../../crates/axon-mcp/src/server/operations.rs).

Legacy requests require a valid `action` and the fields required by its
selected schema. Atomic requests use the discovered leaf schema without
`action` or fixed `subaction`. Fixed selectors are rejected even when their
values match the intended operation. Do not replace these rules with a
permissive union of all action properties.

Safety/destructiveness/idempotence hints describe effects; they do not grant
permission. Caller authorization, explicit write elevation, ownership and
visibility, and destructive confirmation remain separate runtime checks.
Plans may persist review records even when their execution is deferred.

## Source acquisition

The universal source operation accepts a canonical source selector and an
adapter-supported scope. Focused `scrape`, `crawl`, `embed`, and `ingest`
projections remain supported and reuse shared source services. `code_search`
queries committed code vectors. Do not classify those projections as removed
or create private acquisition-to-embedding pipelines for them.

Example arguments for the legacy aggregate:

<!-- doc-example: kind=json schema=mcp/tool-schema.json#/$defs/AxonToolInput -->
```json
{ "action": "source", "source": "https://example.com", "scope": "page" }
```

This submits work when actually called; it is not a read-only capability
probe. Host-local inputs require server-side allowed roots and acquisition
authorization. CLI/MCP tool-source discovery is not permission to execute
the discovered tool. See [source scopes](../sources/adapter-scopes.md).

## Response modes and artifacts

| Mode | Meaning |
|---|---|
| `artifact` | Store the result and return opaque artifact metadata |
| `inline` | Return inline content subject to size/visibility policy |
| `both` | Return permitted inline content and an artifact reference |
| `auto_inline` | Inline small permitted results; use artifacts otherwise |

Defaults can vary by operation; `retrieve` is inline-first for bounded
document reading. Use the actual response schema rather than assuming every
result is the same `{ok, data}` object. Artifact responses expose opaque
`artifact_id` values, not a public `path` contract or a server filesystem
location. Follow the returned resource/API metadata instead of constructing
private paths.

## Task support

The current server advertises task capability through its extension map.
Only canonical `extract.start` supports task-augmented calls, whether reached
through legacy `axon` or atomic `extract_start`. A source job is not
automatically an MCP protocol task.

A client must negotiate the task extension and supply task metadata on the
call. Capability and operation-support checks occur before enqueue. Caller
identity, authorization, and progress metadata must survive canonicalization.
The task identifier aliases the durable job; task acceptance is not job
completion.

In this implementation, `tasks/get` returns the detailed task, including its
terminal result/error. `tasks/cancel` acknowledges the request and the next
`tasks/get` shows the observed state. Do not assume an older client's
`tasks/result` or `tasks/list` sequence is implemented by this build. Respect
the returned polling interval. See
[task handlers](../../../crates/axon-mcp/src/server/tasks.rs) and
[task-augmented calls](overview.md#task-augmented-calls).

## Resources

| URI | Purpose |
|---|---|
| `axon://schema/mcp-tool` | Compatible aggregate tool schema |
| `axon://schema/mcp-operations` | Canonical operation schemas, safety/task metadata, selected projection, and active names |
| `ui://axon/status-dashboard` | MCP App status presentation |

These resources do not replace `tools/list` or authorize operations. Keep
discovery, read-resource behavior, dashboard metadata, tool authorization,
and task capability consistent.

## Transport and auth

`axon mcp` defaults to stdio. HTTP/both transport and `axon serve mcp` use
the unified HTTP listener. Loopback-only tokenless development is distinct
from non-loopback deployment, which requires `AXON_HTTP_TOKEN` or OAuth.
Web-panel password/session unlock is not an API/MCP token. See
[transport](transport.md) and [MCP authentication](../../operations/auth/mcp-auth.md).

## Errors and partial results

Input-shape and unsupported-operation failures must identify the invalid
field/selector. Authorization failures must name the missing scope or
prerequisite without revealing credentials. Runtime failures retain safe
`ApiError`/typed-cause context through MCP error or result envelopes.

Do not flatten provider failures into generic strings, report a failed stage
as a successful empty result, or imply that a transport error rolled back
already-published state. Preserve operation/stage, affected entity and job
IDs, retryability, side effects, and recovery actions. Partial success,
degradation, cancellation, and unknown commit status are distinct outcomes.

## Maintenance

`vertical_scrape`, `purge`, `dedupe`, and `code_search_watch` are removed
legacy actions. Use source/projection operations for acquisition and the
reviewed prune lifecycle for cleanup.

Follow [adding an MCP action](../../development/adding-mcp-action.md), refresh
generated contracts in their declared order, and test the real catalog and
calls for legacy/atomic/both, invalid arguments, denied authorization,
resources, and tasks. A generated schema or accepting job ID alone is not
proof of successful execution.
