---
title: "Axon Configuration"
created: 2026-04-04
updated: 2026-07-30
---

# Axon Configuration
Last Modified: 2026-07-24

Axon uses two user-editable files under `~/.axon/`:

| File | Owns | Does not own |
|---|---|---|
| `~/.axon/.env` | Secrets, endpoint URLs, auth/runtime bootstrap, trusted local override paths, Docker Compose interpolation | Non-secret tuning knobs |
| `~/.axon/config.toml` | Non-secret tuning defaults across `[server]`, `[sources]`, `[pipeline]`, `[jobs]`, `[providers]`, `[retrieval]`, `[ask]`, `[crawl]`, and `[watch]` — see `config.example.toml` | Secrets, endpoint URLs, OAuth client secrets, bearer tokens |

## Precedence (highest to lowest)

1. CLI flags for command inputs (`--collection`, `--wait`, etc.)
2. Environment variables for secrets, URLs, auth/runtime, and bootstrap
3. `~/.axon/config.toml` for non-secret tuning
4. Built-in defaults

Service endpoint URLs are intentionally not accepted from `config.toml`.
Use `QDRANT_URL`, `TEI_URL`, and `AXON_CHROME_REMOTE_URL` from the env layer.
`QDRANT_URL` and `TEI_URL` also have temporary CLI overrides for one-off
diagnostics.

The clean break does not read deprecated aliases. Update live environment files
before restarting Axon:

| Remove | Canonical setting |
|---|---|
| `AXON_OPENAI_MODEL` | `AXON_SYNTHESIS_OPENAI_MODEL` |
| `AXON_MCP_EMBED_ALLOWED_ROOTS` | `AXON_SOURCE_LOCAL_ALLOWED_ROOTS` |
| `AXON_HNSW_EF_SEARCH_LEGACY` | Remove; use `AXON_HNSW_EF_SEARCH` or `[providers.vector].hnsw-ef` |
| `[services].qdrant-url`, `.tei-url`, `.chrome-remote-url` | `QDRANT_URL`, `TEI_URL`, `AXON_CHROME_REMOTE_URL` in `.env` |
| `[ask].backend` | `[providers.llm].backend` or `AXON_LLM_BACKEND` |
| `[pipeline].ingest-lanes`, `.embed-lanes` | Remove — zero runtime consumers ever read these; worker parallelism is `[pipeline].unified-worker-concurrency` |
| `[pipeline].max-pending-crawl-jobs`, `.max-pending-embed-jobs`, `.max-pending-extract-jobs`, `.max-pending-ingest-jobs` | Remove — nothing ever enforced these queue caps |
| `[pipeline].crawl-job-concurrency-limit` / `AXON_CRAWL_JOB_CONCURRENCY_LIMIT` | `[pipeline].max-active-source-jobs` / `AXON_SOURCE_JOB_CONCURRENCY_LIMIT` (now gates every source job kind, not just crawls; default changed 1 → 4) |
| `[jobs].crawl-job-timeout-secs` / `AXON_CRAWL_JOB_TIMEOUT_SECS` | Remove — no timeout was ever enforced against a running job |

`axon setup config rewrite --dry-run` previews environment cleanup. Removed
keys fail with this migration guidance instead of being silently accepted.

## Canonical `~/.axon/` layout

`~/.axon/` is the canonical home for all Axon user-level config, secrets, runtime state, infrastructure data, and generated output. All app data lives directly under this directory — no nested `axon/` subdirectory.

```
~/.axon/
├── config.toml              # tuning knobs (CLI > env > this > default)
├── .env                     # URLs + secrets (loaded after AXON_ENV_FILE,
│                            #   before repo-root .env ancestor walk)
│
├── jobs.db                  # SQLite job queue
├── jobs.db-wal
├── jobs.db-shm
│
├── output/                  # scraped markdown / HTML / JSON
├── logs/
│   └── axon.log             # size-rotated, 10 MiB default
├── artifacts/               # MCP JSON artifacts (response_mode=path)
├── screenshots/             # spider chrome_store_page captures
├── chrome-diagnostics/      # opt-in browser diagnostics
│
├── qdrant/                  # Docker Compose Qdrant bind mount
├── tei/                     # Docker Compose TEI model/cache data
└── lab-auth/                # OAuth/lab-auth state for server deployments
```

`AXON_DATA_DIR` defaults to `~/.axon` for the binary. Docker Compose uses `AXON_HOME` for host-side bind mounts and defaults it to `${HOME}/.axon`; keep `AXON_HOME` and `AXON_DATA_DIR` aligned unless you are deliberately relocating the entire Axon appdata tree.

### Migration from `~/.local/share/axon`

If you previously stored axon data under `~/.local/share/axon/`, axon does NOT auto-migrate. Either move the directory yourself (`mv ~/.local/share/axon ~/.axon`), or set `AXON_DATA_DIR=~/.local/share` explicitly to keep the old location. Tuning knobs that were previously env-only are now also accepted in `~/.axon/config.toml`.

