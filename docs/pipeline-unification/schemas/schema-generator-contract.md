# Schema Generator Contract
Last Modified: 2026-06-30

## Contract

This file is the implementation contract for the schema generation system.

The individual schema contracts define what each schema family emits. This file
defines how the generators are built, where the code lives, what registries they
consume, how they validate fixtures, and what CI must enforce.

An implementation is not complete until:

- every schema family has a generator module
- every generator consumes declared source inputs
- every generated artifact is reproducible
- every schema has valid and invalid fixtures
- every schema has golden snapshots
- every documented example validates
- every drift check runs in one command

## Generated Artifacts

This contract does not define one schema artifact of its own. It defines the
generator that produces every artifact in this directory.

Generator-owned outputs:

```text
docs/reference/**/*
crates/*/tests/fixtures/schema/snapshots/*
target/schema-check-report.json
```

Generator:

```bash
cargo xtask schemas generate
cargo xtask schemas generate --check
```

## Target `xtask` Layout

```text
xtask/
  Cargo.toml
  src/
    main.rs
    schema/
      mod.rs
      args.rs
      artifact.rs
      check.rs
      markdown.rs
      manifest.rs
      validate.rs
      families/
        api.rs
        cli.rs
        openapi.rs
        mcp.rs
        config.rs
        events.rs
        errors.rs
        database.rs
        graph.rs
        vector_payload.rs
        providers.rs
    docs/
      mod.rs
    util/
      diff.rs
      fs.rs
      json.rs
      paths.rs
```

Rules:

- `xtask/src/schema/mod.rs` owns dispatch.
- `xtask/src/schema/families/*.rs` owns one schema family each.
- family modules do not read arbitrary files; they consume declared registries,
  migrations, or generated metadata.
- check mode never writes generated artifacts.
- generation mode writes only declared output paths.
- JSON output is deterministic and pretty-printed with stable key ordering.

## CLI Contract

Required commands:

```bash
cargo xtask schemas generate
cargo xtask schemas generate --check
cargo xtask schemas generate --print

cargo xtask schemas api
cargo xtask schemas api --check
cargo xtask schemas api --print

cargo xtask schemas cli
cargo xtask schemas openapi
cargo xtask schemas mcp
cargo xtask schemas config
cargo xtask schemas events
cargo xtask schemas errors
cargo xtask schemas database
cargo xtask schemas graph
cargo xtask schemas vector-payload
cargo xtask schemas providers
```

Required flags:

| Flag | Meaning |
|---|---|
| `--check` | Generate in memory and fail when output differs. |
| `--print` | Print generated artifact to stdout. |
| `--json` | Print validation/check report as JSON. |
| `--family <name>` | Restrict aggregate generate/check to one family. |
| `--update-fixtures` | Regenerate fixture snapshots; forbidden in CI. |

Exit codes:

| Code | Meaning |
|---:|---|
| `0` | generated/check succeeded |
| `1` | generated output differs |
| `2` | validation fixture failed |
| `3` | source input missing or registry incomplete |
| `4` | generator bug/internal error |

## Core Interfaces

```rust
pub trait SchemaFamilyGenerator {
    fn family(&self) -> SchemaFamily;
    fn source_inputs(&self, repo: &RepoPaths) -> Result<Vec<SourceInput>>;
    fn generate(&self, ctx: &SchemaGenerateContext) -> Result<SchemaArtifactSet>;
    fn validate(&self, ctx: &SchemaValidateContext, artifacts: &SchemaArtifactSet) -> Result<SchemaValidationReport>;
}
```

```rust
pub struct SchemaArtifactSet {
    pub family: SchemaFamily,
    pub artifacts: Vec<GeneratedArtifact>,
    pub source_inputs: Vec<SourceInput>,
    pub validation_fixtures: Vec<FixtureSet>,
}
```

```rust
pub struct GeneratedArtifact {
    pub path: Utf8PathBuf,
    pub kind: ArtifactKind,
    pub content: String,
    pub checksum: String,
}
```

```rust
pub enum ArtifactKind {
    JsonSchema,
    OpenApi,
    MarkdownReference,
    GoldenSnapshot,
    IndexPlan,
}
```

## Required Family Generators

