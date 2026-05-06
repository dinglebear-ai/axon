# Axon Documentation

Web crawl, scrape, extract, embed, and query -- all in one binary backed by a self-hosted RAG stack.

## What is Axon

Axon is a trimodal application:

| Mode | Entry point | Port | Purpose |
|------|-------------|------|---------|
| CLI | `axon <command>` | -- | Interactive command-line tool for crawl, scrape, embed, query, ask |
| MCP server | `axon mcp` | 8001 | Single-tool MCP server exposing all CLI operations to AI agents |
| Web UI | `axon serve` | 49000 (backend), 49010 (Next.js) | Supervisor that runs backend bridge, MCP HTTP, workers, shell server, and Next.js dashboard |

All three modes share the same Rust binary, the same services layer, and the same infrastructure stack.

## Structured documentation

### Root

| File | Description |
|------|-------------|
| [README.md](README.md) | This file -- documentation index |
| [SETUP.md](SETUP.md) | Step-by-step setup for local dev and Docker |
| [CONFIG.md](CONFIG.md) | Configuration reference -- axon.json and environment variables |
| [CHECKLIST.md](CHECKLIST.md) | Pre-release quality checklist |
| [GUARDRAILS.md](GUARDRAILS.md) | Security guardrails and safety patterns |
| [INVENTORY.md](INVENTORY.md) | Complete component inventory |

### mcp/

| File | Description |
|------|-------------|
| [mcp/CLAUDE.md](mcp/CLAUDE.md) | Index for MCP docs |
| [mcp/TOOLS.md](mcp/TOOLS.md) | Tool actions, subactions, parameters, examples |
| [mcp/ENV.md](mcp/ENV.md) | MCP-specific environment variables |
| [mcp/TRANSPORT.md](mcp/TRANSPORT.md) | stdio, HTTP, streamable-http transport config |
| [mcp/DEPLOY.md](mcp/DEPLOY.md) | Deployment patterns -- local, Docker, lite mode |
| [mcp/CONNECT.md](mcp/CONNECT.md) | Connect from Claude Code, Codex, Gemini |
| [mcp/DEV.md](mcp/DEV.md) | MCP development workflow |
| [mcp/PATTERNS.md](mcp/PATTERNS.md) | Code patterns -- dispatch, artifacts, error handling |

### repo/

| File | Description |
|------|-------------|
| [repo/CLAUDE.md](repo/CLAUDE.md) | Index for repo docs |
| [repo/REPO.md](repo/REPO.md) | Directory tree, workspace crates, root files |
| [repo/RECIPES.md](repo/RECIPES.md) | Justfile recipes reference |
| [repo/SCRIPTS.md](repo/SCRIPTS.md) | Scripts directory reference |
| [repo/RULES.md](repo/RULES.md) | Coding rules, git workflow, versioning |
| [repo/MEMORY.md](repo/MEMORY.md) | Memory and knowledge persistence |

### stack/

| File | Description |
|------|-------------|
| [stack/CLAUDE.md](stack/CLAUDE.md) | Index for stack docs |
| [stack/ARCH.md](stack/ARCH.md) | Trimodal architecture, worker topology, data flow |
| [stack/TECH.md](stack/TECH.md) | Technology choices -- Rust, Spider, Qdrant, hybrid search |
| [stack/PRE-REQS.md](stack/PRE-REQS.md) | Prerequisites and dependency installation |

## Existing documentation

### Core docs

- [Architecture](./ARCHITECTURE.md) -- detailed system architecture
- [Deployment](./DEPLOYMENT.md) -- production deployment guide
- [Operations](./OPERATIONS.md) -- operational runbooks
- [Performance](./PERFORMANCE.md) -- tuning and benchmarks
- [Security](./SECURITY.md) -- security model
- [Job Lifecycle](./JOB-LIFECYCLE.md) -- async job state machine
- [Testing](./TESTING.md) -- test patterns and coverage

### MCP docs

- [MCP Runtime Guide](./MCP.md) -- MCP server internals
- [MCP Tool Schema](./MCP-TOOL-SCHEMA.md) -- wire contract (source of truth)

### Guide directories

- [commands/](./commands/) -- CLI command deep-dives
- [ingest/](./ingest/) -- ingest pipeline docs
- [auth/](./auth/) -- authentication patterns
- [services/](./services/) -- service layer docs
- [sessions/](./sessions/) -- session ingestion
- [superpowers/](./superpowers/) -- advanced workflows

### Working directories

- [plans/](./plans/) -- feature plans and proposals
- [reports/](./reports/) -- analysis reports

## Quick links

- **First time?** Start with [SETUP.md](SETUP.md), then [stack/ARCH.md](stack/ARCH.md)
- **MCP integration?** [mcp/CONNECT.md](mcp/CONNECT.md) then [mcp/TOOLS.md](mcp/TOOLS.md)
- **Contributing?** [repo/RULES.md](repo/RULES.md) then [repo/RECIPES.md](repo/RECIPES.md)
- **Deploying?** [mcp/DEPLOY.md](mcp/DEPLOY.md) then [CONFIG.md](CONFIG.md)
