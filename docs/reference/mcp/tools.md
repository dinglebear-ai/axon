# MCP Tools Reference -- Axon

Last reviewed: 2026-09-29

The complete catalog is generated in [tool-schema.json](tool-schema.json)
and [the reference](tool-schema.md). This page explains how to use that
catalog, not a second hand-maintained list of every leaf.

## Discover the selected projection

| Projection | Callable shape |
|---|---|
| `legacy` | `axon` with `action` and operation-specific arguments |
| `atomic` | Canonical names such as `query` or `jobs_get`, without fixed `action`/`subaction` arguments |
| `both` | Both forms, routed through the same canonical dispatcher |

All three retain the auxiliary `axon_status_dashboard` tool. The operation
schema resource `axon://schema/mcp-operations` reports canonical schemas and
active names. Use live `tools/list` against the matching build before calling
a tool, and inspect [projection selection](overview.md#projection-selection-and-compatibility)
when a client sees a different catalog.

## Input shape

This is a valid legacy single-page source request, not a union of possible
values pasted into one field:

<!-- doc-example: kind=json schema=mcp/tool-schema.json#/$defs/AxonToolInput -->
```json
{ "action": "source", "source": "https://example.com", "scope": "page" }
```

Choose one supported scope and response mode. In atomic mode, use the
discovered source leaf schema and omit its fixed routing fields. A gateway
namespace prefix is supplied by the gateway, not part of the canonical Axon
leaf naming rule.

## Source operations

The universal source operation and supported focused `scrape`, `crawl`,
`embed`, and `ingest` projections reuse one shared source pipeline. They
are **not removed**. `code_search` is a committed-state retrieval projection.
The adapter scope matrix defines which source family accepts which scope,
options, authentication, and output contract.

Web `page` is single-page acquisition; `site` and `docs` select bounded
multi-page workflows. Local/session/tool inputs are evaluated on the
executing host with its access policy. A schema that accepts a source string
does not authorize arbitrary filesystem access or execution of a discovered
CLI/MCP tool.

A detached descriptor means work was accepted. Use the same durable job ID
to inspect actual completion, diagnostics, and published generation.

## Retrieval, synthesis, and operations

Use discovered query/retrieve/code-search operations for committed content.
`ask` synthesizes from retrieved context. External `search` and `research`
can have source-indexing side effects and require their explicit write
elevation; a read-shaped result does not make an operation read-only.

Grouped operations include durable jobs, extraction, memory, graph, watches,
providers, and prune. System/watch requests have separate transport request
types, so absence from the primary action enum does not prove absence from
MCP. Use the generated operation schema for exact selectors and required
fields instead of extrapolating CLI subcommands.

## Results and artifacts

Response modes are `artifact`, `inline`, `both`, and `auto_inline`, subject
to the supported operation shape, size, and visibility policy. Artifact
references are opaque IDs; never reconstruct server filesystem paths.
`retrieve` is inline-first for paged document content.

Only extraction start currently supports negotiated MCP task execution.
Other durable source jobs are followed through the job surface, not assumed
to support protocol task methods. See [the tool contract](tool-contract.md).

## Removed actions

`vertical_scrape`, `purge`, `dedupe`, and `code_search_watch` are intentionally
rejected. Do not extend this list using an old migration plan: supported
focused projections and system/utility routes must remain discoverable.

Use [the overview](overview.md) for calls and transports,
[the tool contract](tool-contract.md) for tasks/artifacts/errors, and
[MCP development](dev.md) for implementation and validation.