| Family | Module | Primary Source Registry | Output |
|---|---|---|---|
| `api` | `families/api.rs` | `axon-api::schema_registry()` | `docs/reference/api/schemas.json` |
| `cli` | `families/cli.rs` | `axon-cli::command_registry()` | `docs/reference/cli/commands.json` |
| `openapi` | `families/openapi.rs` | `axon-web::route_registry()` | `docs/reference/rest/openapi.json` |
| `mcp` | `families/mcp.rs` | `axon-mcp::action_registry()` | `docs/reference/mcp/tool-schema.json` |
| `config` | `families/config.rs` | `axon-core::config_registry()` | config/env schemas |
| `events` | `families/events.rs` | `axon-observe::event_registry()` | `docs/reference/runtime/events.schema.json` |
| `errors` | `families/errors.rs` | `axon-error::error_registry()` | `docs/reference/api/errors.schema.json` |
| `database` | `families/database.rs` | migrations + store schema registries | `docs/reference/runtime/database-schema.json` |
| `graph` | `families/graph.rs` | `axon-graph::kind_registry()` | `docs/reference/sources/graph.schema.json` |
| `vector-payload` | `families/vector_payload.rs` | `axon-vectors::payload_registry()` | `docs/reference/sources/vector-payload.schema.json` |
| `providers` | `families/providers.rs` | provider `ProviderSpec` registries | `docs/reference/runtime/provider-capabilities.schema.json` |

Every family generator also emits markdown reference unless the family contract
explicitly says JSON-only is enough.

## Registry Contract

Schema generators consume Rust registries, not ad hoc source parsing, whenever
the schema is code-owned.

Required registry traits:

```rust
pub trait SchemaRegistry {
    fn registry_name(&self) -> &'static str;
    fn owner_crate(&self) -> &'static str;
    fn source_inputs(&self) -> &'static [&'static str];
    fn schema_specs(&self) -> &'static [SchemaSpec];
}
```

```rust
pub struct SchemaSpec {
    pub name: &'static str,
    pub kind: SchemaSpecKind,
    pub rust_type: Option<&'static str>,
    pub fields: &'static [FieldSpec],
    pub enums: &'static [EnumSpec],
    pub examples: &'static [SchemaExample],
    pub extension_points: &'static [ExtensionPointSpec],
    pub forbidden_fields: &'static [&'static str],
}
```

Registries are required for:

- DTOs
- enums
- CLI commands/flags
- MCP actions/subactions
- REST routes
- config/env keys
- events/metrics
- error codes
- graph kinds
- vector payload fields
- provider capabilities

Database schema may additionally use SQLite introspection because migrations are
the source of truth for physical tables.

## Source Input Manifest

Every generated artifact includes:

```json
{
  "x-axon": {
    "source_inputs": [
      {
        "path": "crates/axon-api/src/source.rs",
        "kind": "rust_module",
        "checksum": "sha256:..."
      }
    ]
  }
}
```

Rules:

- source input paths are repo-relative
- checksums are stable SHA-256 values
- generated artifacts include all direct schema inputs
- check mode fails when an input path is missing
- check mode fails when a schema-relevant source changes but generated artifacts
  are stale

## Validation Harness

Every family validator runs:

1. JSON parse for generated artifacts.
2. JSON Schema self-validation where applicable.
3. Fixture validation for valid and invalid fixtures.
4. Golden snapshot comparison.
5. Cross-schema reference validation.
6. Contract-specific drift checks.

Required validation report:

```json
{
  "family": "api",
  "ok": true,
  "artifacts_checked": 3,
  "fixtures_validated": 24,
  "snapshots_checked": 2,
  "drift": [],
  "warnings": []
}
```

## Fixture Contract

Fixture root:

```text
crates/<owner>/tests/fixtures/schema/
  valid/
  invalid/
  snapshots/
  examples/
```

Required per family:

| Fixture | Required |
|---|---:|
| `valid/minimal.json` | yes |
| `valid/full.json` | yes |
| `invalid/missing-required.json` | yes |
| `invalid/unknown-field.json` | yes |
| `invalid/bad-enum.json` | when enums exist |
| `invalid/secret.json` | when public/redacted data exists |
| `snapshots/<artifact>.json` | yes |

Fixture validation must prove both acceptance and rejection. A schema with only
valid fixtures is incomplete.

## Validation Fixtures

The generator itself has fixtures for command dispatch and drift reports.

Required fixtures:

```text
xtask/tests/fixtures/schema-generator/all-families.valid.json
xtask/tests/fixtures/schema-generator/check-report.valid.json
xtask/tests/fixtures/schema-generator/missing-family.invalid.json
xtask/tests/fixtures/schema-generator/stale-artifact.invalid.json
xtask/tests/fixtures/schema-generator/dangling-ref.invalid.json
xtask/tests/fixtures/schema-generator/missing-source-input.invalid.json
```

