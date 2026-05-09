# MCP Environment Variables -- Axon

Environment variables specific to the Axon MCP server. The MCP server inherits all Axon stack variables (Qdrant, TEI, LLM). This page covers MCP-specific configuration.

## MCP server

| Variable | Required | Default | Description | Sensitive |
|----------|----------|---------|-------------|-----------|
| `AXON_MCP_HTTP_HOST` | no | `127.0.0.1` | Bind address for HTTP transport; non-loopback requires `AXON_MCP_HTTP_TOKEN` | no |
| `AXON_MCP_HTTP_PORT` | no | `8001` | Listen port for HTTP transport | no |
| `AXON_MCP_HTTP_TOKEN` | no | unset | Bearer or `x-api-key` token for MCP HTTP requests; required for non-loopback binds | yes |
| `AXON_MCP_ALLOWED_ORIGINS` | no | -- | Comma-separated allowed origins for MCP HTTP CORS (unset = strict default: only same-origin/loopback browser requests pass; non-browser tools unaffected) | no |
| `AXON_MCP_ARTIFACT_DIR` | no | `$AXON_DATA_DIR/artifacts` (default `~/.axon/artifacts`) | Directory for response artifacts | no |
| `AXON_INLINE_BYTES_THRESHOLD` | no | `8192` | Auto-inline payload size threshold (bytes); set to 0 to disable | no |
| `AXON_MCP_EMBED_ALLOWED_ROOTS` | no | -- | Comma-separated local filesystem roots for MCP embed (unset = local file embedding disabled) | no |
| `AXON_MCP_EMBED_MAX_LOCAL_BYTES` | no | -- | Max bytes per local file embedding request via MCP (unset = no per-request size limit; only `AXON_MCP_EMBED_ALLOWED_ROOTS` gates access) | no |

## CLI server mode

These are not MCP transport variables, but they point the host CLI at the same
`axon serve` HTTP process.

| Variable | Required | Default | Description | Sensitive |
|----------|----------|---------|-------------|-----------|
| `AXON_SERVER_URL` | no | unset | Generic CLI server-mode endpoint, for example `http://127.0.0.1:8001`. Supported stateful CLI commands call `/v1/actions`. | no |
| `AXON_LOCAL_MODE` | no | `false` | Force local CLI execution even when `AXON_SERVER_URL` is configured. Equivalent to `--local`. | no |
| `AXON_SERVER_INSECURE` | no | unset | Set to `1` to allow bearer-token auth over plaintext HTTP to non-loopback hosts. Prefer HTTPS instead. | no |
| `AXON_ASK_SERVER_URL` | no | unset | Deprecated compatibility alias for `axon ask` only when `AXON_SERVER_URL` is unset. | no |

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
| `OPENAI_BASE_URL` | LLM provider (legacy path) |
| `OPENAI_API_KEY` | LLM auth |
| `AXON_HEADLESS_GEMINI_MODEL` | Model override for Gemini headless completions |
| `TAVILY_API_KEY` | Web search and research |
| `AXON_LITE` | Enable lite mode (SQLite-backed, default) |
| `AXON_COLLECTION` | Default Qdrant collection |

## Lite mode

The MCP server runs in lite mode by default. Jobs use SQLite and run in-process.

| Operation | Available |
|-----------|-----------|
| scrape, query, ask, search | Yes |
| crawl (sync), embed, ingest | Yes |
| watch scheduler | No |

## Precedence

1. CLI flags override environment variables
2. Environment variables override `~/.axon/config.toml` settings
3. `~/.axon/config.toml` overrides built-in defaults

## See also

- [TRANSPORT.md](TRANSPORT.md) -- transport-specific configuration
- [../CONFIG.md](../CONFIG.md) -- full environment variable reference
