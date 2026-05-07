# axon

Spider-powered self-hosted RAG engine — scrape, map, extract, crawl, embed, and query indexed content via the MCP `axon` tool or the `axon` CLI.

Backed by Qdrant (hybrid dense + BM42 sparse + RRF), TEI for embeddings, optional Chrome (headless) for JS-heavy sites, and a Gemini headless LLM for `ask`, `research`, and extract fallback.

## Installation

```bash
claude plugin install <path>
```

The plugin manifest declares a `userConfig` block — Claude Code prompts for Qdrant URL, TEI URL, collection name, LLM endpoint/model/API key, Tavily API key, and Chrome remote URL on install. These flow into the MCP server via `${user_config.*}` substitution in `.mcp.json`.

The `axon` binary must be on `$PATH` (e.g., `cargo install --path . --bin axon`). The MCP server starts on demand via `axon mcp` (stdio transport).

## MCP Server

Single tool: `mcp__axon__axon`. Routed by `action` plus an optional `subaction` for lifecycle families.

```json
{ "action": "doctor" }
{ "action": "scrape", "url": "https://example.com" }
{ "action": "ask", "query": "How does axon handle Chrome auto-switching?" }
{ "action": "crawl", "subaction": "status", "job_id": "<uuid>" }
```

Response envelope:

```json
{ "ok": true, "action": "...", "subaction": "...", "data": { ... } }
```

Default `response_mode: "path"` writes large outputs to `.cache/axon-mcp/` and returns a compact `shape` summary plus an artifact pointer. See the `axon` skill for the full action map.

## Skills (16)

| Skill | Purpose |
|-------|---------|
| `axon` | Meta-skill: full action map and routing guide |
| `ask` | RAG: retrieve + LLM-synthesized answer with citations |
| `crawl` | Recursive site crawl (async by default) |
| `doctor` | Service health check (Qdrant / TEI / Chrome / Tavily / LLM) |
| `domains` | Indexed domains summary |
| `embed` | Embed local files / dirs / URLs into Qdrant |
| `extract` | LLM-powered structured data extraction |
| `ingest` | GitHub / Reddit / YouTube / AI-session ingestion |
| `map` | URL discovery (sitemap-first, anchor-fallback, no fetch) |
| `query` | Pure semantic vector search (no LLM) |
| `retrieve` | Fetch all chunks indexed for a specific URL |
| `scrape` | Single-URL or small-batch scrape to markdown |
| `search` | Tavily web search; auto-queues crawl for results |
| `sources` | List indexed URLs with chunk counts |
| `stats` | Qdrant collection statistics |
| `status` | Job queue snapshot |

## Agents

- `researcher` — autonomous discover → fetch → embed → synthesize pipeline. Invoked when the index lacks coverage on a topic and the user wants a grounded, cited answer.

## Layout

```
plugins/axon/
├── README.md                  — this file
├── CHANGELOG.md
├── .mcp.json                  — MCP server config (stdio, ${user_config.*})
├── agents/
│   └── researcher.md
└── skills/
    ├── axon/SKILL.md          — meta-skill
    ├── ask/SKILL.md
    ├── crawl/SKILL.md
    ├── doctor/SKILL.md
    ├── domains/SKILL.md
    ├── embed/SKILL.md
    ├── extract/SKILL.md
    ├── ingest/SKILL.md
    ├── map/SKILL.md
    ├── query/SKILL.md
    ├── retrieve/SKILL.md
    ├── scrape/SKILL.md
    ├── search/SKILL.md
    ├── sources/SKILL.md
    ├── stats/SKILL.md
    └── status/SKILL.md
```
