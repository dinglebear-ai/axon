# MCP Tool Schema Contract
Last Modified: 2026-06-30

## Contract

This file owns the exact MCP tool schema shape for the clean-break `axon` tool.
It complements [tool-contract.md](../surfaces/tool-contract.md):

- `tool-contract.md` defines action semantics, routing, boundaries, examples,
  and agent-facing behavior.
- `mcp-tool-schema.md` defines the generated MCP schema document, input schema,
  response envelope schema, action/subaction discriminators, schema generation,
  and drift checks.

The MCP schema is generated from `axon-api` DTOs plus the `axon-mcp` action
registry. Hand-written schema fragments are allowed only for MCP-specific
transport wrapper fields.

## Generated Artifacts

```text
docs/reference/mcp/tool-schema.json
docs/reference/mcp/tool-schema.md
crates/axon-mcp/tests/golden/tool-schema.json
```

Generator:

```bash
cargo xtask schemas mcp
cargo xtask schemas mcp --check
```

## Source Inputs

The MCP tool schema generator reads:

```text
crates/axon-mcp/src/action_registry.rs
crates/axon-mcp/src/tool_model.rs
crates/axon-mcp/src/handlers/**
crates/axon-api/src/**
crates/axon-error/src/**
crates/axon-observe/src/**
docs/pipeline-unification/surfaces/tool-contract.md
```

The generated artifact records these paths in `x-axon.source_inputs`.

## Single Tool Document

Axon exposes one MCP tool:

```json
{
  "name": "axon",
  "description": "Acquire, normalize, embed, refresh, search, retrieve, answer from, inspect, and operate on Axon source knowledge.",
  "inputSchema": {
    "$ref": "#/$defs/AxonToolInput"
  },
  "annotations": {
    "title": "Axon",
    "readOnlyHint": false,
    "destructiveHint": false,
    "idempotentHint": false,
    "openWorldHint": true
  }
}
```

MCP clients discover action-specific capabilities through:

- the `inputSchema` definitions
- `action=capabilities`
- `action=help`

## Root Input Schema

The root schema is a discriminated action envelope.

```json
{
  "$id": "https://axon.local/schemas/mcp/axon-tool-input.schema.json",
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "AxonToolInput",
  "type": "object",
  "required": ["action"],
  "properties": {
    "action": {
      "$ref": "#/$defs/Action"
    },
    "subaction": {
      "$ref": "#/$defs/Subaction"
    },
    "source": {
      "type": "string",
      "description": "Source URI, URL, path, shorthand, package id, repo id, or source id."
    },
    "sources": {
      "type": "array",
      "items": { "type": "string" }
    },
    "query": {
      "type": "string"
    },
    "question": {
      "type": "string"
    },
    "body": {
      "type": "object",
      "additionalProperties": true
    },
    "wait": {
      "type": "boolean",
      "default": false
    },
    "response_mode": {
      "$ref": "#/$defs/ResponseMode",
      "default": "auto"
    }
  },
  "allOf": [
    { "$ref": "#/$defs/ActionDiscriminatorRules" }
  ],
  "unevaluatedProperties": true
}
```

`unevaluatedProperties` is true only at the root for agent ergonomics. After
`action` and `subaction` are selected, Axon validates the effective payload
against the action-specific request DTO. Unknown fields at that point fail
validation unless the target DTO explicitly exposes `options`, `metadata`, or
`body`.

## Action Enum

```json
{
  "$defs": {
    "Action": {
      "type": "string",
      "enum": [
        "source",
        "resolve",
        "map",
        "search",
        "query",
        "retrieve",
        "ask",
        "chat",
        "evaluate",
        "suggest",
        "research",
        "summarize",
        "endpoints",
        "brand",
        "diff",
        "screenshot",
        "extract",
        "memory",
        "jobs",
        "watches",
        "artifacts",
        "uploads",
        "prune",
        "collections",
        "graph",
        "providers",
        "reset",
        "status",
        "doctor",
        "capabilities",
        "help"
      ]
    }
  }
}
```

Removed actions are absent from this enum. There are no compatibility aliases.

## Subaction Enums

Grouped actions use action-specific subactions:

| Action | Subactions |
|---|---|
| `memory` | `remember`, `search`, `context`, `show`, `link`, `supersede`, `reinforce`, `contradict`, `pin`, `archive`, `forget`, `review`, `compact` |
| `jobs` | `get`, `list`, `events`, `cancel`, `retry`, `recover`, `cleanup`, `clear` |
| `watches` | `create`, `list`, `get`, `status`, `exec`, `pause`, `resume`, `delete`, `history` |
| `artifacts` | `list`, `get`, `content` |
| `uploads` | `create`, `get`, `put_content`, `complete`, `abort` |
| `prune` | `plan`, `exec`, `dedupe`, `purge` |
| `collections` | `list`, `get` |
| `graph` | `kinds`, `resolve`, `query`, `node`, `edge`, `source` |
| `providers` | `list`, `get` |

