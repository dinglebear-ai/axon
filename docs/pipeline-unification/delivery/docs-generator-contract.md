# Docs Generator Contract
Last Modified: 2026-06-30

## Contract

This file is the implementation contract for generated documentation.

`documentation-contract.md` defines the desired final documentation tree.
`docs-generator-contract.md` defines the generator architecture, source inputs,
rendering rules, drift checks, fixtures, and CI behavior for keeping generated
reference docs fresh.

Generated docs are not hand-maintained markdown copied from code. They are
rendered from the same registries, schemas, route tables, command tables,
config metadata, and provider capability registries that runtime code uses.

## Target `xtask` Layout

```text
xtask/
  src/
    docs/
      mod.rs
      args.rs
      artifact.rs
      check.rs
      markdown.rs
      manifest.rs
      render.rs
      examples.rs
      families/
        cli.rs
        cli_help.rs
        openapi.rs
        mcp.rs
        api_dto.rs
        api_enums.rs
        errors.rs
        events.rs
        config.rs
        env.rs
        adapters.rs
        schema.rs
        memory.rs
        providers.rs
        presentation.rs
    util/
      diff.rs
      fs.rs
      markdown.rs
      paths.rs
```

Rules:

- one module owns one generated documentation family
- check mode never writes files
- generation mode writes only declared output paths
- markdown rendering is deterministic
- generated docs include source input manifests
- generated docs link to generated schemas where applicable
- hand-authored contract docs may link to generated docs, but must not duplicate
  large generated tables

## CLI Contract

Required commands:

```bash
cargo xtask docs generate
cargo xtask docs generate --check
cargo xtask docs generate --print

cargo xtask docs cli
cargo xtask docs cli-help
cargo xtask docs openapi
cargo xtask docs mcp
cargo xtask docs api-dto
cargo xtask docs api-enums
cargo xtask docs errors
cargo xtask docs events
cargo xtask docs config
cargo xtask docs env
cargo xtask docs adapters
cargo xtask docs schema
cargo xtask docs memory
cargo xtask docs providers
cargo xtask docs presentation
```

Required flags:

| Flag | Meaning |
|---|---|
| `--check` | Generate in memory and fail if files differ. |
| `--print` | Print selected generated markdown to stdout. |
| `--family <name>` | Restrict aggregate generation/check to one family. |
| `--json` | Emit check report as JSON. |
| `--update-snapshots` | Refresh generated doc snapshots; forbidden in CI. |

Exit codes:

| Code | Meaning |
|---:|---|
| `0` | generated/check succeeded |
| `1` | generated docs differ |
| `2` | examples or links failed validation |
| `3` | source input missing or registry incomplete |
| `4` | generator bug/internal error |

## Core Interfaces

```rust
pub trait DocsFamilyGenerator {
    fn family(&self) -> DocsFamily;
    fn source_inputs(&self, repo: &RepoPaths) -> Result<Vec<SourceInput>>;
    fn generate(&self, ctx: &DocsGenerateContext) -> Result<DocsArtifactSet>;
    fn validate(&self, ctx: &DocsValidateContext, artifacts: &DocsArtifactSet) -> Result<DocsValidationReport>;
}
```

```rust
pub struct DocsArtifactSet {
    pub family: DocsFamily,
    pub artifacts: Vec<GeneratedDocArtifact>,
    pub source_inputs: Vec<SourceInput>,
    pub examples: Vec<DocExample>,
}
```

```rust
pub struct GeneratedDocArtifact {
    pub path: Utf8PathBuf,
    pub content: String,
    pub checksum: String,
    pub generated_by: &'static str,
}
```

## Required Family Generators

| Family | Module | Source Inputs | Output |
|---|---|---|---|
| `cli` | `families/cli.rs` | `CliCommandSpec` registry | `docs/reference/cli/commands.md` |
| `cli-help` | `families/cli_help.rs` | clap/help renderer | `docs/reference/cli/axon-help.md` |
| `openapi` | `families/openapi.rs` | generated OpenAPI artifact | `docs/reference/rest/openapi.md` |
| `mcp` | `families/mcp.rs` | generated MCP schema/action registry | `docs/reference/mcp/tool-schema.md` |
| `api-dto` | `families/api_dto.rs` | API DTO schema bundle | `docs/reference/api/dto.md` |
| `api-enums` | `families/api_enums.rs` | enum registry | `docs/reference/api/enums.md` |
| `errors` | `families/errors.rs` | error registry | `docs/reference/api/errors.md` |
| `events` | `families/events.rs` | event schema/phase registry | `docs/reference/runtime/observability.md` |
| `config` | `families/config.rs` | config schema/defaults | `docs/reference/config/config-toml.md` |
| `env` | `families/env.rs` | env schema/example metadata | `docs/reference/config/env.md` |
| `adapters` | `families/adapters.rs` | adapter/scope registry | `docs/reference/sources/adapter-scopes.md` |
| `schema` | `families/schema.rs` | database schema artifact | `docs/reference/runtime/schema.md` |
| `memory` | `families/memory.rs` | memory DTO/store/lifecycle registry | `docs/reference/runtime/memory.md` |
| `providers` | `families/providers.rs` | provider capability registry | `docs/reference/runtime/providers.md` |
| `presentation` | `families/presentation.rs` | presentation token registry | `docs/reference/surfaces/presentation/tokens.md` |

