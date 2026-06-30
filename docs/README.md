# Axon Documentation

Web crawl, scrape, extract, embed, and query — all in one binary backed by a self-hosted RAG stack.

> Current runtime docs describe the pre-#298 implementation. The clean-break
> source-pipeline contracts live in
> [`pipeline-unification/`](pipeline-unification/README.md) and supersede old
> command/action/route shapes when that refactor lands.

## What is Axon

Axon is a trimodal application:

| Mode | Entry point | Port | Purpose |
|------|-------------|------|---------|
| CLI | `axon <command>` | — | Interactive command-line tool for crawl, scrape, summarize, embed, query, ask |
| MCP server | `axon mcp` | 8001 | Single-tool MCP server exposing all CLI operations to AI agents |
| Web panel + HTTP API | `axon serve` | 8001 | Unified HTTP server for web panel, MCP, and direct `/v1` REST routes |

All three modes share the same Rust binary, the same services layer, and the same infrastructure stack.

## Documentation map

Living/reference docs are grouped by intent. Dated, point-in-time records (session logs,
reviews, plans) live under the history directories at the bottom.

### `guides/` — getting started & task-oriented how-to

| Doc | Description |
|-----|-------------|
| [guides/getting-started.md](guides/getting-started.md) | Step-by-step setup for local dev and Docker |
| [guides/configuration.md](guides/configuration.md) | Configuration reference — `~/.axon/config.toml` and environment variables |
| [guides/ask-rag.md](guides/ask-rag.md) | The `ask` RAG pipeline — retrieval, synthesis, citations |
| [guides/reindexing.md](guides/reindexing.md) | Re-indexing and payload schema upgrades |
| [guides/context-injection.md](guides/context-injection.md) | Context-injection mechanics |
| [guides/ingest/](guides/ingest/) | Ingest pipeline + per-source deep-dives (GitHub, GitLab, Reddit, YouTube, sessions) |

### `reference/` — factual reference

| Doc | Description |
|-----|-------------|
| [reference/actions/](reference/actions/) | CLI reference — one page per command |
| [reference/mcp/](reference/mcp/) | MCP server: overview, tool schema, transport, connect, deploy, env, tools, patterns |
| [reference/http-api.md](reference/http-api.md) | HTTP API surface (`axon serve`) |
| [reference/api-parity.md](reference/api-parity.md) | CLI ↔ MCP ↔ HTTP action parity matrix |
| [reference/endpoints.md](reference/endpoints.md) | `endpoints` discovery — API/RPC endpoint extraction |
| [reference/shell-completions.md](reference/shell-completions.md) | Shell completion generation |
| [reference/cargo-features.md](reference/cargo-features.md) | Cargo feature flag matrix |
| [reference/spider-feature-flags.md](reference/spider-feature-flags.md) | Spider.rs feature flags and observable behavior |
| [reference/job-lifecycle.md](reference/job-lifecycle.md) | Async job state machine (SQLite-backed) |
| [reference/inventory.md](reference/inventory.md) | Complete component + command inventory |
| [reference/qdrant-payload-schema.md](reference/qdrant-payload-schema.md) | Qdrant point payload contract |
| [reference/env-matrix.md](reference/env-matrix.md) | Environment variable migration matrix |

### `architecture/` — system design

| Doc | Description |
|-----|-------------|
| [architecture/overview.md](architecture/overview.md) | System architecture diagrams and data flow |
| [architecture/stack/](architecture/stack/) | Trimodal architecture, technology choices, prerequisites |
| [architecture/specs/](architecture/specs/) | Feature specifications (vertical extractors, android, active design notes) |

### `pipeline-unification/` — active future contract

| Doc | Description |
|-----|-------------|
| [pipeline-unification/](pipeline-unification/README.md) | Clean-break contract packet for the unified source pipeline tracked by GitHub issue #298 |

### `operations/` — running it in production

| Doc | Description |
|-----|-------------|
| [operations/deployment.md](operations/deployment.md) | Production deployment guide |
| [operations/operations.md](operations/operations.md) | Operational runbooks and recovery procedures |
| [operations/performance.md](operations/performance.md) | Tuning guide and benchmark results |
| [operations/security.md](operations/security.md) | Security model, SSRF guards, port boundaries |
| [operations/auth/](operations/auth/) | Authentication — MCP auth + static API token |

### `contributing/` — development & repo conventions

| Doc | Description |
|-----|-------------|
| [contributing/rust.md](contributing/rust.md) | Rust conventions and best practices |
| [contributing/testing.md](contributing/testing.md) | Test strategy, how to run, coverage targets |
| [contributing/monolith-policy.md](contributing/monolith-policy.md) | File/function size policy and enforcement |
| [contributing/guardrails.md](contributing/guardrails.md) | Security guardrails and safety patterns |
| [contributing/checklist.md](contributing/checklist.md) | Pre-release quality checklist |
| [contributing/feature-delivery-framework.md](contributing/feature-delivery-framework.md) | Feature development process |
| [contributing/desktop-palette-testing.md](contributing/desktop-palette-testing.md) | Desktop palette testing harness |
| [contributing/repo/](contributing/repo/) | Repo tree, coding rules, Justfile recipes, scripts, memory |

### History (dated records — not kept up to date)

- [sessions/](sessions/) — session logs (`YYYY-MM-DD-HH-MM-description.md`)
- [reports/](reports/) — code reviews, audits, analysis
- [plans/](plans/) — implementation plans (`plans/complete/` = archived)
- [superpowers/](superpowers/) — superpowers plans/specs
- [perf/](perf/) — dated performance snapshots
- [archive/](archive/) — historical removed-runtime docs (do not edit)
- [eval/](eval/) — evaluation datasets and fixtures

## Quick links

- **First time?** [guides/getting-started.md](guides/getting-started.md), then [architecture/stack/arch.md](architecture/stack/arch.md)
- **MCP integration?** [reference/mcp/connect.md](reference/mcp/connect.md), then [reference/mcp/tools.md](reference/mcp/tools.md)
- **Contributing?** [contributing/repo/rules.md](contributing/repo/rules.md), then [contributing/repo/recipes.md](contributing/repo/recipes.md)
- **Deploying?** [reference/mcp/deploy.md](reference/mcp/deploy.md), then [guides/configuration.md](guides/configuration.md)