Subaction validation is strict:

- grouped actions require `subaction`
- ungrouped actions reject `subaction`
- invalid action/subaction pairs fail before side effects

## Action Request Mapping

Every action maps to exactly one `axon-api` request DTO after MCP conversion.

| Action | Effective DTO |
|---|---|
| `source` | `SourceRequest` |
| `resolve` | `ResolveSourceRequest` |
| `map` | `SourceRequest` with `intent=map` |
| `search` | `SearchRequest` |
| `query` | `QueryRequest` |
| `retrieve` | `RetrievalRequest` |
| `ask` | `AskRequest` |
| `chat` | `ChatRequest` |
| `evaluate` | `EvaluationRequest` |
| `suggest` | `SuggestRequest` |
| `research` | `ResearchRequest` |
| `summarize` | `SummarizeRequest` |
| `reset` | `ResetPlanRequest` or `ResetExecRequest` based on `body.confirm` / `body.reset_plan_id` |
| `endpoints` | `EndpointDiscoveryRequest` |
| `brand` | `BrandRequest` |
| `diff` | `DiffRequest` |
| `screenshot` | `ScreenshotRequest` |
| `extract` | `ExtractRequest` |
| grouped actions | matching `*Request` for selected subaction |

`body` is merged after top-level fields. Top-level fields win on conflict
unless the action explicitly declares `body_precedence=true`.

## Complete Action Registry Shape

The `axon-mcp` crate owns a compile-time registry:

```rust
pub struct McpActionSpec {
    pub action: &'static str,
    pub subactions: &'static [McpSubactionSpec],
    pub request_dto: &'static str,
    pub result_dto: &'static str,
    pub service: &'static str,
    pub mutates: bool,
    pub async_job: bool,
    pub required_scope: Option<AuthScope>,
    pub read_only_hint: bool,
    pub destructive_hint: bool,
    pub idempotent_hint: bool,
    pub open_world_hint: bool,
}

pub struct McpSubactionSpec {
    pub subaction: &'static str,
    pub request_dto: &'static str,
    pub result_dto: &'static str,
    pub service: &'static str,
    pub mutates: bool,
    pub async_job: bool,
    pub required_scope: Option<AuthScope>,
}
```

The schema generator uses this registry for:

- `Action` enum
- subaction enums
- action/subaction validation branches
- annotations
- help/capabilities output
- dispatch parity tests

If an MCP handler exists without a registry entry, compilation or schema check
fails.

## Required Action Branches

Every action branch must define:

| Field | Required? | Rule |
|---|---:|---|
| `action` | yes | exact const |
| `subaction` | grouped only | exact const or enum |
| primary input field | action-specific | `source`, `query`, `question`, ids, or `body` |
| `body` | optional | refs effective DTO |
| `wait` | async actions | boolean |
| `response_mode` | all | enum |
| forbidden fields | yes | e.g. `subaction` forbidden for ungrouped actions |

Minimum validation branch matrix:

| Action | Required Input | Forbidden |
|---|---|---|
| `source` | `source` | `query`, `question`, `subaction` |
| `resolve` | `source` | `query`, `question`, `subaction` |
| `map` | `source` | `query`, `question`, `subaction` |
| `search` | `query` | `question`, `subaction` |
| `query` | `query` | `question`, `subaction` |
| `retrieve` | one of `source`, `source_id`, `document_id`, `url`, `chunk_id` | `question`, `subaction` |
| `ask` | `question` | `subaction` |
| `extract` | `source` and `schema`/`body.schema` | `query`, `subaction` |
| grouped | `subaction` | invalid subaction/action pairs |

## Action Discriminator Rules

The generated schema must include a `oneOf` discriminator for action/subaction
pairs. Each branch constrains:

- exact `action` value
- exact allowed `subaction` values when grouped
- required fields
- forbidden fields
- referenced DTO schema

Example branch:

```json
{
  "if": {
    "properties": { "action": { "const": "query" } },
    "required": ["action"]
  },
  "then": {
    "required": ["query"],
    "not": { "required": ["subaction"] },
    "properties": {
      "body": { "$ref": "#/$defs/QueryRequest" }
    }
  }
}
```

Grouped action branch:

```json
{
  "if": {
    "properties": {
      "action": { "const": "jobs" },
      "subaction": { "const": "events" }
    },
    "required": ["action", "subaction"]
  },
  "then": {
    "properties": {
      "body": { "$ref": "#/$defs/JobEventListRequest" }
    }
  }
}
```

