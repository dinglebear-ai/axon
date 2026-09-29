# axon-mcp design-contract maintenance

This directory documents the `axon-mcp` boundary: Own MCP discovery, schemas, request routing, auth integration, task protocol handling, resources, and envelopes.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-mcp/src/AGENTS.md) · [Crate exports](../../../../crates/axon-mcp/src/lib.rs)

[overview](../../../../docs/reference/mcp/overview.md) · [tool schema](../../../../docs/reference/mcp/tool-schema.md) · [adding mcp action](../../../../docs/development/adding-mcp-action.md) · [api parity](../../../../docs/reference/api-parity.md)

## Review the actual boundary

- The catalog includes primary axon and auxiliary axon_status_dashboard tools. Ordinary actions, system requests, and watches use distinct request routing types; absence from AxonRequest alone does not imply unsupported.

- Do not infer shipped projection behavior from an unmerged worktree or from a narrower generated schema. Inspect the matching build’s tools/list and representative actual calls.

For implementation evidence, inspect [server.rs](../../../../crates/axon-mcp/src/server.rs), [server/tool_schema.rs](../../../../crates/axon-mcp/src/server/tool_schema.rs), [server/system_requests.rs](../../../../crates/axon-mcp/src/server/system_requests.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

Schema and server sidecars plus real MCP discovery/calls, including invalid input, unauthorized tools/resources, task cancellation/recovery, and artifact responses.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
