# Schema Contracts
Last Modified: 2026-06-30

## Contract

This directory owns all generated or schema-like contracts for the pipeline
unification. Surface docs describe behavior. Schema docs define exact machine
shapes, generation sources, output artifacts, and drift checks.

Schemas are generated from implementation sources where possible. Hand-authored
schema contracts exist to define the target and required generator behavior.

These are not placeholders. Each schema contract is an implementation target.
If an engineer implements the owning generator from the schema contract alone,
the result should be close enough that remaining work is wiring, not design.

## Schema Inventory

| Schema Contract | Owner | Generated Output |
|---|---|---|
| [schema-generator-contract.md](schema-generator-contract.md) | `xtask` + all schema owners | schema generator commands, fixtures, snapshots, CI |
| [mcp-tool-schema.md](mcp-tool-schema.md) | `axon-mcp` + `axon-api` | `docs/reference/mcp/tool-schema.json` |
| [openapi-schema.md](openapi-schema.md) | `axon-web` + `axon-api` | `docs/reference/rest/openapi.json` |
| [cli-schema.md](cli-schema.md) | `axon-cli` + `axon-api` | `docs/reference/cli/commands.json` |
| [api-dto-schema.md](api-dto-schema.md) | `axon-api` | `docs/reference/api/schemas.json` |
| [config-schema.md](config-schema.md) | `axon-core` | `docs/reference/config/config.schema.json` |
| [event-schema.md](event-schema.md) | `axon-observe` | `docs/reference/runtime/events.schema.json` |
| [error-schema.md](error-schema.md) | `axon-error` | `docs/reference/api/errors.schema.json` |
| [database-schema.md](database-schema.md) | store crates | `docs/reference/runtime/database-schema.json` |
| [graph-schema.md](graph-schema.md) | `axon-graph` + `axon-parse` | `docs/reference/sources/graph.schema.json` |
| [vector-payload-schema.md](vector-payload-schema.md) | `axon-vectors` + `axon-api` | `docs/reference/sources/vector-payload.schema.json` |
| [provider-capability-schema.md](provider-capability-schema.md) | provider crates + `axon-api` | `docs/reference/runtime/provider-capabilities.schema.json` |

## Generation Rules

Required command:

```bash
cargo xtask schemas generate
cargo xtask schemas generate --check
```

The generator must:

- emit deterministic JSON
- emit matching markdown reference where useful
- include generator version and source crate versions
- fail on missing schema owners
- fail when generated files drift from source code
- validate all documented examples in the contract packet

## Standard Schema Artifact Shape

Every generated schema artifact must use this top-level envelope unless the
external standard forbids it. OpenAPI is the only exception; OpenAPI uses the
standard OpenAPI root and embeds this metadata under `x-axon`.

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://axon.local/schemas/<family>/<name>.schema.json",
  "title": "HumanReadableSchemaName",
  "description": "One sentence purpose.",
  "x-axon": {
    "contract_version": "2026-06-30",
    "generated_by": "cargo xtask schemas <family>",
    "owner_crates": ["axon-api"],
    "source_inputs": [
      "crates/axon-api/src/source.rs"
    ],
    "clean_break": true
  },
  "$defs": {},
  "type": "object",
  "required": [],
  "properties": {},
  "additionalProperties": false
}
```

Every schema contract in this directory must define:

- exact generated artifact paths
- source crates and files
- root object shape
- required `$defs`
- enum registries
- extension points
- all required records and fields
- all required generated markdown tables
- fixture paths
- validation fixtures
- drift checks
- acceptance criteria

## Completeness Bar

A schema contract is complete only when it answers all of these:

| Question | Required Answer |
|---|---|
| What file is generated? | exact path, format, and root shape |
| Who owns it? | crate/module and generator command |
| What code inputs generate it? | source modules, registries, migrations, clap/routes/DTOs |
| What are the top-level keys? | concrete JSON shape |
| What definitions are required? | `$defs`, registries, route records, table records, etc. |
| What fields are required? | required/optional field list with type and semantics |
| Where are extension points? | exact maps allowed to accept unknown keys |
| What must never appear? | removed aliases, secrets, compatibility fields, raw paths, etc. |
| How is it validated? | check command, fixtures, snapshot tests |
| What breaks CI? | explicit drift/failure list |

## Shared Primitive Definitions

Every schema may reference these shared primitives. They live in
`api-dto-schema.md` as `$defs` and may be imported by generated schema bundles.

| Name | Shape |
|---|---|
| `JobId` | string, pattern `^job_[A-Za-z0-9_-]+$` |
| `RequestId` | string, pattern `^req_[A-Za-z0-9_-]+$` |
| `SourceId` | string, pattern `^src_[A-Za-z0-9_-]+$` |
| `DocumentId` | string, pattern `^doc_[A-Za-z0-9_-]+$` |
| `ChunkId` | string, pattern `^chk_[A-Za-z0-9_-]+$` |
| `ArtifactId` | string, pattern `^art_[A-Za-z0-9_-]+$` |
| `GraphNodeId` | string, pattern `^node_[A-Za-z0-9_-]+$` |
| `GraphEdgeId` | string, pattern `^edge_[A-Za-z0-9_-]+$` |
| `Timestamp` | string, `date-time`, RFC3339 |
| `MetadataMap` | object, scalar/array/object values, no secrets unless field visibility is `sensitive` and redacted before public projection |
| `ContentRef` | discriminated object: `inline_text`, `inline_bytes`, `artifact`, or `external` |
| `ArtifactRef` | object with `artifact_id`, `kind`, `uri`, `content_type`, `size_bytes`, `visibility` |

## Shared `$defs` Import Rule

Generated schemas may either inline shared `$defs` or reference
`https://axon.local/schemas/api/schemas.schema.json#/$defs/<Name>`.

