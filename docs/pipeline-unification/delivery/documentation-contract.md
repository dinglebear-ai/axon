# Documentation Contract
Last Modified: 2026-06-30

## Contract

Documentation is part of the implementation surface. The final Axon repo has a
deliberate documentation tree, generated references where possible, and checks
that prevent CLI/MCP/REST/config/schema docs from drifting behind code.

Docs are not a compatibility layer. They describe the clean-break target and
the implemented behavior after cutover.

## Documentation Ownership

| Doc Family | Owner | Freshness Source |
|---|---|---|
| CLI help/reference | `axon-cli` | clap command model |
| REST/OpenAPI | `axon-web` + `axon-api` | OpenAPI generator and DTO schemas |
| MCP tool contract | `axon-mcp` + `axon-api` | tool schema generator |
| API/DTO schemas | `axon-api` | serde/schemars exports |
| Config reference | `axon-core` | config structs + defaults |
| Env reference | root/bootstrap | `.env.example` generator |
| Provider capabilities | provider crates | capability registry |
| Adapter scopes | `axon-adapters` | adapter registry |
| Error codes | `axon-error` | error code registry |
| Observability/events | `axon-observe` | event/phase registry |
| DB schema | owning store crates | migrations/schema metadata |
| Crate docs | each crate | `src/CLAUDE.md` + rustdoc |

## Final Docs Tree

```text
docs/
  README.md
  architecture/
    overview.md
    repo-structure.md
    crate-structure.md
    source-pipeline.md
    boundary-map.md
    dependency-layering.md
  reference/
    cli/
      overview.md
      commands.md
      commands.json
      axon-help.md
    rest/
      overview.md
      openapi.md
      openapi.json
      routes.md
      schemas.md
    mcp/
      overview.md
      tool-contract.md
      tool-schema.md
      tool-schema.json
    api/
      dto.md
      schemas.json
      enums.md
      errors.md
      stage-results.md
    config/
      config-toml.md
      config.schema.json
      env.md
      env.schema.json
      examples.md
    sources/
      adapter-scopes.md
      adding-source.md
      url-normalization.md
      metadata-payload.md
      parsing.md
      chunking.md
      source-graph.md
      graph.schema.json
      vector-payload.schema.json
    runtime/
      jobs.md
      ledger.md
      memory.md
      observability.md
      events.schema.json
      providers.md
      provider-capabilities.schema.json
      storage.md
      schema.md
      database.schema.json
      auth.md
      security.md
      redaction.md
      pruning.md
    surfaces/
      web.md
      palette.md
      android.md
      chrome-extension.md
      presentation.md
    memory/
      overview.md
      decay.md
      review.md
    operations/
      doctor.md
      backup-restore.md
      reset.md
      troubleshooting.md
  guides/
    quickstart.md
    local-sources.md
    web-crawls.md
    github-repos.md
    package-registries.md
    sessions.md
    cli-tool-sources.md
    mcp-tool-sources.md
    ask-query-retrieve-search.md
  development/
    contributing.md
    testing.md
    adding-source-adapter.md
    adding-source.md
    adding-parser.md
    adding-provider.md
    adding-vector-store.md
    adding-rest-route.md
    adding-mcp-action.md
    release-checklist.md
  pipeline-unification/
    README.md
    crates/
      README.md
      axon-api/
        README.md
        CLAUDE.md
      ...one directory per target crate...
    schemas/
      README.md
      schema-generator-contract.md
      mcp-tool-schema.md
      openapi-schema.md
      cli-schema.md
      api-dto-schema.md
      config-schema.md
      event-schema.md
      error-schema.md
      database-schema.md
      graph-schema.md
      vector-payload-schema.md
      provider-capability-schema.md
    delivery/
      docs-generator-contract.md
      documentation-contract.md
      testing-contract.md
      current-implementation-sweep.md
      implementation-checklist.md
      issue-pr-draft.md
      contradiction-review.md
      cutover-contract.md
      surface-removal-contract.md
    ...implementation contracts...
```

## Generated Artifacts

As much reference documentation as possible must be generated.

