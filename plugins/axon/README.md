# axon

Spider-powered self-hosted RAG engine — scrape, map, extract, crawl, embed, and query indexed content via the MCP `axon` tool or the `axon` CLI.

Backed by Qdrant (hybrid dense + BM42 sparse + RRF), TEI for embeddings, optional Chrome (headless) for JS-heavy sites, and a configurable LLM backend for `ask`, `research`, and extract fallback. Gemini headless is the default; OpenAI-compatible endpoints such as llama.cpp (`AXON_LLM_BACKEND=openai-compat`) and the OpenAI Codex CLI (`AXON_LLM_BACKEND=codex-app-server`) are also supported, and named provider profiles can be saved and switched with `axon config provider use <name>`.

## Installation

```bash
claude plugin install <path>
```

The plugin manifest declares a minimal `userConfig` block. Claude Code prompts for the shared Axon server URL, bearer token, optional Tavily/GitHub/Reddit credentials, and optional OAuth settings. Qdrant, TEI, Chrome, Qwen3 embedding, and LLM backend settings are configured by the shared Docker setup path, not by plugin prompts.

The SessionStart hook invokes `${CLAUDE_PLUGIN_ROOT}/bin/axon setup plugin-hook` directly. It maps the `CLAUDE_PLUGIN_OPTION_*` plugin options to `AXON_*` env vars, refreshes the user's `~/.local/bin/axon` copy, and then **probes the stack — it does not deploy**:

1. Map plugin options (server URL, bearer token, Tavily/GitHub/Reddit credentials, OAuth settings) to `AXON_*` env vars.
2. Probe `/readyz` once (3s) at the configured bind (`AXON_MCP_HTTP_HOST`/`AXON_MCP_HTTP_PORT` from `~/.axon/.env`, default `127.0.0.1:8001`; bind-all hosts are probed over loopback). Because `/readyz` asserts qdrant + tei readiness, a 200 means the whole stack is up.
   - **Up** → exit `0` silently (no stdout in human mode; `--json` prints `{"stack":"already_healthy",...}`).
   - **Down** → print one line, `axon stack not reachable on /readyz — run /axon-deploy to start it`, and exit `0` (non-blocking).
3. Existing `~/.axon/.env` and `~/.axon/config.toml` are never modified by the hook.

**The hook never deploys.** Provisioning is the `/axon-deploy` slash command (or `axon setup` / `axon compose up`). This keeps session start quiet on running hosts and out of the deploy business entirely — a missing prerequisite or a down stack produces a one-line nudge, never a `compose pull`/`up`.

No systemd unit is created, and the plugin-cache binary is not symlinked into `~/.local/bin`. Docker Compose is the only production deployment target. The `.mcp.json` connects Claude Code to `${user_config.server_url}/mcp` with the configured bearer token.

## Commands

| Command | Purpose |
|---------|---------|
| `/axon-deploy [up\|restart\|rebuild]` | Explicit on-demand deploy/restart/rebuild of the stack (`axon compose …` + `axon doctor`). This is how you provision — the SessionStart hook is probe-only and never deploys. |

`~/.axon` is the canonical appdata root for plugin deployments too. Keep `~/.axon/.env`, `~/.axon/config.toml`, jobs, artifacts, output, logs, and service data there.

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

Default `response_mode: "path"` writes large outputs under the configured Axon appdata root (default `~/.axon/artifacts`) and returns a compact `shape` summary plus an artifact pointer. See the `axon` skill for the full action map.

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