## Environment files

Three env files are auto-loaded in this order; the first one that exists and parses wins (later files do **not** override earlier ones):

| Order | Path | Notes |
|-------|------|-------|
| 1 | `$AXON_ENV_FILE` | Explicit override; only consulted when set |
| 2 | `~/.axon/.env` | Canonical user-level secrets, loaded automatically |
| 3 | First `.env` found by walking ancestors of CWD (or the binary's parent) | Repo-root `.env` fallback for development only |

Docker Compose also reads `~/.axon/.env` by default as the service env file and uses `AXON_HOME` for host bind mounts:

| File | Purpose | Loaded by |
|------|---------|-----------|
| `~/.axon/.env` | Canonical app runtime variables, secrets, and Docker Compose interpolation | `dotenvy` in binary; `docker compose --env-file ~/.axon/.env`; compose service `env_file` |
| Repo `.env` | Development fallback only | `dotenvy` ancestor walk; `scripts/axon` only when `~/.axon/.env` is absent |

```bash
mkdir -m 700 -p ~/.axon
cp .env.example ~/.axon/.env
chmod 600 ~/.axon/.env
```

`axon setup init` is non-destructive: it adds missing required runtime keys and fills blank generated auth tokens, but it does not prune unknown keys.

If `AXON_ENV_FILE` is set, Axon treats that file as the effective env file.

## Local execution and HTTP API access

The `axon` CLI and MCP server always run actions in-process — locally against
Qdrant and TEI. There is no client-to-server forwarding (the `AXON_SERVER_URL`
env var, the `--local` / `AXON_LOCAL_MODE` flag, and `AXON_SERVER_INSECURE` were
removed in 5.0.0).

To expose Axon over HTTP for external API clients, run `axon serve`. It serves
the first-party `/v1` REST routes and MCP-over-HTTP on `/mcp`, owning SQLite job
state, output files, screenshots, and artifacts under its `AXON_DATA_DIR`
(default `~/.axon`), behind the `AXON_HTTP_TOKEN` bearer policy. Point your
own HTTP/MCP clients at it; the bundled CLI does not consume those routes.

## ~/.axon/config.toml

`~/.axon/config.toml` holds tuning knobs — parameters that are safe to commit to source control because they contain no secrets or security toggles. Copy `config.example.toml` from the repo root and place it at `~/.axon/config.toml` (create `~/.axon/` with `chmod 700` and the file with `chmod 600`).

```bash
mkdir -m 700 ~/.axon
cp config.example.toml ~/.axon/config.toml
chmod 600 ~/.axon/config.toml
```

To point at a custom path: `AXON_CONFIG_PATH=/path/to/config.toml`.

**`config.example.toml` at the repo root is the single source of truth for
every TOML key** — each entry is documented inline with its env override, its
clamp range, and its real default. Do not hand-maintain a second copy of that
table here; read the file directly, or use the generated registries:

- `docs/reference/config/config-toml.md` / `config.schema.json` — generated
  TOML key registry
- `docs/reference/config/env.md` / `env.schema.json` — generated env var
  registry

Unknown top-level sections and several individually-renamed/removed keys fail
parsing with a message naming the current replacement (see the migration
table above); this is deliberate — a config file for the wrong shape should
error, not silently no-op.

The parser (`RawTomlConfig` in `crates/axon-core/src/config/parse/toml_config/raw.rs`)
accepts exactly these top-level sections:

`server`, `sources`, `pipeline`, `jobs`, `providers`, `retrieval`, `ask`,
`crawl`, `watch`, `memory`, `graph`, `artifacts`, `prune`, `observability`,
`security`

The env var shown next to each `config.example.toml` key still overrides the
TOML value at the precedence chain above.

URLs, API keys, secrets, and LLM runtime/bootstrap controls belong in `~/.axon/.env` — not in `config.toml`. Non-secret model names and tuning knobs belong in `config.toml`. Gemini headless is the default LLM synthesis path; set `AXON_LLM_BACKEND=openai-compat` with `AXON_OPENAI_BASE_URL` for llama.cpp/OpenAI-compatible endpoints, or `AXON_LLM_BACKEND=codex-app-server` to spawn Codex CLI app-server completions over stdio.

`[server] allow-fallback-web-assets = true` is the preferred local-development
way to let the Rust build embed the placeholder web panel when `apps/web/out`
has not been built. The `AXON_ALLOW_FALLBACK_WEB_ASSETS` env var remains a
compatibility override for CI or one-off commands, but normal developer
machines should keep the value in `~/.axon/config.toml`.

> **Replaced by:** `axon.json` was removed in v0.36. Migrate tuning params to `~/.axon/config.toml`.

## Environment variables by category

### Core runtime (required)

| Variable | Default | Description |
|----------|---------|-------------|
| `QDRANT_URL` | -- | Qdrant vector database URL |
| `TEI_URL` | -- | Text Embeddings Inference URL |

### Host paths

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_DATA_DIR` | `~/.axon` | Root directory for all persistent data (flat — no `axon/` subdir nesting) |

### ArtifactCandidate delivery (optional, fail-fast pair)

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_ARTIFACT_CANDIDATE_DEPOT_URL` | unset | Trusted Depot HTTP(S) base URL for committed candidate intake. HTTP is accepted only for loopback. |
| `AXON_ARTIFACT_CANDIDATE_DEPOT_TOKEN` | unset | Depot bearer token with `skills:write` scope. |

Leave both variables absent to use the disabled no-op sink. Setting either
variable opts into Depot delivery: both values must then be present, non-empty,
free of surrounding whitespace, and valid. Partial or invalid explicit
configuration aborts worker/server runtime construction so staged delivery is
not accidentally treated as disabled.

### Server ports

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_HTTP_PUBLISH` | `8001` | Docker Compose host publish address for the `axon` MCP HTTP service. The default `8001` maps to `0.0.0.0:8001` inside Compose — the container is reachable on the host's port 8001 from all interfaces. Set to `127.0.0.1:8001` to restrict to loopback only. |
| `AXON_HTTP_HOST` | `127.0.0.1` | HTTP bind address for `axon serve` / MCP HTTP. Non-loopback requires bearer or OAuth auth. |
| `AXON_HTTP_PORT` | `8001` | HTTP listen port for `axon serve` / MCP HTTP. |

### SQLite job runtime

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_SQLITE_PATH` | `$AXON_DATA_DIR/jobs.db` (default `~/.axon/jobs.db`) | SQLite jobs database path. Env-only; no CLI flag. |

**Worker spawn is conditional**, not unconditional. The SQLite backend has two construction modes:

- `SqliteJobBackend::new(cfg)` — **enqueue-only**. No workers spawn. Used by `ServiceContext::new()` for short-lived CLI commands (status/list/cancel/fire-and-forget submit).
- `SqliteJobBackend::new_with_workers(cfg)` — spawns in-process tokio workers (crawl + N×embed + extract + N×ingest). Used by `ServiceContext::new_with_workers()` for long-running processes: `axon serve`, MCP server, web routes, and CLI commands that block on `--wait true`.

Spawning workers in a fire-and-forget CLI process orphans claimed jobs at process exit, so the CLI defaults to enqueue-only and lets a separate `serve`/`mcp` process drain the queue.

`--wait false` is intentionally fire-and-forget for crawl/embed/ingest submits: the command enqueues the job, prints the job ID, and exits without draining the table. `--wait true` starts in-process workers where the service path needs queued workers, then waits only for the job IDs submitted by the current command and any explicit dependent job IDs.

### Local code search

Code search now routes through the unified query/source surface. Use
`axon query <text> --content-kind code` with source/path filters for committed
code search. Local code index data is stored in the shared SQLite database plus
Qdrant vectors with source metadata for the indexed checkout.

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_CODE_SEARCH_ALLOWED_ROOTS` | -- | Colon- or comma-separated filesystem roots allowed for MCP `code_search` `cwd` resolution. `/` and `HOME` are rejected. |
| `sources.code-search.freshness-ttl-secs` / `AXON_CODE_SEARCH_FRESHNESS_TTL_SECS` | `30` | Process-local freshness cache TTL before a new manifest check is required. |
| `sources.code-search.reindex-timeout-secs` / `AXON_CODE_SEARCH_REINDEX_TIMEOUT_SECS` | `300` | Foreground refresh timeout. On timeout, stale vectors are returned with a freshness warning; no background refresh continues in v1. |
| `sources.code-search.max-file-bytes` / `AXON_CODE_SEARCH_MAX_FILE_BYTES` | `10485760` | Max local source file size considered by the manifest and embed pass. Larger files are skipped. |
| `sources.code-search.changed-file-batch-size` / `AXON_CODE_SEARCH_CHANGED_FILE_BATCH_SIZE` | `64` | Changed-file batch size for local-code embedding. |

### TEI embedding

Axon client-side TEI retry/batching knobs (`[providers.embedding]`) and
markdown chunking knobs (`[pipeline.chunking]`) are documented with their env
overrides and real defaults in `config.example.toml` — see that file rather
than a second copy of the table here. The corresponding env vars remain
accepted as compatibility overrides, but should not live in `~/.axon/.env`
for normal operation.

TEI container runtime and Compose interpolation values stay in `~/.axon/.env`:

| Variable | Default | Description |
|----------|---------|-------------|
| `TEI_HTTP_PORT` | `52000` | Host port for TEI container |
| `TEI_EMBEDDING_MODEL` | `Qwen/Qwen3-Embedding-0.6B` | HuggingFace embedding model |
| `TEI_MAX_CONCURRENT_REQUESTS` | `512` | Max concurrent TEI server requests |
| `TEI_MAX_BATCH_TOKENS` | `196608` | Max TEI server batch tokens for Qwen3-Embedding-0.6B on the RTX 4070 profile; `245760` OOM'd during warmup in local testing |
| `TEI_MAX_BATCH_REQUESTS` | `512` | Max TEI server batch requests; keeps concurrent docs batches from tripping overload at the old 256-input boundary |
| `TEI_SERVER_MAX_CLIENT_BATCH_SIZE` | `256` | Max TEI server client batch size. Distinct from Axon's `providers.embedding.batch-size` client tuning knob. |
| `TEI_POOLING` | `last-token` | Pooling strategy |
| `TEI_TOKENIZATION_WORKERS` | `20` | Tokenization workers |
| `HF_TOKEN` | -- | HuggingFace token for gated models |

### LLM runtime

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_LLM_BACKEND` | `gemini-headless` | Completion backend. Supported: `gemini-headless`, `openai-compat`, `codex-app-server`. |
| `AXON_OPENAI_BASE_URL` | -- | OpenAI-compatible API root, for example `http://127.0.0.1:8080/v1`. Do not include `/chat/completions`; Axon appends it. |
| `providers.llm.synthesis-openai-model` / `AXON_SYNTHESIS_OPENAI_MODEL` | -- | Synthesis model for the OpenAI-compatible endpoint (ask/evaluate/suggest/extract/research). Required when the backend is `openai-compat`. |
| `llm.chat-openai-model` / `AXON_CHAT_OPENAI_MODEL` | -- | Direct-chat model override. Empty = use the synthesis model. |
| `AXON_OPENAI_API_KEY` | -- | Optional bearer token for OpenAI-compatible endpoints. Leave unset for local llama.cpp servers that do not require auth. |
| `llm.synthesis-gemini-model` / `AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL` | -- | Gemini synthesis model override (ask/evaluate/suggest/extract/research). Legacy env alias: `AXON_HEADLESS_GEMINI_MODEL`. |
| `llm.chat-gemini-model` / `AXON_CHAT_HEADLESS_GEMINI_MODEL` | -- | Direct-chat Gemini model override. Empty = use the synthesis model. |
| `AXON_HEADLESS_GEMINI_CMD` | `gemini` | Gemini CLI command for headless synthesis. Path-like values are validated before launch. |
| `AXON_HEADLESS_GEMINI_HOME` | `HOME` | Source HOME to copy Gemini CLI auth files from before running with isolated temporary HOME. |
| `AXON_CODEX_CMD` | `codex` | Host-only Codex CLI command shared by the Codex synthesis backend and Palette control. Do not put host paths in the shared compose `.env`; production compose clears this variable inside the container. Explicit paths must be executable and non-symlinked; when `AXON_CODEX_CONTROL_ENABLED=true`, the value must be an absolute, service-owned path. |
| `AXON_CODEX_HOME` | -- | Host-only source Codex home used for auth isolation. The backend creates a throwaway runtime home and does not load user hooks, MCP servers, apps, or skills. Do not put host paths in the shared compose `.env`; production compose clears this variable inside the container. |
| `AXON_SYNTHESIS_CODEX_MODEL` | -- | Optional synthesis model for Codex app-server. If unset, Codex uses its configured default. Legacy alias: `AXON_CODEX_MODEL`. |
| `AXON_CODEX_COMPLETION_CONCURRENCY` | `1` | Max concurrent Codex app-server completions. Defaults lower than HTTP backends because this backend spawns a child app-server per completion. |
| `AXON_LLM_COMPLETION_CONCURRENCY` | `4` | Runtime-only max concurrent LLM completion requests. |
| `AXON_LLM_COMPLETION_TIMEOUT_SECS` | `300` | Runtime-only timeout for each LLM completion request. |

LLM completion concurrency is enforced per backend/limit bucket so a first
request cannot pin a different backend or later limit until process restart.
OpenAI-compatible upstream error bodies are size-bounded and redacted before
they are returned to callers or logs.

### Collections and worker lanes

Vector collection name, Qdrant write/index tuning, worker concurrency, Chrome
render tuning, and job watchdog timing are all normal
`~/.axon/config.toml` settings under `[server]`, `[providers.vector]`,
`[pipeline]`, `[providers.render]`, and `[jobs]` — see `config.example.toml`
for the full, current key list with env overrides and defaults. Env vars
remain accepted for temporary overrides and legacy scripts.

### Adaptive crawl concurrency

`[crawl.adaptive-concurrency]` is TOML-only in this release and is disabled by default. When enabled, Axon replaces the fixed Spider crawl semaphore with Spider's adaptive semaphore on the main crawl path only. Post-crawl sitemap backfill, standalone `axon screenshot`, and non-Spider fetch helpers continue to use their existing fixed limits.

HTTP `429`, HTTP `5xx`, and crawl broadcast lag reduce concurrency. HTTP `2xx` responses are the only successes: they increase the target after Spider's fixed success threshold. Other `3xx`/`4xx` responses are neutral because Axon skips them as page errors without treating them as crawler pressure. Spider 2.52.0 uses a fixed failure decrease of `0.5`; `decrease-factor`, `sync-interval-ms`, and palette editing are intentionally unsupported here.

Shrinks lower the controller target immediately, but they do not cancel already in-flight fetches. Spider 2.52.0 does not claw back permits already held by in-flight requests, so active requests may temporarily exceed the lower target while they finish. Axon drains that returned surplus on later pressure events.

Pair adaptive mode with polite bounds: `respect-robots`, `delay-ms`, `max-pages`, path budgets, or `url-whitelist`. Axon logs warnings when adaptive mode is combined with uncapped or impolite settings.

`providers.render.remote-local-policy` applies only to Spider-backed Chrome render paths during crawls, including the post-crawl Chrome thin-page refetch path. When this policy is enabled, Axon's raw inline CDP thin-page optimization is skipped so Chrome refetches flow through Spider interception. It is intended for capable remote Chrome engines that support Spider/Chromey's policy push; generic CDP proxies may reject the underlying command. It does not apply to `axon screenshot` in this release.

### Search and research

| Variable | Default | Description |
|----------|---------|-------------|
| `TAVILY_API_KEY` | -- | Tavily AI Search API key (fallback when `AXON_SEARXNG_URL` is unset) |
| `AXON_SEARXNG_URL` | -- | Base URL of a self-hosted SearXNG instance (e.g. `https://searx.example.com`). When set, `search` and `research` use SearXNG's JSON API instead of Tavily. SearXNG must have the `json` output format enabled in `settings.yml`. |
| `AXON_RESEARCH_FULL_CONTENT` | `true` | When true, `research` fetches each top source's full page and synthesizes over it; set `false`/`0`/`no`/`off` to synthesize over search snippets only (faster). |

### Ingest credentials

| Variable | Default | Description |
|----------|---------|-------------|
| `GITHUB_TOKEN` | -- | GitHub PAT for private repos and rate limits |
| `GITLAB_TOKEN` | -- | GitLab personal, project, or group access token for private projects and rate limits |
| `GITEA_TOKEN` | -- | Gitea/Forgejo access token for private repos and rate limits |
| `REDDIT_CLIENT_ID` | -- | Reddit OAuth2 client ID |
| `REDDIT_CLIENT_SECRET` | -- | Reddit OAuth2 client secret |

### Chrome browser

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_CHROME_REMOTE_URL` | `http://axon-chrome:6000` | CDP management endpoint |
| ~~`CHROME_URL`~~ | — | **Stale alias — do not set.** Superseded by `AXON_CHROME_REMOTE_URL`. Deleted by `axon config migrate`. Spider reads this as `CHROM_BASE` but axon's `runtime.rs` pins the connection via `with_chrome_connection` so the spider fallback never fires. |
| `AXON_CHROME_PROXY` | -- | Proxy URL for Chrome requests |
| `AXON_CHROME_USER_AGENT` | -- | User-Agent override for Chrome requests |
| `AXON_CHROME_DIAGNOSTICS` | `false` | Enable browser diagnostics artifact collection |
| `AXON_CHROME_DIAGNOSTICS_DIR` | `$AXON_DATA_DIR/chrome-diagnostics` (default `~/.axon/chrome-diagnostics`) | Output directory for diagnostics artifacts |
| `AXON_CHROME_DIAGNOSTICS_EVENTS` | `false` | Include event-log capture in diagnostics |
| `AXON_CHROME_DIAGNOSTICS_SCREENSHOT` | `false` | Include screenshot capture in diagnostics |

### Hybrid search

Hybrid search tuning lives under `[providers.vector]` (`hybrid-enabled`,
`hnsw-ef`) and `[retrieval]` (`hybrid-candidates`, `ask-hybrid-candidates`) in
`~/.axon/config.toml` — see `config.example.toml` for the full entries with
env overrides and defaults.

### Ask / RAG tuning

Core retrieval selection knobs live in `~/.axon/config.toml` under `[ask]`.

| TOML key | Env override | Default | Description |
|----------|--------------|---------|-------------|
| `ask.max-context-chars` | `AXON_ASK_MAX_CONTEXT_CHARS` | Model-tiered | Max context characters passed to the LLM (clamped 20000-1000000). When unset, model-tier fallbacks apply: 1,000,000 large, 400,000 GPT/Codex, 128,000 local Gemma, 40,000 unknown |
| `ask.candidate-limit` | `AXON_ASK_CANDIDATE_LIMIT` | Model-tiered | Max retrieval candidates per prefetch (clamped 8-300). When unset, model-tier fallbacks apply: 250 large, 150 GPT/Codex, 120 local Gemma, 60 unknown |
| `ask.chunk-limit` | `AXON_ASK_CHUNK_LIMIT` | Model-tiered | Max total chunks selected for LLM context (clamped 3-64). When unset, model-tier fallbacks apply: 50 large, 28 GPT/Codex, 20 local Gemma, 10 unknown |
| `ask.full-docs` | `AXON_ASK_FULL_DOCS` | Adaptive | Explicit max full documents included in context (clamped 1-20). When unset, `ask` resolves 4 for simple queries and 6 for complex queries; high-context Gemini/Claude/GPT/Codex-family models use at least 4 |
| `ask.backfill-chunks` | `AXON_ASK_BACKFILL_CHUNKS` | `5` | Backfill chunks from top documents to pad context (clamped 0-20) |
| `ask.doc-fetch-concurrency` | `AXON_ASK_DOC_FETCH_CONCURRENCY` | `4` | Concurrent document fetches during context build (clamped 1-16) |
| `ask.doc-chunk-limit` | `AXON_ASK_DOC_CHUNK_LIMIT` | `96` | Max chunks per document in context (clamped 8-2000) |
| `ask.min-relevance-score` | `AXON_ASK_MIN_RELEVANCE_SCORE` | `0.45` | Minimum relevance score for candidate inclusion |
| `ask.authoritative-domains` | `AXON_ASK_AUTHORITATIVE_DOMAINS` | `[]` | Authoritative domains to boost in reranking |
| `ask.authoritative-boost` | `AXON_ASK_AUTHORITATIVE_BOOST` | `0.0` | Boost weight for authoritative domains in reranking (clamped 0.0-0.5) |
| `ask.min-citations-nontrivial` | `AXON_ASK_MIN_CITATIONS_NONTRIVIAL` | `2` | Min unique citations for non-trivial answers (clamped 1-5) |

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_ASK_*` | see table above | Env overrides remain supported for one-off runs and deployments |

### Web panel

The setup/config panel is served by `axon serve` and uses a file-backed panel
password under `~/.axon/panel-password`. MCP and protected `/v1` routes use
`AXON_HTTP_TOKEN` or OAuth; see the MCP auth section above.

### Logging

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Rust tracing filter |
| `AXON_LOG_PATH` | `$AXON_DATA_DIR/logs/axon.log` (default `~/.axon/logs/axon.log`) | Full path to the active log file. Rotated archives (`<file>.1`, `<file>.2`, …) live in the same directory. |
| `AXON_LOG_MAX_BYTES` | `10485760` | Size threshold (bytes) that triggers rotation. `0` disables rotation. Env-only — log rotation initialises before `config.toml` is parsed. |
| `AXON_LOG_MAX_FILES` | `3` | Number of rotated archives to retain. `0` truncates without keeping any archive. |
| `AXON_LOG_FULL_QUERIES` | -- | Log full query text instead of a redacted preview (verbose; default off) |

### MCP server

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_HTTP_HOST` | `127.0.0.1` | HTTP bind address; non-loopback requires bearer or OAuth auth |
| `AXON_HTTP_PORT` | `8001` | HTTP listen port |
| `AXON_MCP_TRANSPORT` | per-command | Transport override: `stdio`, `http`, or `both`. Unrecognized values fall back to the command default. Also `--transport`. |
| `AXON_HTTP_TOKEN` | -- | Bearer or `x-api-key` token; generated by `axon setup init` for local bearer mode |
| `AXON_AUTH_MODE` | `bearer` | Set to `oauth` to enable Google OAuth + DCR through lab-auth. |
| `AXON_PUBLIC_URL` | -- | Public origin used for OAuth metadata, e.g. `https://axon.example.com`. |
| `AXON_GOOGLE_CLIENT_ID` | -- | Google OAuth client ID for MCP OAuth mode. |
| `AXON_GOOGLE_CLIENT_SECRET` | -- | Google OAuth client secret for MCP OAuth mode. |
| `AXON_AUTH_ADMIN_EMAIL` | -- | Admin email accepted by OAuth mode; this account receives full Axon OAuth scopes. |
| `AXON_ALLOWED_REDIRECT_URIS` | Claude callback included | Additional comma-separated OAuth redirect URI allowlist. |
| `AXON_ALLOWED_ORIGINS` | -- | Comma-separated allowed origins for MCP HTTP CORS |
| `AXON_MCP_ARTIFACT_DIR` | `$AXON_DATA_DIR/artifacts` (default `~/.axon/artifacts`) | Directory for response artifacts |
| `AXON_INLINE_BYTES_THRESHOLD` | `8192` | Payload size below which auto-inline triggers (0 = disable) |
| `AXON_TASK_RESULT_WAIT_TIMEOUT_SECS` | `300` | Max seconds an MCP `tasks/result` request waits for a task to reach a terminal state |
| `AXON_SOURCE_LOCAL_ALLOWED_ROOTS` | -- | Comma-separated local filesystem roots allowed for source requests from server transports (unset = local source submission disabled) |
| `AXON_MCP_EMBED_MAX_LOCAL_BYTES` | `10485760` | Max bytes per local file embedding request via MCP |
| `AXON_MCP_EMBED_MAX_LOCAL_DEPTH` | `16` | Max directory traversal depth for local directory embedding requests |
| `AXON_MCP_EMBED_MAX_LOCAL_ENTRIES` | `10000` | Max filesystem entries visited for local directory embedding requests |

The MCP and REST embed routes use the same server-side validator. URL and raw
text inputs are accepted, but host-local file and directory inputs must resolve
under `AXON_SOURCE_LOCAL_ALLOWED_ROOTS` and satisfy the byte/depth/entry limits.
Missing path-like inputs such as `/data/missing.md` or `./missing.md` are
rejected instead of being silently treated as raw text.

Source creation uses the same fail-closed rule for authenticated server
callers, both before a detached job is written and when a worker executes or
recovers it. Configure absolute, existing roots only. Requested roots and their
components must not be symlinks. On Linux, Axon opens discovered documents
relative to a held root descriptor with `openat2` containment (`BENEATH`,
`NO_SYMLINKS`, `NO_MAGICLINKS`, and `NO_XDEV`).
Because local acquisition selects the no-symlink containment flow,
`follow_symlinks=true` is rejected for every local caller, including trusted
CLI use; copy or bind the intended real directory into the fixed root instead.

For production indexing, expose a dedicated projection directory as a
read-only mount at a fixed container path and allow only that path. Do not
allow a broad data mount, a host workspace, `/proc`, or a mutable symlink:

```bash
AXON_SOURCE_LOCAL_ALLOWED_ROOTS=/srv/young-office-index/current
```

Promote `current` with an atomic directory rename while no indexing job owns
the mount. Nested mounts and bind-mount crossings beneath the root are rejected.
The configured allowed-root path and its ancestors are an operator trust
boundary: they must be root-owned and not writable by the Axon service account.
Axon acquires the allowed-root descriptor at source execution, resolves the
requested source beneath it, and retains the resolved descriptor through
discovery and acquisition; it does not retain a descriptor from config-load
time.

### Ask cache

The `[ask.cache]` section in `~/.axon/config.toml` controls the optional
process-local full-document cache used by ask retrieval. It is disabled by
default and only useful for long-lived `axon serve` / `axon mcp` processes.
`max-capacity-bytes` limits the summed `chunk_text` bytes retained in memory;
`ttl-secs` is capped at 300 seconds as a security backstop. When enabled in
`serve` or `mcp`, startup enforces `RLIMIT_CORE=0` to avoid core files
containing cached source text.

### Output and CLI

All commands accept `--color auto|always|never` and
`--motion auto|always|never`. Motion defaults to animated progress only when
stderr is an interactive terminal outside CI; `--motion never` keeps the same
status surface static. Normal output shows results and actionable warnings.
Use `-v` for operational detail, `-vv` for timestamped debug diagnostics, and
`--quiet` to retain only errors plus the command's stdout result.

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_OUTPUT_DIR` | `$AXON_DATA_DIR/output` (default `~/.axon/output`) | Output directory for file-writing commands |
| `NO_COLOR` | -- | Disable ANSI color output (standard `NO_COLOR`; any value). `FORCE_COLOR`/`CLICOLOR_FORCE` force it on |
| `AXON_NO_WIPE` | -- | Prevent destructive cache wipes |
| `AXON_DOMAINS_DETAILED` | -- | Enable detailed per-domain breakdown in `axon domains` |
| `AXON_SOURCES_FACET_LIMIT` | `100000` | Facet limit for `axon sources` |
| `AXON_SOURCES_DOMAIN_LIMIT` | `10000` | Max URLs fetched for explicit `axon sources --domain <host> --all` exports |
| `AXON_DOMAINS_FACET_LIMIT` | `100000` | Facet limit for `axon domains` |
| `AXON_SESSION_INGEST_MAX_BYTES` | -- | Max bytes per session ingest payload |

### Miscellaneous

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_TEST_QDRANT_URL` | `http://127.0.0.1:53335` | Host-accessible Qdrant URL for integration tests (backfilled by `dev-setup.sh`) |
| `QDRANT_COLLECTION` | -- | Legacy alias for `AXON_COLLECTION` |
| `AXON_EXCLUDE_PATH_PREFIX` | -- | Env form of `--exclude-path-prefix`; comma-separated path prefixes to skip during crawl (`none` disables the default locale list) |
| `AXON_SUGGEST_BASE_URL_LIMIT` | `250` | Max base URLs scanned by `axon suggest` (clamped 10–5000) |
| `AXON_SUGGEST_EXISTING_URL_LIMIT` | `500` | Max already-indexed URLs `axon suggest` considers (clamped 0–5000) |
| `AXON_ENDPOINT_BUNDLE_CONCURRENCY` | `8` | Concurrent JS-bundle fetches during `endpoints` discovery |
| `AXON_ENDPOINT_CHROME_CONCURRENCY` | `1` | Concurrent Chrome probes during `endpoints` discovery (Chrome is scarce) |
| `AXON_ENDPOINT_VERIFY_CONCURRENCY` | `16` | Concurrent endpoint verification requests |
| `AXON_CODEX_CMD` | -- | Host-only path to the Codex CLI binary for the codex LLM backend (non-symlink executable) |
| `AXON_CODEX_HOME` | -- | Host-only source HOME dir holding Codex auth files (non-symlink directory) |

### Webclaw port (axon_rust-zehr)

Per-site vertical extractors, DOM retry ladder, antibot detection, structured-data payload tuning.

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_ENABLE_VERTICALS` | `true` | Enable per-site vertical extractors (GitHub, PyPI, Reddit, HN, etc.). TOML: `crawl.verticals.enabled`. |
| `AXON_AUTO_DISPATCH_SKIP` | (empty) | Comma-separated extractor names to SKIP in auto-dispatch (still available via `--vertical`). TOML: `crawl.verticals.auto-dispatch-skip`. |
| `AXON_VERTICAL_CACHE_TTL_<NAME>` | github=86400, reddit=3600, hn=21600 | Per-vertical cache TTL in seconds. e.g. `AXON_VERTICAL_CACHE_TTL_GITHUB=43200`. TOML: `[crawl.verticals.cache-ttl-secs]`. |
| `AXON_STRUCTURED_DATA_MAX_BYTES` | `65536` | Max bytes per chunk in Qdrant `structured_blob` field. Clamped 1024–16777216. TOML: `providers.vector.structured-data-max-bytes`. |
| `AXON_LADDER_STRATEGY1_THRESHOLD` | `30` | DOM retry ladder Strategy 1 word threshold. Clamped 1–1000. TOML: `crawl.ladder-strategy1-threshold`. |
| `AXON_LADDER_STRATEGY2_THRESHOLD` | `200` | DOM retry ladder Strategy 2 word threshold. Clamped 1–10000. TOML: `crawl.ladder-strategy2-threshold`. |
| `AXON_LADDER_BODY_MULTIPLIER` | `2.0` | Body-fallback wins only if it produces N× scored words. Clamped 1.0–10.0. TOML: `crawl.ladder-body-multiplier`. |
| `AXON_CHALLENGE_WARMUP` | `true` | Enable Akamai/CF cookie warmup retry on antibot challenge. TOML: `crawl.antibot.cookie-warmup`. |
| `AXON_ANTIBOT_MAX_BODY_SCAN_BYTES` | `150000` | Max bytes scanned for antibot challenge patterns. Clamped 1000–10485760. TOML: `crawl.antibot.max-body-scan-bytes`. |

## Dev vs container URL resolution

The CLI auto-detects its runtime environment:

- **Inside Docker** (`/.dockerenv` exists): uses container DNS for Qdrant/TEI
- **Outside Docker** (local dev): rewrites to localhost with mapped ports

This means `.env` can use container DNS names -- `normalize_local_service_url()` in `config.rs` handles translation transparently.

## Keeping this file in sync

`docs/guides/configuration.md` is the single source of truth for env var documentation. When adding a new env variable:

1. Add it here in the appropriate section.
2. Add it to `.env.example` with a sensible default or blank value and a `[OPTIONAL]`/`[REQUIRED]` comment.
3. If it is MCP-server-specific, also add it to `docs/reference/mcp/env.md`.
4. Do not add full env tables to `README.md` — keep that to a short essentials list with a link here.

To spot drift between `.env.example` and this file, extract keys from both and diff:

```bash
# Keys in .env.example (non-comment, non-blank)
grep -v '^\s*#' .env.example | grep '=' | cut -d= -f1 | sort > /tmp/example_keys.txt

# Keys in CONFIG.md table rows (backtick-wrapped identifiers)
grep -oP '`[A-Z][A-Z0-9_]+`' docs/guides/configuration.md | tr -d '`' | sort -u > /tmp/config_keys.txt

# Vars in .env.example but missing from CONFIG.md
comm -23 /tmp/example_keys.txt /tmp/config_keys.txt

# Vars in CONFIG.md but missing from .env.example
comm -13 /tmp/example_keys.txt /tmp/config_keys.txt
```
