# Config Examples

Last Modified: 2026-07-19

Worked configuration examples for common Axon deployments. The authoritative
key reference is [`config.example.toml`](../../../config.example.toml) (TOML
tuning) and [`.env.example`](../../../.env.example) (URLs/secrets/auth).
Generated schema references:
[`config-toml.md`](config-toml.md), [`env.md`](env.md),
[`config.schema.json`](config.schema.json), [`env.schema.json`](env.schema.json).

## Precedence

```text
CLI flags  >  environment variables  >  ~/.axon/config.toml  >  built-in defaults
```

`.env` holds URLs, secrets, auth, and runtime bootstrap. `config.toml` holds
non-secret tuning (search, workers, chunking, providers). Service endpoint
URLs are **not** accepted from `config.toml` — they must be env vars
(`QDRANT_URL`, `TEI_URL`, `AXON_CHROME_REMOTE_URL`).

## Example A — minimal local loopback (bearer)

`~/.axon/.env`:

```bash
QDRANT_URL=http://127.0.0.1:53333
TEI_URL=http://127.0.0.1:52000
AXON_CHROME_REMOTE_URL=http://127.0.0.1:6000
AXON_COLLECTION=axon
AXON_HTTP_HOST=127.0.0.1        # tokenless allowed only on loopback
AXON_HTTP_PORT=8001
AXON_AUTH_MODE=bearer           # default; AXON_HTTP_TOKEN optional on loopback
```

`~/.axon/config.toml`:

```toml
[server]
default-collection = "axon"
```

`axon setup init` generates a random `AXON_HTTP_TOKEN` and writes both files.
Tokenless HTTP is allowed **only** when `AXON_HTTP_HOST=127.0.0.1`; non-loopback
binds require OAuth or a set `AXON_HTTP_TOKEN`.

## Example B — OpenAI-compatible LLM backend

For llama.cpp server, LM Studio, vLLM, etc.:

`~/.axon/.env`:

```bash
AXON_LLM_BACKEND=openai-compat
AXON_OPENAI_BASE_URL=http://127.0.0.1:8080/v1   # API root; NOT /chat/completions
AXON_OPENAI_API_KEY=...
AXON_SYNTHESIS_OPENAI_MODEL=llama3.1-70b        # ask/evaluate/suggest/extract/research
AXON_CHAT_OPENAI_MODEL=                          # empty = use synthesis model
# AXON_OPENAI_MODEL is the legacy alias for AXON_SYNTHESIS_OPENAI_MODEL
```

`~/.axon/config.toml`:

```toml
[providers.llm]
backend = "openai-compat"
completion-concurrency = 4          # 1-64
completion-timeout-secs = 300       # 10-1800

[providers.embedding]
# If using an OpenAI-compatible embedding endpoint instead of TEI:
openai-model = "axon-qwen3-embedding"
openai-max-client-batch-size = 32
openai-max-concurrent = 32
```

Alternatives: `AXON_LLM_BACKEND=gemini-headless` (default; OAuth via
`$HOME/.gemini` or `GEMINI_API_KEY`), or `AXON_LLM_BACKEND=codex-app-server`
(`codex app-server` over stdio in an isolated `CODEX_HOME`).

## Example C — external Qdrant + SearXNG

When Qdrant lives on another host and you self-host search:

`~/.axon/.env`:

```bash
QDRANT_URL=http://qdrant.example.internal:6333
AXON_SEARXNG_URL=http://searxng.example.internal:8080
TAVILY_API_KEY=                     # optional; SearXNG takes precedence when set
GITHUB_TOKEN=...                    # optional adapter credentials
```

`~/.axon/config.toml`:

```toml
[providers.vector]
upsert-batch-points = 1024
write-concurrency = 1
hybrid-enabled = true               # requires named-mode collection (dense + BM42)
hnsw-ef = 128

[providers.search]
research-full-content = true        # fetch full page + synthesize vs snippet-only
```

For Docker Compose, set `AXON_QDRANT_URL` (the compose-interpolation var) to
the external host. SearXNG must have JSON format enabled in its `settings.yml`.

## Example D — OAuth mode (non-loopback bind)

To expose Axon on a non-loopback address behind Google OAuth:

`~/.axon/.env`:

```bash
AXON_HTTP_HOST=0.0.0.0
AXON_HTTP_PORT=8001
AXON_AUTH_MODE=oauth
AXON_PUBLIC_URL=https://axon.example.com
AXON_GOOGLE_CLIENT_ID=...
AXON_GOOGLE_CLIENT_SECRET=...
AXON_AUTH_ADMIN_EMAIL=admin@example.com        # grants admin scope
AXON_ALLOWED_REDIRECT_URIS=https://axon.example.com/callback
AXON_ALLOWED_ORIGINS=https://axon.example.com
```

OAuth email allowlisting is the access boundary. Allowed OAuth users receive
full Axon server access; newly issued tokens default to both `axon:read` and
`axon:write`. `AXON_HTTP_TOKEN` may remain unset in OAuth mode (and is still
accepted in dual-mode).

## Common rules

- `AXON_CONFIG_PATH=/path/to/config.toml` overrides the config file location.
- `mkdir -m 700 ~/.axon` and `chmod 600 ~/.axon/config.toml`.
- Unknown TOML keys and stale pre-contract section names (`[llm]`, `[tei]`,
  `[scrape]`, `[workers]`) fail with a clear deprecation error naming the new
  section — they are not silently accepted as aliases.
- Local-path source requests from server transports are fail-closed unless
  under `AXON_SOURCE_LOCAL_ALLOWED_ROOTS` (comma-separated).

## Review rule

Do not add new config keys without updating `config.example.toml`, the
generated schema (`cargo xtask schemas config`), and migration guidance.
Removed keys must fail clearly rather than being silently accepted.

If a config example becomes stale, update it alongside `config.example.toml`
and `.env.example` in the same PR.