| Output | Generator |
|---|---|
| `docs/reference/cli/commands.md` | `cargo xtask docs cli` from clap |
| `docs/reference/cli/axon-help.md` | `cargo xtask docs cli-help` |
| `docs/reference/cli/commands.json` | `cargo xtask schemas cli` |
| `docs/reference/rest/openapi.md` | `cargo xtask docs openapi` |
| `docs/reference/rest/openapi.json` | `cargo xtask schemas openapi` |
| `docs/reference/rest/schemas.md` | `cargo xtask docs schemas` |
| `docs/reference/mcp/tool-schema.md` | `cargo xtask docs mcp` |
| `docs/reference/mcp/tool-schema.json` | `cargo xtask schemas mcp` |
| `docs/reference/api/dto.md` | `cargo xtask docs api-dto` |
| `docs/reference/api/schemas.json` | `cargo xtask schemas api` |
| `docs/reference/api/enums.md` | `cargo xtask docs api-enums` |
| `docs/reference/api/errors.md` | `cargo xtask docs errors` from `axon-error` |
| `docs/reference/runtime/observability.md` | `cargo xtask docs events` from `axon-observe` |
| `docs/reference/runtime/memory.md` | `cargo xtask docs memory` from `axon-memory` |
| `docs/reference/runtime/events.schema.json` | `cargo xtask schemas events` |
| `docs/reference/config/config-toml.md` | `cargo xtask docs config` |
| `docs/reference/config/env.md` | `cargo xtask docs env` |
| `docs/reference/config/config.schema.json` | `cargo xtask schemas config` |
| `docs/reference/config/env.schema.json` | `cargo xtask schemas config` |
| `docs/reference/sources/adapter-scopes.md` | `cargo xtask docs adapters` |
| `docs/reference/sources/adding-source.md` | `cargo xtask docs new-source` |
| `docs/reference/runtime/schema.md` | `cargo xtask docs schema` from migrations |
| `docs/reference/runtime/database.schema.json` | `cargo xtask schemas database` |
| `docs/reference/sources/graph.schema.json` | `cargo xtask schemas graph` |
| `docs/reference/sources/vector-payload.schema.json` | `cargo xtask schemas vector-payload` |
| `docs/reference/runtime/provider-capabilities.schema.json` | `cargo xtask schemas providers` |
| `docs/reference/surfaces/presentation/tokens.md` | `cargo xtask presentation generate` |
| `docs/reference/surfaces/presentation/tokens.schema.json` | `cargo xtask presentation generate` |

Generated docs must include a header:

```text
<!-- generated by cargo xtask docs ...; do not edit directly -->
```

Generated schema markdown must use the matching schema command in its header:

```text
<!-- generated by cargo xtask schemas ...; do not edit directly -->
```

Hand-authored docs may link to generated docs but must not duplicate large
generated tables.

## Freshness Checks

CI/local checks:

```bash
cargo xtask docs generate --check
cargo xtask schemas generate --check
cargo xtask check-doc-links
cargo xtask check-doc-contracts
```

The implementation contract for `cargo xtask docs ...` lives in
[docs-generator-contract.md](docs-generator-contract.md).

Checks must fail when:

- clap help differs from CLI docs
- OpenAPI differs from REST docs
- MCP schema differs from tool docs
- config structs/defaults differ from config docs
- error code registry differs from error docs
- event/phase registry differs from observability docs
- adapter capabilities differ from adapter-scope docs
- migrations differ from schema docs
- markdown links are broken
- generated JSON schema headers or `x-axon.generated_by` values use stale
  generator names

## `src/CLAUDE.md` Rule

Every non-trivial crate has `crates/<crate>/src/CLAUDE.md` containing:

- crate purpose
- public modules
- ownership boundaries
- must-not-own list
- test commands
- common gotchas

`CLAUDE.md` is the source of truth. Sibling `AGENTS.md` and `GEMINI.md` are
symlinks when present.

## Testing Requirements

- generated docs are reproducible
- generated schemas are reproducible
- generated docs check mode fails on drift
- schema check mode fails on drift
- link checker covers all markdown docs
- crate docs exist for every non-trivial crate
- docs inventory matches repo structure contract
- examples in docs are smoke-tested where practical
