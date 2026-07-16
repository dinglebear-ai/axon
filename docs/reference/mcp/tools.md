# MCP Tools Reference -- Axon

Axon exposes one MCP operation tool:

| Tool | Purpose | Primary parameter |
|------|---------|-------------------|
| `axon` | Unified action router for source, retrieval, RAG, jobs, memory, graph, watch, provider, and prune operations | `action` |

The live action set is generated into [tool-schema.json](tool-schema.json) and
mirrored in [pipeline-tool-schema.md](pipeline-tool-schema.md). Removed legacy
actions such as `scrape`, `crawl`, `embed`, `ingest`, `code_search`,
`vertical_scrape`, and `purge` are not valid MCP actions.

## Input Shape

```json
{
  "action": "source",
  "source": "https://example.com",
  "scope": "page|site|docs|map",
  "embed": true,
  "response_mode": "artifact|inline|both|auto_inline"
}
```

Common fields:

| Field | Meaning |
|---|---|
| `action` | Required live action name. |
| `subaction` | Operation within grouped actions such as `jobs`, `extract`, `memory`, `watch`, and `graph`. |
| `response_mode` | Optional output policy. Artifact-backed responses return an opaque `artifact_id`; `retrieve` is inline-first. |

## Source Action

`action=source` is the single MCP indexing entrypoint. It maps to
`SourceRequest` and can acquire/index web URLs, local paths, git repositories,
feeds, Reddit/YouTube/session/registry targets, CLI tool output, and MCP tool
output when the caller has the required scopes.

Examples:

```json
{ "action": "source", "source": "https://example.com", "scope": "page", "embed": true }
{ "action": "source", "source": "https://example.com", "scope": "site", "embed": true }
{ "action": "source", "source": "/workspace/project", "scope": "directory", "embed": true }
```

Use `scope=page` for the single-page scrape shape and `scope=site` or
`scope=docs` for crawl-like site acquisition. There is no separate MCP
`scrape` or `crawl` action.

## Common Read Actions

| Action | Purpose |
|---|---|
| `query` | Semantic vector search over indexed content. |
| `retrieve` | Fetch stored chunks/content for a known source URL. |
| `ask` | RAG answer over indexed content. |
| `search` | External web search with Source-backed auto-index side effects. |
| `research` | Web research synthesis with Source-backed auto-index side effects. |
| `map` | Discover URLs/items without embedding. |
| `resolve` | Resolve source identity and adapter route without acquiring content. |
| `capabilities` | Machine-readable action/provider capability document. |
| `providers` | Provider health/capability list and detail views. |

## Grouped Actions

| Action | Notes |
|---|---|
| `jobs` | List, inspect, stream/page events, cancel, retry, recover, cleanup, or clear durable jobs. |
| `extract` | Start or manage structured extraction jobs. |
| `memory` | Remember, search, show, link, supersede, import, and maintain agent memory. |
| `watch` | Create, list, inspect, update, pause, resume, or delete source-backed watches. |
| `graph` | Query SourceGraph kinds, nodes, edges, source subgraphs, and resolution results. |
| `prune` | Plan or execute destructive cleanup by source, generation, or collection. |

## System And Utility Actions

| Action | Purpose |
|---|---|
| `status` | Service/job queue status. |
| `doctor` | Provider and service connectivity checks. |
| `endpoints` | Static endpoint discovery. |
| `screenshot` | Headless Chrome screenshot capture. |
| `brand` | Brand metadata extraction. |
| `diff` | URL/content comparison. |
| `summarize` | URL-context summarization. |
| `evaluate` | RAG quality evaluation. |
| `suggest` | Source/acquisition suggestions. |
| `help` | Action/subaction summary and schema resource links. |

## Removed Actions

These names are intentionally rejected before handler dispatch:

- `crawl`
- `scrape`
- `embed`
- `ingest`
- `code_search`
- `code_search_watch`
- `vertical_scrape`
- `purge`
- `dedupe`
- `sources`
- `domains`
- `stats`
- `elicit_demo`

Use `action=source` for indexing and `action=prune` for destructive cleanup.
Public callers never provide or receive server filesystem paths for artifacts;
they use opaque artifact IDs through the artifact service or REST resource.
