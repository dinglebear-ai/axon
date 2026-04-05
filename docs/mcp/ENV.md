# MCP Environment Variables -- Axon

Environment variables specific to the Axon MCP server. The MCP server inherits all Axon stack variables (Postgres, Redis, RabbitMQ, Qdrant, TEI, LLM). This page covers MCP-specific configuration.

## MCP server

| Variable | Required | Default | Description | Sensitive |
|----------|----------|---------|-------------|-----------|
| `AXON_MCP_HTTP_HOST` | no | `0.0.0.0` | Bind address for HTTP transport | no |
| `AXON_MCP_HTTP_PORT` | no | `8001` | Listen port for HTTP transport | no |
| `AXON_MCP_TRANSPORT` | no | `http` | Transport mode: `http`, `stdio`, or `both` | no |
| `AXON_MCP_ARTIFACT_DIR` | no | `$AXON_DATA_DIR/axon/artifacts` | Directory for response artifacts | no |
| `AXON_INLINE_BYTES_THRESHOLD` | no | `8192` | Auto-inline payload size threshold (bytes) | no |

## OAuth broker (optional)

MCP OAuth is an optional auth system for MCP HTTP clients. It uses `atk_` tokens and is separate from the web UI token model.

| Variable | Required | Default | Description | Sensitive |
|----------|----------|---------|-------------|-----------|
| `GOOGLE_OAUTH_CLIENT_ID` | no | -- | Google OAuth client ID | no |
| `GOOGLE_OAUTH_CLIENT_SECRET` | no | -- | Google OAuth client secret | **yes** |
| `GOOGLE_OAUTH_AUTH_URL` | no | -- | Authorization endpoint | no |
| `GOOGLE_OAUTH_TOKEN_URL` | no | -- | Token endpoint | no |
| `GOOGLE_OAUTH_REDIRECT_URI` | no | -- | Full redirect URI | no |
| `GOOGLE_OAUTH_REDIRECT_HOST` | no | -- | Redirect hostname | no |
| `GOOGLE_OAUTH_REDIRECT_PATH` | no | -- | Redirect path | no |
| `GOOGLE_OAUTH_REDIRECT_POLICY` | no | -- | Redirect policy | no |
| `GOOGLE_OAUTH_SCOPES` | no | -- | OAuth scopes | no |
| `GOOGLE_OAUTH_BROKER_ISSUER` | no | -- | Token issuer | no |
| `GOOGLE_OAUTH_REDIS_URL` | no | -- | Redis URL for token cache | no |
| `GOOGLE_OAUTH_REDIS_PREFIX` | no | -- | Redis key prefix | no |

## Stack variables consumed by MCP

The MCP server reads existing Axon stack variables at startup:

| Variable | Purpose |
|----------|---------|
| `AXON_PG_URL` | Job persistence (full mode) |
| `AXON_REDIS_URL` | Queue state and cancel flags (full mode) |
| `AXON_AMQP_URL` | Job queue dispatch (full mode) |
| `QDRANT_URL` | Vector search and retrieval |
| `TEI_URL` | Embedding generation |
| `OPENAI_BASE_URL` | LLM provider (legacy path) |
| `OPENAI_API_KEY` | LLM auth |
| `OPENAI_MODEL` | Model override for ACP completions |
| `TAVILY_API_KEY` | Web search and research |
| `AXON_LITE` | Run without Postgres/Redis/RabbitMQ |
| `AXON_COLLECTION` | Default Qdrant collection |

## Lite mode

When `AXON_LITE=1`, the MCP server runs without Postgres, Redis, or RabbitMQ. Jobs use SQLite and run in-process. Some operations are unavailable:

| Operation | Available in lite |
|-----------|-------------------|
| scrape, query, ask, search | Yes |
| crawl (sync), embed, ingest | Yes |
| graph, refresh, watch | No |
| export | No |

## Precedence

1. CLI flags override environment variables
2. Environment variables override `axon.json` settings
3. `axon.json` overrides built-in defaults

## See also

- [TRANSPORT.md](TRANSPORT.md) -- transport-specific configuration
- [../CONFIG.md](../CONFIG.md) -- full environment variable reference
