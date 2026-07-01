# axon

Spider-powered self-hosted RAG engine — scrape, map, extract, crawl, embed, and query indexed content via the MCP `axon` tool or the `axon` CLI.

> Current pre-#298 plugin docs. The future source-pipeline contract lives in
> `docs/pipeline-unification/`; after that cutover source acquisition should
> route through the shared source action/model rather than legacy crawl/embed/
> ingest action families.

Backed by Qdrant (hybrid dense + BM42 sparse + RRF), TEI for embeddings, optional Chrome (headless) for JS-heavy sites, and a configurable LLM backend for `ask`, `research`, and extract fallback. Gemini headless is the default; OpenAI-compatible endpoints such as llama.cpp are supported with `AXON_LLM_BACKEND=openai-compat`.

## Installation

```bash
claude plugin install <path>
```

The plugin manifest declares a minimal `userConfig` block. Claude Code prompts
only for connection details for an already-running Axon server.

The current plugin prompt surface is intentionally small:

- `server_url` — base URL for a running `axon serve` instance, defaulting to `http://localhost:8080`.
- `api_token` — bearer token sent to `${server_url}/mcp`; leave empty only for loopback/unauthenticated development instances.

Search providers, ingest credentials, Qdrant, TEI, Chrome, embedding, and LLM backend settings live in the shared Axon host configuration (`~/.axon/.env` and `~/.axon/config.toml`), not in plugin prompts.

The plugin includes two narrow Claude hooks:

- `SessionStart` runs best-effort local setup, then recalls compact `axon memory context` for the current git project when Axon memory is available.
- `ConfigChange` runs the same local setup helper after user settings change.

The hooks are intentionally non-blocking. They do not deploy Docker services; stack provisioning stays explicit.

To provision the stack for the first time, run `/axon-deploy` (or `axon setup` / `axon compose up` on the host directly).

No systemd unit is created. Docker Compose is the only production deployment target. The `.mcp.json` uses HTTP transport and connects Claude Code to `${user_config.server_url}/mcp` with the configured bearer token.

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

The Axon MCP server exposes a single `axon` tool. Hosts generate the concrete
tool name, so use the current environment's generated name. Requests are routed
by `action` plus an optional `subaction` for lifecycle families.

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

## Skills

The plugin ships 25 plain-name Axon skills under `skills/`. Because these
skills already live inside the Axon plugin namespace, folder names do not carry
an `axon-` prefix. Every skill includes `agents/openai.yaml` metadata.

Action skills cover the core CLI/MCP surfaces; workflow skills cover
outcome-focused research, monitoring, QA, shopping, and design deliverables.

| Skill | Purpose |
|-------|---------|
| `using-axon` | Unified usage guide for the single `axon` MCP/CLI surface. |
| `cli`, `crawl`, `download`, `extract`, `map`, `scrape`, `search`, `monitor` | Core Axon command and action workflows. |
| `company-directories`, `competitive-intel`, `dashboard-reporting`, `deep-research`, `demo-walkthrough`, `knowledge-base`, `knowledge-ingest`, `lead-gen`, `lead-research`, `market-research`, `qa`, `research-papers`, `seo-audit`, `shop`, `website-design-clone`, `workflows` | Outcome-focused Axon workflow skills. |

The `download` skill documents Axon's current composed capture workflow:
`scrape`, `crawl --output-dir`, and `screenshot`. It is not a promise that Axon
already has a single offline-site mirroring command that rewrites linked assets
for fully browsable local copies.

The runtime RAG synthesis prompt is stored under
`references/rag-synthesize/SKILL.md` and embedded into `ask` synthesis at compile
time. It is intentionally not exposed as a user-invocable plugin skill.

## Agents

- `researcher` — autonomous discover → fetch → embed → synthesize pipeline. Invoked when the index lacks coverage on a topic and the user wants a grounded, cited answer.

## Layout

```
plugins/axon/
├── README.md                  — this file
├── CHANGELOG.md
├── .claude-plugin/
│   └── plugin.json            — plugin manifest and userConfig
├── .mcp.json                  — MCP server config (HTTP, ${user_config.*})
├── agents/
│   └── researcher.md
├── commands/
│   └── axon-deploy.md
├── examples/
│   └── workflow-output-templates.md
├── hooks/
│   └── hooks.json
├── references/
│   ├── capture-recipes.md
│   ├── workflow-authoring.md
│   └── rag-synthesize/
│       ├── SKILL.md          — runtime RAG synthesis prompt
│       └── example-response.md
├── scripts/
│   ├── plugin-setup.sh
│   └── session-start-memory-context.sh
└── skills/
    ├── using-axon/
    │   ├── SKILL.md          — meta-skill
    │   └── agents/openai.yaml
    ├── cli/SKILL.md
    ├── crawl/SKILL.md
    ├── download/SKILL.md
    ├── extract/SKILL.md
    ├── map/SKILL.md
    ├── scrape/SKILL.md
    ├── search/SKILL.md
    ├── monitor/SKILL.md
    └── <workflow-name>/SKILL.md
```
