# MCP Environment Variables -- Axon

Environment variables specific to the Axon MCP server. The MCP server inherits all Axon stack variables (Qdrant, TEI, LLM). This page covers MCP-specific configuration.

## MCP server

| Variable | Required | Default | Description | Sensitive |
|----------|----------|---------|-------------|-----------|
| `AXON_MCP_HTTP_HOST` | no | `127.0.0.1` | Bind address for HTTP transport; non-loopback requires `AXON_MCP_HTTP_TOKEN` | no |
| `AXON_MCP_HTTP_PORT` | no | `8001` | Listen port for HTTP transport | no |
| `AXON_MCP_HTTP_TOKEN` | no | unset | Bearer or `x-api-key` token for MCP HTTP requests; required for non-loopback binds | yes |
| `AXON_MCP_AUTH_MODE` | no | `bearer` | Set to `oauth` to enable lab-auth Google OAuth/JWT mode | no |
| `AXON_MCP_PUBLIC_URL` | oauth | -- | Public origin used in OAuth metadata and protected-resource responses | no |
| `AXON_MCP_GOOGLE_CLIENT_ID` | oauth | -- | Google OAuth client ID | yes |
| `AXON_MCP_GOOGLE_CLIENT_SECRET` | oauth | -- | Google OAuth client secret | yes |
| `AXON_MCP_AUTH_ADMIN_EMAIL` | oauth | -- | Admin email accepted by the auth layer; receives full Axon OAuth scopes | yes |
| `AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS` | no | Claude callback included | Additional comma-separated OAuth redirect URIs | no |
| `AXON_MCP_ALLOWED_ORIGINS` | no | -- | Comma-separated allowed origins for MCP HTTP CORS (unset = strict default: only same-origin/loopback browser requests pass; non-browser tools unaffected) | no |
| `AXON_MCP_ARTIFACT_DIR` | no | `$AXON_DATA_DIR/artifacts` (default `~/.axon/artifacts`) | Directory for response artifacts | no |
| `AXON_INLINE_BYTES_THRESHOLD` | no | `8192` | Auto-inline payload size threshold (bytes); set to 0 to disable | no |
| `AXON_TASK_RESULT_WAIT_TIMEOUT_SECS` | no | `300` | Max seconds an MCP `tasks/result` request waits for terminal task state | no |
| `AXON_MCP_EMBED_ALLOWED_ROOTS` | no | -- | Comma-separated local filesystem roots for MCP embed (unset = local file embedding disabled) | no |
| `AXON_MCP_EMBED_MAX_LOCAL_BYTES` | no | `10485760` | Max bytes per local file embedding request via MCP | no |
| `AXON_MCP_EMBED_MAX_LOCAL_DEPTH` | no | `16` | Max directory traversal depth for local directory embedding requests | no |
| `AXON_MCP_EMBED_MAX_LOCAL_ENTRIES` | no | `10000` | Max filesystem entries visited for local directory embedding requests | no |

## Local execution (no CLI server mode)

CLI and MCP commands always run in-process — locally against Qdrant and TEI.
There is no client-to-server forwarding, so `AXON_SERVER_URL`, `AXON_LOCAL_MODE` /
`--local`, and `AXON_SERVER_INSECURE` were removed in 5.0.0. To expose Axon over
HTTP for API clients, run `axon serve` (see [SERVE.md](../commands/serve.md)).

## Transport selection

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `AXON_MCP_TRANSPORT` | no | per-command | `stdio` / `http` / `both`. Overrides the per-command default (`axon mcp` defaults to stdio; `axon serve mcp` defaults to http; `axon serve` to both). |

## Stack variables consumed by MCP

The MCP server reads existing Axon stack variables at startup:

| Variable | Purpose |
|----------|---------|
| `QDRANT_URL` | Vector search and retrieval |
| `TEI_URL` | Embedding generation |
| `AXON_HEADLESS_GEMINI_MODEL` | Model override for Gemini headless completions |
| `AXON_LLM_BACKEND` | LLM backend selector: `gemini-headless` (default) or `openai-compat` |
| `AXON_OPENAI_BASE_URL` | OpenAI-compatible `/v1` base URL when `AXON_LLM_BACKEND=openai-compat` |
| `AXON_OPENAI_MODEL` | OpenAI-compatible model name |
| `AXON_OPENAI_API_KEY` | Optional API key for OpenAI-compatible endpoints |
| `TAVILY_API_KEY` | Tavily fallback for web search and research when `AXON_SEARXNG_URL` is unset |
| `AXON_COLLECTION` | Default Qdrant collection |

## Job runtime

The MCP server uses SQLite for job state and runs workers in-process when the
hosting command creates a worker-enabled service context.

| Operation | Available |
|-----------|-----------|
| scrape, query, ask, search | Yes |
| crawl (sync), embed, ingest | Yes |
| watch scheduler | Yes for wired subcommands (`create`, `list`, `run-now`, `history`) |

## Precedence

1. CLI flags override environment variables
2. Environment variables override `~/.axon/config.toml` settings
3. `~/.axon/config.toml` overrides built-in defaults

## See also

- [TRANSPORT.md](transport.md) -- transport-specific configuration
- [../CONFIG.md](../../guides/configuration.md) -- full environment variable reference
