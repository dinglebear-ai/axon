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
| `AXON_MCP_EMBED_ALLOWED_ROOTS` | no | -- | Comma-separated local filesystem roots for MCP embed (unset = local file embedding disabled) | no |
| `AXON_MCP_EMBED_MAX_LOCAL_BYTES` | no | -- | Max bytes per local file embedding request via MCP (unset = no per-request size limit; only `AXON_MCP_EMBED_ALLOWED_ROOTS` gates access) | no |
| `AXON_MCP_EMBED_MAX_LOCAL_DEPTH` | no | `8` | Max directory traversal depth for local directory embedding requests | no |
| `AXON_MCP_EMBED_MAX_LOCAL_ENTRIES` | no | `5000` | Max filesystem entries visited for local directory embedding requests | no |

## CLI server mode

These are not MCP transport variables, but they point the host CLI at the same
`axon serve` HTTP process.

| Variable | Required | Default | Description | Sensitive |
|----------|----------|---------|-------------|-----------|
| `AXON_SERVER_URL` | no | unset | Generic CLI server-mode endpoint, for example `http://127.0.0.1:8001`. Supported stateful CLI commands call direct `/v1` REST routes. | no |
| `AXON_LOCAL_MODE` | no | `false` | Force local CLI execution even when `AXON_SERVER_URL` is configured. Equivalent to `--local`. | no |
| `AXON_SERVER_INSECURE` | no | unset | Set to `1` to allow bearer-token auth over plaintext HTTP to non-loopback hosts. Prefer HTTPS instead. | no |

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
| `TAVILY_API_KEY` | Web search and research |
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
