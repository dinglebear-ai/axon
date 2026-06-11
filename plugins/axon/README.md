# axon

Spider-powered self-hosted RAG engine — scrape, map, extract, crawl, embed, and query indexed content via the MCP `axon` tool or the `axon` CLI.

Backed by Qdrant (hybrid dense + BM42 sparse + RRF), TEI for embeddings, optional Chrome (headless) for JS-heavy sites, and a configurable LLM backend for `ask`, `research`, and extract fallback. Gemini headless is the default; OpenAI-compatible endpoints such as llama.cpp are supported with `AXON_LLM_BACKEND=openai-compat`.

## Installation

```bash
claude plugin install <path>
```

The plugin manifest declares a minimal `userConfig` block. Claude Code prompts for the shared Axon server URL, bearer token, optional Tavily/GitHub/Reddit credentials, and optional OAuth settings. Qdrant, TEI, Chrome, Qwen3 embedding, and LLM backend settings are configured by the shared Docker setup path, not by plugin prompts.

The plugin includes two narrow Claude hooks:

- `SessionStart` runs best-effort local setup, then recalls compact `axon memory context` for the current git project when Axon memory is available.
- `ConfigChange` runs the same local setup helper after user settings change.

The hooks are intentionally non-blocking. They do not deploy Docker services; stack provisioning stays explicit.

To provision the stack for the first time, run `/axon-deploy` (or `axon setup` / `axon compose up` on the host directly).

No systemd unit is created. Docker Compose is the only production deployment target. The `.mcp.json` connects Claude Code to `${user_config.server_url}/mcp` with the configured bearer token.

### Session Memory and Auto-Ingest

The SessionStart hook is recall-only and prints best-effort `axon memory context` for the current git project. It does not scan or ingest transcript files during session startup.

For automatic transcript capture, install the host-local watcher:

```bash
axon setup session-watch-service install
```

The service runs `axon sessions watch --no-initial-scan --json` and reuses the existing prepared-session ingest path.

## Commands

| Command | Purpose |
|---------|---------|
| `/axon-deploy [up\|restart\|rebuild]` | On-demand deploy/restart/rebuild of the stack (`axon compose …` + `axon doctor`). This is how you provision the stack. |

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

Default `response_mode: "path"` writes large outputs under the configured Axon appdata root (default `~/.axon/artifacts`) and returns a compact `shape` summary plus an artifact pointer. See the `using-axon` skill for the full action map.

## Skills (2)

The per-action skills were consolidated into a single unified usage skill.

| Skill | Purpose |
|-------|---------|
| `using-axon` | Unified usage guide — full action map and routing for the single `axon` MCP/CLI tool (scrape, crawl, map, extract, search, research, embed, query, ask, ingest, …) |
| `axon-rag-synthesize` | RAG synthesis prompt embedded at compile time into `ask` synthesis (not user-invocable) |

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