## Required `$defs`

The MCP schema bundle must embed or reference:

- `Action`
- action-specific subaction enums
- `ResponseMode`
- `AxonToolInput`
- `AxonToolResponse`
- all action request DTOs
- all action result DTOs
- `ApiError`
- `SourceWarning`
- `JobDescriptor`
- `WatchDescriptor`
- `SourceProgressEvent`
- `ArtifactRef`
- `Page`
- `TraceContext`

## Generated Markdown Reference

`docs/reference/mcp/tool-schema.md` contains:

- one table of actions
- one table per grouped action with subactions
- generated JSON schema excerpt for root input
- generated examples for every action
- removed-action absence checklist
- response envelope shape

Examples are generated from fixtures and validated against the JSON schema.

## MCP Content Rendering

Tool result content rules:

- JSON response is the primary content item.
- Human text is optional and derived from the same envelope.
- Artifacts are returned as ids/URIs, not as large inline content.
- Errors use MCP error response only for protocol-level failures; Axon domain
  errors use `ok=false` envelope unless the MCP server itself cannot process the
  request.

## Response Envelope Schema

Every MCP response uses the shared envelope projected into MCP content.

```json
{
  "$defs": {
    "AxonToolResponse": {
      "type": "object",
      "required": ["ok", "action", "request_id", "contract_version", "warnings", "trace"],
      "properties": {
        "ok": { "type": "boolean" },
        "action": { "$ref": "#/$defs/Action" },
        "subaction": { "type": ["string", "null"] },
        "request_id": { "type": "string" },
        "contract_version": { "type": "string" },
        "data": { "type": ["object", "array", "string", "null"] },
        "error": { "$ref": "#/$defs/ApiError" },
        "warnings": {
          "type": "array",
          "items": { "$ref": "#/$defs/SourceWarning" }
        },
        "job": { "$ref": "#/$defs/JobDescriptor" },
        "watch": { "$ref": "#/$defs/WatchDescriptor" },
        "progress": { "$ref": "#/$defs/SourceProgressEvent" },
        "artifacts": {
          "type": "array",
          "items": { "$ref": "#/$defs/ArtifactRef" }
        },
        "pagination": { "$ref": "#/$defs/Page" },
        "trace": { "$ref": "#/$defs/TraceContext" }
      }
    }
  }
}
```

Large outputs use `artifact` references instead of giant inline content.

## Schema Generation

The generated schema is produced by:

```bash
cargo xtask schemas mcp
```

Generation inputs:

- `axon-api` request/result DTO schemas
- `axon-error` error schema
- `axon-observe` progress event schema
- `axon-mcp` action/subaction registry
- `axon-mcp` MCP wrapper fields

Generated outputs:

- `docs/reference/mcp/tool-schema.md`
- `docs/reference/mcp/tool-schema.json`
- schema snapshots under the owning crate tests

Generated files include:

```text
<!-- generated by cargo xtask schemas mcp; do not edit directly -->
```

## Drift Checks

Required check:

```bash
cargo xtask schemas mcp --check
```

The check fails when:

- action enum differs from `axon-mcp` registry
- subaction enum differs from grouped action registry
- action-to-DTO mapping differs from service dispatch
- request DTO schema differs from `axon-api`
- response envelope differs from shared envelope
- removed actions appear anywhere in the schema
- examples in `tool-contract.md` no longer validate

## Testing Requirements

- root schema validates every documented example
- invalid action fails before dispatch
- invalid subaction fails before dispatch
- removed actions are absent from schema
- grouped actions require subaction
- ungrouped actions reject subaction
- top-level ergonomic fields and `body` merge deterministically
- large responses return artifact refs
- MCP schema generation is reproducible

## Validation Fixtures

Required fixtures:

```text
crates/axon-mcp/tests/fixtures/schema/source.valid.json
crates/axon-mcp/tests/fixtures/schema/query.valid.json
crates/axon-mcp/tests/fixtures/schema/retrieve.valid.json
crates/axon-mcp/tests/fixtures/schema/jobs_events.valid.json
crates/axon-mcp/tests/fixtures/schema/missing_subaction.invalid.json
crates/axon-mcp/tests/fixtures/schema/removed_crawl.invalid.json
```

## Acceptance Criteria

- every MCP action has a registry entry
- every registry entry maps to an `axon-api` DTO and `axon-services` method
- generated schema validates every example in `tool-contract.md`
- invalid action/subaction pairs fail before service dispatch
- removed actions are absent from schema and dispatch
- response envelope matches `axon-api` envelope projection
