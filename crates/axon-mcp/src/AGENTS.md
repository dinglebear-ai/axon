# axon-mcp

Own MCP discovery, schemas, request routing, auth integration, task protocol handling, resources, and envelopes.

## Read before changing

[overview](../../../docs/reference/mcp/overview.md) · [tool schema](../../../docs/reference/mcp/tool-schema.md) · [adding mcp action](../../../docs/development/adding-mcp-action.md) · [api parity](../../../docs/reference/api-parity.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[server.rs](server.rs) · [server/tool_schema.rs](server/tool_schema.rs) · [server/system_requests.rs](server/system_requests.rs) · [server/tasks.rs](server/tasks.rs) · [server/authz.rs](server/authz.rs) · [server/http.rs](server/http.rs) · [schema.rs](schema.rs)

## Change requirements

- The catalog includes primary axon and auxiliary axon_status_dashboard tools. Ordinary actions, system requests, and watches use distinct request routing types; absence from AxonRequest alone does not imply unsupported.

- Keep discovery, schema acceptance, dispatch, auth, task completion/cancellation, and resource metadata synchronized. New projections reuse canonical services rather than creating parallel pipelines.

- Tool/resource errors must preserve safe actionable cause, correlation IDs, retry information, and partial-effect status. Return opaque artifact IDs, never server paths.

- Do not infer shipped projection behavior from an unmerged worktree or from a narrower generated schema. Inspect the matching build’s tools/list and representative actual calls.

## Verification for code changes

Schema and server sidecars plus real MCP discovery/calls, including invalid input, unauthorized tools/resources, task cancellation/recovery, and artifact responses.

Use focused `cargo test -p axon-mcp` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
