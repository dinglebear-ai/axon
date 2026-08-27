---
title: "MCP Tool Sources"
created: 2026-07-15
updated: 2026-08-24
---

# MCP Tool Sources
Last Modified: 2026-08-24

MCP tool sources let Axon index MCP tool contracts and, with explicit execution authorization, materialize MCP call results through the shared source pipeline.

## Source Shape

Use an `mcp:` source identifier for a specific logical MCP target, for example `mcp:labby/search`. The router canonicalizes the source to `mcp://<server>/tools/<tool>`, then the service dispatch layer runs the `mcp_tool` adapter.

## Current Behavior

The current default path is metadata-only. It records a ledger generation and one adapter-owned schema/metadata document without executing an upstream tool or writing vectors.

The current call mode is available only on `scope=api` with `execution_mode=call`. It re-checks `axon:execute`, validates an exact MCP target allowlist, and then uses a configured local caller command. That command-caller bridge is transitional and is being superseded by the Labby-backed provider design below.

## Target: Labby-backed MCP Ingestion

Axon should not own OAuth with each upstream MCP server. Labby is the gateway and auth boundary:

```text
Axon mcp_tool adapter
  -> Labby service identity
  -> Labby gateway / snippet
  -> OAuth-authenticated upstream MCP server
  -> axon.mcp-ingest/v1 snapshot
  -> Axon manifest/diff/SourceDocument pipeline
```

Labby owns upstream discovery, OAuth authorization/refresh, route/loadout/tool policy, and invocation. Axon owns source identity, durable generations, diffing, normalization, parsing, graphing, embedding, publication, retrieval, and cleanup.

Labby's outbound upstream OAuth credentials are subject-scoped. A single-user or homelab deployment may use a dedicated Axon ingestion service identity whose Labby subject has already authorized the desired upstreams. Multi-user deployments must preserve or explicitly delegate the authorized Labby subject rather than silently sharing one user's upstream OAuth state.

The Labby-backed provider should be injected into `McpToolSourceAdapter` by `axon-services`; `axon-adapters` remains independent of Labby transport/auth details.

## One-call Materialization

For ingestion profiles such as Asana or Linear, the Labby snippet/tool may return a bounded complete snapshot. `McpToolSourceAdapter::materialize` should call Labby once and write the returned `axon.mcp-ingest/v1` envelope to an OS-created, unpredictable temporary directory. Create the directory owner-only (`0700`) and the dump with exclusive creation and owner-only permissions (`0600`); never follow a caller-selected symlink or log the raw path or contents. A scope guard must remove the dump and directory on success, error, cancellation, and unwind. `discover`, `acquire`, and `normalize` then operate against that same materialized snapshot.

This follows the existing Axon pattern used by registry, Reddit, and feed adapters and preserves the canonical pipeline order:

```text
materialize once
  -> discover item manifest
  -> ledger diff
  -> acquire only added/modified records from snapshot
  -> normalize to SourceDocument
  -> parse / graph / prepare / embed / publish
```

A complete snapshot may infer removals. Partial or paginated results must not infer removals unless the envelope explicitly carries tombstones or completion semantics.

## Execution Policy

MCP ingestion remains security-sensitive even though Labby performs the upstream call. Axon should retain `axon:execute`, an Axon-side exact target allowlist, durable authorization audit, timeout/result-size bounds, redaction, and artifact capture as defense in depth. Labby independently enforces its own route, loadout, upstream, OAuth, and tool policies.

Axon should remove the local `mcp_caller_command` / `mcp_caller_allowlist` execution path once the Labby provider is implemented. See [Adding a Source Adapter](../development/adding-source-adapter.md#labby-backed-mcp-migration-code-impact) for the code migration map.