Required tests:

- aggregate generator lists every schema family from `schemas/README.md`
- `--check` fails when a generated artifact differs
- `--check` fails when a source input is missing
- `--print` emits one selected artifact without writing files
- JSON check report validates against the check-report fixture
- dangling `$ref` is reported as validation failure

## Markdown Reference Generation

Generated markdown references must include:

```text
<!-- generated by cargo xtask schemas <family>; do not edit directly -->
```

Required sections:

- Overview
- Generated Artifacts
- Source Inputs
- Root Shape
- Required Definitions
- Field Tables
- Enum Tables
- Extension Points
- Forbidden Fields
- Examples
- Fixture Paths
- Drift Checks

Markdown output is generated from the same artifact model as JSON output.
Maintaining markdown tables separately from schema JSON is forbidden.

## Cross-Schema Consistency Checks

Aggregate check mode validates:

| Relationship | Check |
|---|---|
| CLI -> API | every `maps_to_dto` exists in API schema |
| MCP -> API | every action request/result DTO exists in API schema |
| OpenAPI -> API | every component schema references API schema |
| OpenAPI -> errors/events | every error/SSE schema references generated error/event schemas |
| provider -> config | provider ids used in config exist in provider schema |
| vector -> metadata | payload fields exist in metadata contract/registry |
| graph -> parser | parser emitted kinds exist in graph schema |
| database -> stores | store-owned tables match table owner metadata |
| app clients -> OpenAPI/API | generated client schemas do not rename fields |
| enum -> events/observability | every `PipelinePhase`, `LifecycleStatus`, and `JobKind` projection exactly matches `axon-api` enums |
| enum -> errors | every `ErrorStage` has either a direct `PipelinePhase` projection or an explicit contextual-boundary rule |
| jobs -> surfaces | every CLI/MCP/REST job kind is a canonical `JobKind`; aliases such as `watch_run` are forbidden |
| database -> runtime docs | required tables exactly match `database-schema.md`; legacy names such as `memory_decay`, `watch_events`, and `job_config_snapshots` are forbidden |
| config -> docs | `.env`, `config.toml`, OpenAPI, MCP, and CLI config schemas expose the same canonical config keys |
| vector -> qdrant index | every generated Qdrant index targets an existing payload field and uses `source_generation`, never bare `generation` |
| removal -> generated surfaces | removed commands, actions, routes, DTO fields, and config keys are absent from generated references |

## CI Contract

Required CI steps for schema work:

```bash
cargo xtask schemas generate --check
cargo xtask check-doc-links
cargo xtask check-doc-contracts
```

When code changes touch any source input path, CI must run the relevant family
check and the aggregate check.

CI fails when:

- generated artifact differs
- fixture validation fails
- source input manifest is stale
- schema has a dangling `$ref`
- public schema includes secret fields
- removed command/action/route appears
- removed config key or DTO/request field appears
- a canonical enum projection adds, drops, or renames a value
- a required SQLite table list adds a legacy table name or drops a canonical table
- a vector payload/index example uses bare `generation` where `source_generation`
  is required
- markdown reference differs from generated schema

## Drift Checks

The generator contract drifts when:

- an individual schema contract names a generator command that is not supported
  by `xtask`
- a schema family exists without a family module
- a generated artifact path exists in a schema contract but not the aggregate
  generator manifest
- a family generator emits JSON without matching markdown when the contract
  requires markdown
- a family generator lacks valid and invalid fixture validation
- aggregate `--check` omits any family listed in `schemas/README.md`
- CI does not run the aggregate schema check for schema-relevant source changes

## Implementation Task Breakdown

Implementation should proceed in this order:

1. Add `xtask/src/schema` command dispatch and artifact model.
2. Add shared JSON/markdown/snapshot helpers.
3. Add API DTO registry and generator.
4. Add error/event generators because transports depend on them.
5. Add CLI/MCP/OpenAPI generators.
6. Add config/provider generators.
7. Add database/graph/vector payload generators.
8. Add aggregate cross-schema consistency checks.
9. Add CI checks and generated reference docs.

## Acceptance Criteria

- `cargo xtask schemas generate` writes all schema artifacts
- `cargo xtask schemas generate --check` passes on a clean tree
- every family generator has source inputs, fixtures, snapshots, and markdown
  output
- aggregate checks validate cross-schema references
- CI fails on stale generated artifacts
- implementation requires no hand-maintained generated markdown tables
- schema contracts are sufficient for an engineer to implement generators
  without inventing architecture