## Generated Header

Every generated markdown file starts with:

```text
<!-- generated by cargo xtask docs <family>; do not edit directly -->
<!-- source inputs: <manifest checksum> -->
```

The header is required for check mode to distinguish generated docs from
hand-authored contracts.

## Source Input Manifest

Every generated doc has a source input manifest:

```json
{
  "generated_by": "cargo xtask docs cli",
  "source_inputs": [
    {
      "path": "crates/axon-cli/src/command_registry.rs",
      "kind": "rust_registry",
      "checksum": "sha256:..."
    }
  ]
}
```

Rules:

- source input paths are repo-relative
- source input checksums are stable SHA-256 values
- generated docs include all direct documentation inputs
- check mode fails when any source input is missing
- check mode fails when a source input changes and generated docs are stale

## Markdown Rendering Contract

Generated markdown uses a shared renderer.

Required rendering rules:

- headings are deterministic
- tables use stable column order
- rows sort by registry order unless the registry defines another order
- code blocks include language tags
- links are repo-relative and validated
- generated examples are validated against schema fixtures
- long tables may include generated anchors
- generated docs do not include timestamps except contract version/source
  manifest metadata

## Required Sections by Family

| Family | Required Sections |
|---|---|
| CLI | overview, command table, flags, examples, removed-command absence |
| REST/OpenAPI | overview, route table, auth scopes, request/response DTOs, errors, SSE |
| MCP | tool definition, action table, subactions, input schema, response envelope |
| API DTO | DTO families, fields, enums, envelopes, extension points |
| Config/env | keys, defaults, env overrides, secret status, examples, removed keys |
| Events | progress event, heartbeat, stream events, phases/statuses |
| Errors | error registry, HTTP/MCP/CLI mapping, retry/cooling policy |
| Adapters | adapter registry, scopes, credentials, graph facts, chunking hints |
| Database | tables, columns, indexes, foreign keys, migrations, store owners |
| Memory | types, statuses, scoring, decay, review, graph/vector integration |
| Providers | provider ids, capabilities, limits, health, scheduler fields |
| Presentation | token registry, platform projections, status mappings |

## Example Validation

Every generated example is executable or schema-valid.

Example categories:

| Example | Validation |
|---|---|
| CLI command | parses into expected DTO |
| REST request | validates against OpenAPI/API schema |
| MCP input | validates against MCP tool schema |
| config TOML | validates against config schema |
| env file | validates against env schema |
| event JSON | validates against event schema |
| payload JSON | validates against vector payload schema |

Examples that cannot be executed in CI must have a fixture proving the shape.

## Link and Anchor Contract

Docs checks validate:

- every markdown link target exists
- every anchor link resolves
- every generated reference doc is linked from the final docs tree
- no generated doc links to removed command/action/route pages
- no generated doc links to missing schema artifacts

## Drift Checks

Docs generation drifts when:

- generated docs differ from source registries
- generated docs differ from generated schemas
- generated docs contain removed aliases/routes/actions
- hand-authored docs duplicate generated tables that should be generated
- source input manifest is stale
- examples fail validation
- generated markdown lacks generated header
- docs inventory omits a generated artifact
- link checker fails

## Validation Fixtures

Required fixtures:

```text
xtask/tests/fixtures/docs-generator/all-families.valid.json
xtask/tests/fixtures/docs-generator/check-report.valid.json
xtask/tests/fixtures/docs-generator/stale-doc.invalid.json
xtask/tests/fixtures/docs-generator/missing-header.invalid.md
xtask/tests/fixtures/docs-generator/broken-link.invalid.md
xtask/tests/fixtures/docs-generator/removed-command.invalid.md
```

Required tests:

- aggregate generator lists every docs family from `documentation-contract.md`
- `--check` fails when generated markdown differs
- `--print` emits one family without writing files
- generated examples validate against schemas
- broken links fail check mode
- removed command/action/route strings fail when present in generated output

## CI Contract

Required CI commands:

```bash
cargo xtask docs generate --check
cargo xtask schemas generate --check
cargo xtask check-doc-links
cargo xtask check-doc-contracts
```

CI fails when:

- generated markdown differs
- generated schema differs
- generated doc source input is stale
- link checker fails
- examples fail validation
- generated doc references removed surfaces
- docs inventory and generated artifacts diverge

## Implementation Order

1. Add `xtask/src/docs` command dispatch and artifact model.
2. Add shared markdown renderer and source manifest support.
3. Generate CLI/help docs from command registry.
4. Generate API/OpenAPI/MCP docs from schema artifacts.
5. Generate config/env/provider/adapter docs from registries.
6. Generate database/memory/presentation docs.
7. Add example validation harness.
8. Add aggregate docs check and CI integration.

## Acceptance Criteria

- `cargo xtask docs generate` writes every generated markdown reference
- `cargo xtask docs generate --check` passes on a clean tree
- every generated doc includes generated header and source input manifest
- every generated example is validated
- generated docs and generated schemas agree
- docs inventory matches generated outputs
- CI fails on stale generated docs
- no large generated table is hand-maintained in contract docs
