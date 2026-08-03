# Axon Documentation

Axon is one Rust binary with CLI, MCP, REST, and web surfaces over a shared
services layer, durable job runtime, and unified source pipeline.

The living documentation in this tree describes the current runtime. Generated
references under `reference/` are the machine-readable source of truth for
CLI, MCP, REST, DTO, configuration, database, event, graph, vector, and provider
contracts.

## Documentation map

### `guides/`: setup and task-oriented workflows

| Doc | Purpose |
|---|---|
| [Getting Started](guides/getting-started.md) | local and Docker development setup |
| [Quickstart](guides/quickstart.md) | shortest path to a working Axon stack |
| [Configuration](guides/configuration.md) | `.env`, `config.toml`, and overrides |
| [Local Sources](guides/local-sources.md) | index files, directories, and workspaces |
| [Web Crawls](guides/web-crawls.md) | page and site acquisition |
| [GitHub Repositories](guides/github-repos.md) | repository source workflow |
| [Package Registries](guides/package-registries.md) | crates.io, npm, PyPI, and container registries |
| [Sessions](guides/sessions.md) | Claude, Codex, and Gemini session sources |
| [Ask/RAG](guides/ask-rag.md) | retrieval, synthesis, and citations |
| [Reindexing](guides/reindexing.md) | generation and payload-schema refreshes |

### `reference/`: factual and generated runtime contracts

| Area | Purpose |
|---|---|
| [CLI](reference/cli/overview.md) | generated command registry and help |
| [MCP](reference/mcp/overview.md) | transport, tool contract, and generated schema |
| [REST](reference/rest/overview.md) | routes, OpenAPI, and schemas |
| [API](reference/api/dto.md) | DTOs, enums, errors, and stage results |
| [Configuration](reference/config/config-toml.md) | config and environment schemas |
| [Runtime](reference/runtime/jobs.md) | jobs, ledger, memory, providers, storage, security |
| [Sources](reference/sources/adding-source.md) | adapter, parsing, graph, chunk, and payload contracts |
| [Surfaces](reference/surfaces/web.md) | web, Palette, Android, extension, presentation |
| [Inventory](reference/inventory.md) | components, actions, workers, tables, scripts |

Generated files are marked in their headers. Regenerate them with
`cargo xtask schemas generate` and `cargo xtask docs generate`; do not edit
them by hand.

### `architecture/`: current system design

- [Overview](architecture/overview.md)
- [Source Pipeline](architecture/source-pipeline.md)
- [Crate Structure](architecture/crate-structure.md)
- [Crate Ownership](architecture/crate-ownership.md)
- [Boundary Map](architecture/boundary-map.md)
- [Dependency Layering](architecture/dependency-layering.md)
- [Repository Structure](architecture/repo-structure.md)
- [Fetch Unification](architecture/fetch-unification.md)

### `operations/`: production operation

- [Deployment](operations/deployment.md)
- [Operations](operations/operations.md)
- [Performance](operations/performance.md)
- [Security](operations/security.md)
- [API Token Auth](operations/auth/api-token.md)
- [MCP Auth](operations/auth/mcp-auth.md)

### `development/`: contribution and extension workflows

- [Contributing](development/contributing.md)
- [Testing](development/testing.md)
- [Feature Delivery](development/feature-delivery-framework.md)
- [Adding a Source](development/adding-source.md)
- [Adding a Source Adapter](development/adding-source-adapter.md)
- [Adding a Parser](development/adding-parser.md)
- [Adding a Provider](development/adding-provider.md)
- [Adding a Vector Store](development/adding-vector-store.md)
- [Adding a REST Route](development/adding-rest-route.md)
- [Adding an MCP Action](development/adding-mcp-action.md)
- [Release Checklist](development/release-checklist.md)
- [Repository Rules and Recipes](development/repo/rules.md)

## History directories

The following directories are dated records and are intentionally not kept up
to date. They preserve decisions, investigations, implementation plans, and
review context. They do not override living or generated documentation.

- `sessions/`: session logs
- `plans/`: implementation plans; completed plans remain written history
- `reports/`: reviews, audits, and investigations
- `superpowers/`: plans and specifications produced by superpowers workflows
- `pipeline-unification/`: completed design, contract, plan, and delivery packet
- `perf/`: dated performance snapshots
- `archive/`: removed-runtime documentation retained as history
- `eval/`: evaluation fixtures and notes

## Required Living Documentation

The repository check `cargo xtask docs check` verifies that every named file
below exists. Directory entries containing `...` are descriptive and are not
expanded by the check.

```text
docs/
  README.md
  architecture/
    overview.md
    repo-structure.md
    crate-structure.md
    crate-ownership.md
    source-pipeline.md
    boundary-map.md
    dependency-layering.md
    fetch-unification.md
    stack/
      arch.md
      pre-reqs.md
      tech.md
  reference/
    inventory.md
    public-api-surface.md
    crate-dependency-graph.md
    source-input-manifest.json
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
      transport.md
      connect.md
      deploy.md
    api/
      dto.md
      schemas.json
      enums.md
      errors.md
      errors.schema.json
      stage-results.md
    config/
      config-toml.md
      config.schema.json
      env.md
      env.schema.json
      examples.md
    sources/
      adapter-scopes.md
      adapter-scopes.json
      adding-source.md
      url-normalization.md
      metadata-payload.md
      parsing.md
      chunking.md
      source-graph.md
      graph.md
      graph.schema.json
      vector-payload.md
      vector-payload.schema.json
    runtime/
      jobs.md
      ledger.md
      memory.md
      observability.md
      events.md
      events.schema.json
      providers.md
      provider-capabilities.md
      provider-capabilities.schema.json
      storage.md
      schema.md
      database-schema.md
      database-schema.json
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
    getting-started.md
    quickstart.md
    configuration.md
    local-sources.md
    web-crawls.md
    github-repos.md
    package-registries.md
    sessions.md
    cli-tool-sources.md
    mcp-tool-sources.md
    ask-rag.md
    ask-query-retrieve-search.md
    reindexing.md
  operations/
    deployment.md
    operations.md
    performance.md
    security.md
    auth/
      api-token.md
      mcp-auth.md
  development/
    contributing.md
    testing.md
    feature-delivery-framework.md
    adding-source-adapter.md
    adding-source.md
    adding-parser.md
    adding-provider.md
    adding-vector-store.md
    adding-rest-route.md
    adding-mcp-action.md
    release-checklist.md
    repo/
      repo.md
      rules.md
      recipes.md
      scripts.md
```

## Quick links

- First setup: [Getting Started](guides/getting-started.md)
- Architecture: [Overview](architecture/overview.md)
- Current source path: [Source Pipeline](architecture/source-pipeline.md)
- CLI: [Generated Commands](reference/cli/commands.md)
- MCP: [Connect](reference/mcp/connect.md)
- REST: [Routes](reference/rest/routes.md)
- Deployment: [Deployment](operations/deployment.md)
- Contribution rules: [Repository Rules](development/repo/rules.md)
