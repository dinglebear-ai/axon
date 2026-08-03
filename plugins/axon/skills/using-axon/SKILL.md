---
name: using-axon
description: Use Axon for grounded retrieval, unified source indexing, web search, extraction, memory, jobs, watches, and cited answers.
---

# Using Axon

Axon is a self-hosted knowledge and retrieval engine with three projections over
one backend:

- **MCP, preferred in Claude Code**: one tool named `axon`, routed by
  `action` and optional `subaction`
- **CLI**: `axon <command> [flags]`, preferred for scripts and advanced source
  options
- **REST**: direct `/v1` routes used by applications and integrations

All three surfaces share the same DTOs, authorization, durable jobs, source
pipeline, SQLite state, Qdrant vectors, and providers.

## Start with help and doctor

Use the live server as the authority for available operations:

```json
{ "action": "help" }
{ "action": "doctor" }
```

CLI health check:

```bash
axon doctor
```

## Choose the operation

| Goal | MCP action | CLI |
|---|---|---|
| Answer from indexed knowledge | `ask` | `axon ask` |
| Semantic search | `query` | `axon query` |
| Retrieve indexed content | `retrieve` | `axon retrieve` |
| Web search and auto-index results | `search` | `axon search` |
| Discover URLs | `map` | `axon map` |
| Index any source | `source` | `axon source` or bare `axon <source>` |
| One-page web capture | `source` with `scope=page` | `axon scrape` |
| Site/docs capture | `source` with `scope=site` | `axon source --scope site` |
| Structured extraction | `extract` | `axon extract` |
| Multi-source research | `research` | `axon research` |
| Summarize a page | `summarize` | `axon summarize` |
| Discover API endpoints | `endpoints` | `axon endpoints` |
| Screenshot | `screenshot` | `axon screenshot` |
| Brand extraction | `brand` | `axon brand` |
| Compare URLs | `diff` | `axon diff` |
| Durable memory | `memory` | `axon memory ...` |
| Detached lifecycle | `jobs` | `axon jobs ...` |
| Recurring refresh | `watch` | `axon watch ...` |
| Source graph | `graph` | `axon graph ...` |

Use `ask` before re-fetching when the answer may already be indexed.

## Unified source indexing

`source` is the single MCP indexing action. It accepts a source string plus an
optional acquisition scope, collection, detached mode, and response mode.

```json
{ "action": "source", "source": "https://example.com", "scope": "page" }
{ "action": "source", "source": "https://docs.example.com", "scope": "site" }
{ "action": "source", "source": "/home/user/project" }
{ "action": "source", "source": "https://github.com/dinglebear-ai/axon" }
{ "action": "source", "source": "r/rust" }
{ "action": "source", "source": "https://youtube.com/watch?v=..." }
{ "action": "source", "source": "https://example.com/feed.xml" }
{ "action": "source", "source": "pkg:npm/react" }
{ "action": "source", "source": "session:claude:/home/user/.claude/projects/..." }
```

Supported source families include web, local, git, package registries, Reddit,
YouTube, feeds, AI sessions, uploads, CLI tools, MCP tools, and memory records.

MCP source calls are synchronous unless `detached: true` is supplied:

```json
{ "action": "source", "source": "https://docs.example.com", "scope": "site", "detached": true }
```

For advanced acquisition controls, use the CLI:

```bash
axon source https://docs.example.com   --scope site   --max-pages 100   --max-depth 3   --wait true   --output-dir .axon/docs

axon source https://docs.example.com   --scope site   --render-mode chrome   --automation-script ./capture.json   --wait true

axon source /home/user/project --wait true
axon source https://github.com/dinglebear-ai/axon --wait true
axon scrape https://example.com/article --wait true
```

Embedding is enabled by default. Use `--skip-embed` only when the requested
result is acquisition output without vector publication.

## Detached jobs

The CLI source command is detached by default unless `--wait true` is used.
MCP source requests are synchronous by default unless `detached: true` is used.

MCP lifecycle:

```json
{ "action": "jobs", "subaction": "get", "job_id": "<uuid>" }
{ "action": "jobs", "subaction": "events", "job_id": "<uuid>" }
{ "action": "jobs", "subaction": "cancel", "job_id": "<uuid>" }
{ "action": "jobs", "subaction": "retry", "job_id": "<uuid>" }
```

CLI lifecycle:

```bash
axon jobs list
axon jobs get <job-id>
axon jobs events <job-id>
axon jobs cancel <job-id>
axon jobs retry <job-id>
axon jobs recover
```

Do not use removed family-specific status commands. Every source family uses the
same durable jobs store.

## Watches

Create recurring source refreshes through the watch surface:

```bash
axon watch create https://docs.example.com --every-seconds 86400
axon watch list
axon watch status <watch-id>
axon watch exec <watch-id>
axon watch pause <watch-id>
axon watch resume <watch-id>
```

Use the equivalent `action=watch` subactions over MCP.

## Query, retrieve, and ask

```json
{ "action": "query", "query": "provider reservations", "limit": 10 }
{ "action": "retrieve", "url": "https://example.com/article" }
{ "action": "ask", "query": "How are provider reservations renewed?" }
{ "action": "ask", "query": "What changed this week?", "since": "7d" }
```

`query` returns ranked matches. `retrieve` returns indexed content for a
specific source or URL. `ask` retrieves, synthesizes, and cites evidence.
Hybrid dense and sparse retrieval is enabled by default when configured.

Only add filters and diagnostics when the user requests them or they are needed
to complete the task.

## Search, map, and research

```json
{ "action": "search", "query": "Rust async cancellation", "search_time_range": "month" }
{ "action": "map", "url": "https://docs.example.com" }
{ "action": "research", "query": "Current MCP authorization patterns" }
```

`search` uses configured web-search providers and indexes results. `map`
performs bounded URL discovery without full indexing. `research` combines
search, source acquisition, retrieval, and synthesis.

## Extraction and web utilities

```json
{ "action": "extract", "urls": ["https://example.com/pricing"], "prompt": "Extract plan, price, and features" }
{ "action": "summarize", "url": "https://example.com/article" }
{ "action": "endpoints", "url": "https://app.example.com" }
{ "action": "brand", "url": "https://example.com" }
{ "action": "diff", "url_a": "https://example.com/v1", "url_b": "https://example.com/v2" }
{ "action": "screenshot", "url": "https://example.com" }
```

Use `axon extract <url> --wait true --json` when a script or file-oriented
workflow needs the CLI.

## Durable memory

Store facts, decisions, constraints, and preferences that should be recalled
across sessions:

```json
{ "action": "memory", "subaction": "remember", "body": "Source jobs are unified.", "project": "axon" }
{ "action": "memory", "subaction": "context", "project": "axon", "query": "source jobs" }
{ "action": "memory", "subaction": "search", "project": "axon", "query": "publication" }
{ "action": "memory", "subaction": "show", "id": "<memory-id>" }
```

Use `memory.context` when broad project recall can improve a task. Use search
and show for targeted lookup.

## Response handling

Large MCP outputs may be artifact-backed. When a response contains a path or
artifact pointer:

1. Read or fetch the artifact through the available host tools.
2. Treat the inline shape as a summary, not the complete result.
3. Preserve citations, source IDs, job IDs, and warnings in the final answer.

Set `response_mode` only when the user or workflow requires a specific output
shape.

## Parameter discipline

- Start with the smallest valid request.
- Do not invent source limits, render modes, filters, collections, or output
  paths.
- Use `scope=page` for one page and `scope=site` for a bounded site capture.
- Use CLI source flags for advanced web acquisition controls not exposed by the
  MCP source DTO.
- Use jobs for lifecycle and watches for recurrence.
- Treat generated CLI and MCP references as authoritative when examples drift.

## Authoritative references

- `docs/reference/cli/commands.md`
- `docs/reference/mcp/tool-contract.md`
- `docs/reference/mcp/tool-schema.md`
- `docs/architecture/source-pipeline.md`
