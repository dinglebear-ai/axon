# Axon Plugin

The Axon Claude plugin connects Claude Code to a running Axon server through
MCP-over-HTTP. Axon provides grounded retrieval, unified source indexing,
web search, extraction, durable memory, jobs, watches, graph operations, and
operational diagnostics through one MCP tool named `axon`.

Axon uses SQLite, Qdrant, an embedding provider such as TEI, optional
Chrome/CDP for rendered acquisition, and configured LLM providers.

## Prerequisite

Run Axon before installing the plugin. Supported application deployments are:

- native Axon in an Incus system container under systemd
- native Axon on bare metal under systemd
- the development Compose stack for local work

See [Deployment](../../docs/operations/deployment.md). By default
`axon serve` listens at `http://127.0.0.1:8001` and exposes MCP at
`/mcp`.

## Installation

```bash
claude plugin install <path-to-repo>/plugins/axon
```

The plugin prompts for:

- `server_url`: the running server, default `http://localhost:8001`
- `api_token`: bearer token for `${server_url}/mcp`; omit only for an
  explicitly tokenless loopback development server

Provider endpoints, source credentials, Qdrant, Chrome, embedding, LLM, and
runtime settings belong to `~/.axon/.env` and
`~/.axon/config.toml`, not plugin configuration.

The plugin registers no Claude hooks. Nothing runs automatically at session
start or when configuration changes.

## MCP surface

The server exposes one tool named `axon`. Requests use `action` and, for
lifecycle families, `subaction`.

```json
{ "action": "doctor" }
{ "action": "source", "source": "https://example.com", "scope": "page" }
{ "action": "source", "source": "https://docs.example.com", "scope": "site" }
{ "action": "source", "source": "https://github.com/dinglebear-ai/axon" }
{ "action": "ask", "query": "How does Axon publish source generations?" }
{ "action": "jobs", "subaction": "get", "job_id": "<uuid>" }
```

Large responses default to artifact-backed output; compact results may be
returned inline. The generated MCP contract is authoritative:

- [Tool Contract](../../docs/reference/mcp/tool-contract.md)
- [Generated Tool Schema](../../docs/reference/mcp/tool-schema.md)

## Unified source behavior

`action=source` classifies and indexes web pages and sites, local paths, Git
repositories, package registries, Reddit, YouTube, feeds, session exports,
tool sources, memory records, and uploads through one pipeline.

```json
{ "action": "source", "source": "/home/user/project" }
{ "action": "source", "source": "r/rust" }
{ "action": "source", "source": "https://youtube.com/watch?v=..." }
{ "action": "source", "source": "pkg:npm/react" }
{ "action": "source", "source": "session:claude:/home/user/.claude/projects/..." }
{ "action": "source", "source": "https://docs.example.com", "scope": "site", "detached": true }
```

Use `action=jobs` for detached lifecycle management and `action=watch` for
recurring refreshes.

## CLI fallback

Use the CLI for scripts, cron, advanced source options, or when MCP is
unavailable:

```bash
axon source https://example.com --scope page --wait true
axon source https://docs.example.com --scope site --max-pages 100 --wait true
axon source /home/user/project --wait true
axon jobs get <job-id>
axon watch create https://docs.example.com --every-seconds 86400
```

`axon scrape <url>` remains a one-page CLI projection of the same source
pipeline.

## Memory

Memory recall is explicit. Nothing scans or indexes session transcripts at
plugin startup.

```json
{ "action": "memory", "subaction": "remember", "body": "Use unified source jobs.", "project": "axon" }
{ "action": "memory", "subaction": "context", "project": "axon", "query": "source jobs" }
```

CLI equivalents:

```bash
axon memory remember "Use unified source jobs." --project axon
axon memory context --project axon --query "source jobs"
axon sessions
```

## Plugin command

| Command | Purpose |
|---|---|
| `/axon-deploy [up\|restart\|rebuild]` | Run the configured deployment workflow and doctor checks |

## Skills

The plugin includes focused skills for retrieval, source collection, structured
extraction, QA, lead generation, knowledge-base construction, research, and
memory. The master reference is
[using-axon](skills/using-axon/SKILL.md).

## Troubleshooting

1. Run `axon doctor` on the server host.
2. Verify `${server_url}/healthz` and `${server_url}/mcp` are reachable.
3. Confirm the bearer token and server auth mode.
4. Use `{ "action": "help" }` to inspect the live action map.
5. Use the jobs action for detached-source failures.