Rules:

- generated standalone artifacts for users must be self-contained
- internal snapshots may use `$ref` to reduce churn
- public docs must show the resolved fields, not only `$ref` chains
- schema cycles are forbidden

## Generator Implementation Contract

The full implementation contract lives in
[schema-generator-contract.md](schema-generator-contract.md). In short,
`cargo xtask schemas generate` dispatches to family generators:

```text
schemas api
schemas cli
schemas openapi
schemas mcp
schemas config
schemas events
schemas errors
schemas database
schemas graph
schemas vector-payload
schemas providers
```

Each generator writes:

- JSON schema artifact
- markdown reference artifact when useful
- golden snapshot under the owning crate
- validation report in check mode
- validation fixtures for valid and invalid examples
- source-input manifest listing every registry/migration/module that fed the
  generated artifact

Each generator has three modes:

| Mode | Behavior |
|---|---|
| default | write generated artifacts |
| `--check` | generate in memory and fail on diff |
| `--print` | print artifact to stdout for debugging |

## Required Generator Source Manifests

Every generated schema artifact includes an `x-axon.source_inputs` array. The
array is deterministic and includes every code/document input that can change
the schema.

Required source input families:

| Schema | Required Inputs |
|---|---|
| MCP | `axon-mcp` action registry, `axon-api` DTO registry, error/event schemas |
| OpenAPI | `axon-web` route registry, `axon-api` DTO registry, auth scope registry, error/event schemas |
| CLI | `axon-cli` command registry, clap metadata, config/env flag mappings, DTO registry |
| API DTO | `axon-api` DTO registry, enum registry, envelope registry |
| Config | config structs/defaults, env metadata registry, secret classification registry |
| Events | event structs, phase/status enum registry, metric descriptors |
| Errors | error code registry, error constructors, transport status mappings |
| Database | migrations, store table metadata, SQLite introspection report |
| Graph | graph kind registry, graph DTOs, parser graph emission registry |
| Vector payload | payload builder registry, metadata field registry, Qdrant index plan |
| Providers | provider spec registry, family capability schemas, fake provider registry |

If any required input cannot be represented in `source_inputs`, the generator is
not implementation-ready.

## Standard Fixture Layout

Every schema family owns fixtures under its owning crate.

```text
crates/<owner>/tests/fixtures/schema/
  valid/
  invalid/
  snapshots/
```

Required fixture categories:

| Category | Requirement |
|---|---|
| `valid/minimal` | Smallest accepted object. |
| `valid/full` | Object using every optional field and extension point. |
| `invalid/missing-required` | Missing one required field. |
| `invalid/unknown-field` | Unknown top-level field where forbidden. |
| `invalid/bad-enum` | Wrong enum casing or unknown value. |
| `invalid/secret` | Secret appears in public schema/payload when forbidden. |
| `snapshots/generated` | Golden generated artifact. |

Fixtures are part of the contract. A schema without valid and invalid fixtures
is not complete.

## Standard Generated Markdown

Every generated markdown reference includes:

- generated header
- owning crate/module
- source input manifest
- root schema shape
- required fields table
- optional fields table
- enum tables
- extension points
- examples linked to fixtures
- drift-check command
- removed/forbidden fields when applicable

Generated markdown must not be hand-maintained tables copied from code.

## Schema Naming

Rules:

- JSON schema ids use `https://axon.local/schemas/...`
- JSON fields are snake_case
- Rust enum variants serialize as snake_case
- removed commands/actions/routes/fields are absent
- deprecated fields are not used for this clean break
- unknown fields fail unless explicitly carried by `metadata`, `options`, or
  adapter-owned extension maps

## Testing Requirements

- every schema has generator tests
- every schema has golden snapshot tests
- every schema validates examples in docs
- every schema is linked from final documentation
- every schema owner has a fake-backed test path when behavior is executable

## Acceptance Criteria

The schema directory is implementation-ready when:

- every schema contract has generated artifact paths
- every schema contract has root shape and required definition/record shapes
- every schema contract has drift checks
- every schema contract has acceptance criteria
- every schema artifact can be generated with `cargo xtask schemas generate`
- `cargo xtask schemas generate --check` fails on any code/doc drift
- every generated artifact has a matching markdown reference or an explicit
  reason why JSON-only is enough
