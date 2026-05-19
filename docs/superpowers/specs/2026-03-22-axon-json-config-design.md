# axon.json Config File — Design Spec
Date: 2026-03-22 (revised)

## Problem

The axon project has grown to ~145 environment variables spread across `.env`,
`apps/web/.env.local`, and ~30 more read by code but never documented in either file
(e.g. `AXON_DOMAINS_FACET_LIMIT`, `AXON_MAX_WS_CONNECTIONS`, `AXON_PG_POOL_SIZE`,
`AXON_PULSE_CHAT_TIMEOUT_MS`, `GOOGLE_OAUTH_*`, `AI_GATEWAY_API_KEY`, and others).

The `.env` file mixes secrets, infrastructure credentials, and hundreds of lines of operational
tuning knobs. This makes it hard to know what is actually required and buries important secrets
under noise. Many vars kept in `.env` aren't secrets at all — they're service URLs, model names,
and tuning knobs that have no business being credentials-adjacent.

---

## Goals

- `.env` contains **only** values that truly cannot live elsewhere: embedded-credential DSNs,
  bare secrets/tokens, Docker Compose interpolation vars, host bind-mount paths,
  `NEXT_PUBLIC_*` build-time vars (~43 vars total, many optional)
- All non-secret configuration — service URLs, model names, tuning knobs, feature flags,
  operational settings — lives in `axon.json` (~85 vars)
- `apps/web/.env.local` is eliminated — root `.env` is the single env file for both
  Rust workers and Next.js
- Precedence: `CLI flag > env var > axon.json > hardcoded default` (non-breaking for all
  existing env var overrides)
- `axon.json` is committed to the repo with all defaults set explicitly
- Previously undocumented vars are captured: tuning knobs → `axon.json`, secrets → `.env.example`

---

## Format: JSON (not TOML)

Both `serde_json` (Rust) and `JSON.parse` (Next.js) read it natively with no additional
dependencies. Unknown keys are **silently ignored** (no `deny_unknown_fields`) for
forward-compatibility. Descriptions live in `.env.example` and `CLAUDE.md`, not inline.

### JSON Schema (`axon.schema.json`)

`axon.schema.json` is committed alongside `axon.json` at the repo root. It provides
editor validation and autocomplete (VS Code, JetBrains, Neovim LSP) via the `$schema`
pointer in `axon.json`:

```json
{ "$schema": "./axon.schema.json", ... }
```

The schema uses `additionalProperties: true` everywhere — matching `no deny_unknown_fields`
— so forward-compatible fields added before the schema is updated are never rejected by
editors. It is **not** used at runtime; Rust validation is handled by serde.

## Security: `axon.json` must never contain secrets

`axon.json` is committed to the repo. It must only hold non-secret values. API keys,
passwords, tokens, and credentials must never appear in this file.

For per-machine values that differ between deployments (e.g. a custom `services.qdrant_url`
pointing to a non-default host), use env vars — they override `axon.json` via the precedence
chain. There is no `axon.local.json` escape hatch: env vars are the correct per-machine
override mechanism and already work exactly this way.

---

## What Stays in `.env` (explicit, justified list)

Every var here has a concrete reason it cannot move to `axon.json`.

### Docker Compose credential interpolation
Used as `${VAR}` in `docker-compose.services.yaml`. Docker Compose reads `.env` directly —
there is no mechanism for it to read `axon.json`.
```
POSTGRES_USER        # compose ${POSTGRES_USER}
POSTGRES_PASSWORD    # compose ${POSTGRES_PASSWORD}  — SECRET
POSTGRES_DB          # compose ${POSTGRES_DB}
REDIS_PASSWORD       # compose ${REDIS_PASSWORD}      — SECRET
RABBITMQ_USER        # compose ${RABBITMQ_USER}
RABBITMQ_PASS        # compose ${RABBITMQ_PASS}        — SECRET
```

### Connection strings with embedded credentials
Passwords are embedded in the DSN. Splitting into components + reconstructing is more complex
than the problem warrants.
```
AXON_PG_URL          # postgresql://user:PASSWORD@host/db   — SECRET
AXON_REDIS_URL       # redis://:PASSWORD@host:port          — SECRET
AXON_AMQP_URL        # amqp://user:PASSWORD@host/vhost      — SECRET
```

### Bare API secrets and tokens
```
OPENAI_API_KEY
TAVILY_API_KEY
GITHUB_TOKEN           # optional
REDDIT_CLIENT_ID       # OAuth app identifier (treat as private)
REDDIT_CLIENT_SECRET   # OAuth app secret
HF_TOKEN               # optional, for gated HuggingFace models
AI_GATEWAY_API_KEY     # optional
AXON_NEO4J_PASSWORD    # optional, only when Neo4j is configured
GOOGLE_OAUTH_CLIENT_ID
GOOGLE_OAUTH_CLIENT_SECRET
GOOGLE_OAUTH_DCR_TOKEN     # optional
GOOGLE_OAUTH_REDIS_URL     # contains Redis password
```

### Auth tokens (web/MCP surfaces)
```
AXON_WEB_API_TOKEN           # required when auth is enabled
AXON_MCP_API_KEY             # optional
AXON_WEB_BROWSER_API_TOKEN   # optional second-tier token
AXON_SHELL_WS_TOKEN          # optional dedicated shell WS token
```

### Host bind-mount paths
Per-machine values used for Docker volume mounts. Must be in `.env` for compose interpolation.
```
AXON_DATA_DIR      # host root for all persistent data
HOST_HOME          # host user home directory
AXON_WORKSPACE     # host workspace directory
HOST_WORKSPACE     # host axon_rust repo path
AXON_BIN           # path to pre-built axon binary (for axon-web container)
```

### Next.js build-time client vars
Baked into the browser bundle at `next build`. Next.js reads these from env at build time —
a JSON file loaded at runtime cannot retroactively change compiled client code.
```
NEXT_PUBLIC_AXON_API_TOKEN
NEXT_PUBLIC_AXON_WS_TOKEN
NEXT_PUBLIC_AXON_WS_URL
NEXT_PUBLIC_SHELL_WS_TOKEN
NEXT_PUBLIC_AXON_PORT
NEXT_PUBLIC_AXON_BROWSER_API_TOKEN
NEXT_PUBLIC_AXON_WEB_ALLOW_INSECURE_DEV
NEXT_PUBLIC_ENABLE_FAKE_AI_STREAM
```

### Test URL overrides (optional)
Per-machine overrides for integration tests. Optional — tests skip when unset.
```
AXON_TEST_PG_URL
AXON_TEST_AMQP_URL
AXON_TEST_REDIS_URL
AXON_TEST_QDRANT_URL
```

### Build artifact
```
AXON_GIT_SHA   # set by CI: export AXON_GIT_SHA=$(git rev-parse HEAD)
               # baked into Docker image labels; changes per commit so cannot
               # be a committed JSON value
```

### Config path escape hatch
```
AXON_CONFIG    # optional: absolute path to alternate axon.json
               # stays in .env so Docker containers can mount a custom config
```

**Total: ~43 vars. Many are optional (GITHUB_TOKEN, HF_TOKEN, GOOGLE_OAUTH_*, test URLs, etc.)
The truly required minimum for basic operation is ~15.**

---

## `axon.json` Structure (~85 settings)

Committed at the repo root. All defaults are set explicitly. A missing file or missing
JSON keys silently fall back to hardcoded defaults — the file is always optional.

```json
{
  "services": {
    "qdrant_url": "http://axon-qdrant:6333",
    "tei_url": "http://axon-tei:80",
    "chrome_remote_url": "http://axon-chrome:6000",
    "chrome_url": "http://axon-chrome:6000",
    "neo4j_url": "",
    "neo4j_user": "neo4j",
    "backend_url": "http://axon-workers:49000",
    "workers_ws_url": "",
    "backend_hostname": ""
  },
  "llm": {
    "base_url": "",
    "model": ""
  },
  "tei": {
    "max_retries": 5,
    "request_timeout_ms": 30000,
    "max_client_batch_size": 128,
    "http_port": 52000,
    "embedding_model": "Qwen/Qwen3-Embedding-0.6B",
    "max_concurrent_requests": 80,
    "max_batch_tokens": 163840,
    "max_batch_requests": 80,
    "pooling": "last-token",
    "tokenization_workers": 8
  },
  "search": {
    "hybrid_enabled": true,
    "hybrid_candidates": 100,
    "ask_hybrid_candidates": 150,
    "hnsw_ef": 128,
    "hnsw_ef_legacy": 64
  },
  "ask": {
    "max_context_chars": 120000,
    "candidate_limit": 64,
    "chunk_limit": 10,
    "full_docs": 4,
    "backfill_chunks": 3,
    "doc_fetch_concurrency": 4,
    "doc_chunk_limit": 192,
    "min_relevance_score": 0.45,
    "authoritative_domains": [],
    "authoritative_boost": 0.0,
    "authoritative_allowlist": [],
    "min_citations_nontrivial": 2
  },
  "embed": {
    "collection": "axon",
    "doc_concurrency": null,
    "doc_timeout_secs": 300,
    "strict_predelete": true
  },
  "queues": {
    "crawl": "axon.crawl.jobs",
    "extract": "axon.extract.jobs",
    "embed": "axon.embed.jobs",
    "ingest": "axon.ingest.jobs",
    "refresh": "axon.refresh.jobs",
    "graph": "axon.graph.jobs"
  },
  "workers": {
    "ingest_lanes": 2,
    "max_pending_crawl_jobs": 100,
    "crawl_size_warn_threshold": 10000,
    "job_stale_timeout_secs": 300,
    "job_stale_confirm_secs": 60,
    "pg_pool_size": null,
    "max_ws_connections": null,
    "max_shell_connections": null,
    "max_sync_concurrent": null
  },
  "graph": {
    "concurrency": 4,
    "llm_model": "qwen3.5:4b",
    "similarity_threshold": 0.75,
    "similarity_limit": 20,
    "context_max_chars": 2000,
    "taxonomy_path": ""
  },
  "acp": {
    "adapter_cmd": "",
    "adapter_args": "",
    "prewarm": true,
    "auto_approve": true,
    "max_concurrent_sessions": 8,
    "turn_timeout_ms": 300000,
    "allowed_claude_betas": "interleaved-thinking",
    "agents": {
      "claude": { "cmd": "", "args": "" },
      "codex":  { "cmd": "", "args": "" },
      "gemini": { "cmd": "", "args": "" }
    }
  },
  "web": {
    "allowed_origins": [],
    "allow_insecure_dev": false,
    "allow_query_token": false,
    "trust_proxy": false,
    "pulse_chat_timeout_ms": 300000,
    "shell_allowed_origins": [],
    "shell_server_host": "127.0.0.1",
    "shell_server_port": null,
    "docker_socket_path": "",
    "enable_docker_socket_logs": false
  },
  "mcp": {
    "transport": "http",
    "http_host": "0.0.0.0",
    "http_port": 8001,
    "artifact_dir": "",
    "inline_bytes_threshold": 8192
  },
  "serve": {
    "host": "",
    "port": 49000
  },
  "chrome": {
    "diagnostics": false,
    "diagnostics_dir": "",
    "diagnostics_events": false,
    "diagnostics_screenshot": false,
    "proxy": "",
    "user_agent": ""
  },
  "logging": {
    "file": "",
    "max_bytes": 10485760,
    "max_files": 3,
    "no_color": false
  },
  "output": {
    "dir": "",
    "extract_est_cost_per_1k_tokens": null
  },
  "ingest": {
    "github_max_issues": 100,
    "github_max_prs": 100,
    "download_max_bytes": null,
    "download_max_files": null,
    "domains_detailed": false,
    "domains_facet_limit": null
  },
  "oauth": {
    "auth_url": "",
    "token_url": "",
    "redirect_uri": "",
    "redirect_host": "",
    "redirect_path": "",
    "redirect_policy": "",
    "scopes": "",
    "required_scopes": "",
    "redis_prefix": "",
    "broker_issuer": ""
  }
}
```

---

## Rust Implementation

### New: `crates/core/config/axon_config.rs`

Serde struct tree mirroring the JSON shape. Every field is `Option<T>`. All structs implement
`Default` and carry `#[serde(default)]` so missing JSON sections deserialize to `Default`
rather than erroring.

```rust
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct AxonConfig {
    pub services: ServicesConfig,
    pub llm: LlmConfig,
    pub tei: TeiConfig,
    pub search: SearchConfig,
    pub ask: AskConfig,
    pub embed: EmbedConfig,
    pub queues: QueuesConfig,
    pub workers: WorkersConfig,
    pub graph: GraphConfig,
    pub acp: AcpConfig,
    pub web: WebConfig,
    pub mcp: McpConfig,
    pub serve: ServeConfig,
    pub chrome: ChromeConfig,
    pub logging: LoggingConfig,
    pub output: OutputConfig,
    pub ingest: IngestConfig,
    pub oauth: OAuthConfig,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ServicesConfig {
    pub qdrant_url: Option<String>,
    pub tei_url: Option<String>,
    pub chrome_remote_url: Option<String>,
    pub chrome_url: Option<String>,
    pub neo4j_url: Option<String>,
    pub neo4j_user: Option<String>,
    pub backend_url: Option<String>,
    pub workers_ws_url: Option<String>,
    pub backend_hostname: Option<String>,
}

// ... (all other sub-structs follow same pattern)
```

No `deny_unknown_fields` — forward-compatible with future fields.
Estimated size: ~250 lines of struct definitions + derives.

### New: `crates/core/config/parse/axon_config_loader.rs`

```rust
/// Load `axon.json` from the path in `AXON_CONFIG` env var, or `axon.json` in CWD.
/// - Missing file: silently returns `AxonConfig::default()`
/// - Parse error: logs WARNING to stderr (not silent), returns `AxonConfig::default()`
/// - `AXON_CONFIG` value: resolved as-is (absolute path recommended; relative = relative to CWD)
pub fn load_axon_config() -> AxonConfig {
    let path = std::env::var("AXON_CONFIG")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("axon.json"));

    match std::fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str::<AxonConfig>(&contents) {
            Ok(cfg) => cfg,
            Err(e) => {
                // File found but malformed = hard error. Silently continuing with defaults
                // would produce mysterious misconfiguration that is very hard to debug,
                // especially in long-running worker processes.
                eprintln!("ERROR: axon.json parse error ({}): {e}", path.display());
                std::process::exit(1);
            }
        },
        // File not found = silently use defaults. axon.json is optional.
        Err(_) => AxonConfig::default(),
    }
}
```

**CWD resolution:** Workers are always started from the repo root (`just dev`, `cargo run`,
or Docker `WORKDIR /workspace`). `AXON_CONFIG` handles any edge case where CWD differs.

**Ordering:** `load_axon_config()` is called at the top of `into_config()` before CLI arg
processing. It reads `AXON_CONFIG` from env directly — no conflict with CLI parsing.

### New helpers: `crates/core/config/parse/helpers.rs`

Four new `_or` variants accepting an `Option<T>` JSON default. Existing helpers kept —
call sites migrated incrementally:

```rust
pub fn env_usize_clamped_or(
    var: &str, json: Option<usize>, fallback: usize, min: usize, max: usize,
) -> usize {
    // env::var returns Err when unset, Ok("") when set to empty string.
    // Unset = fall through to json/fallback.
    // Set to "" = parse fails = fall through to json/fallback.
    // Set to a value = use it (even if it overrides json).
    std::env::var(var)
        .ok()
        .and_then(|v| v.parse().ok())  // parse failure (including "") → None
        .or(json)
        .unwrap_or(fallback)
        .clamp(min, max)
}

pub fn env_bool_or(var: &str, json: Option<bool>, fallback: bool) -> bool { ... }
pub fn env_f64_clamped_or(var: &str, json: Option<f64>, fallback: f64, min: f64, max: f64) -> f64 { ... }
// env_str_or and env_opt_str_or: env::var Err (unset) → fall through to json.
// env::var Ok("") (set to empty) → use "" (does NOT fall through).
// This is correct: a user who sets VAR= in .env explicitly wants an empty string.
pub fn env_str_or(var: &str, json: Option<String>, fallback: &str) -> String { ... }
pub fn env_opt_str_or(var: &str, json: Option<String>) -> Option<String> { ... }
```

**Env var semantics:** `env::var` returns `Err` when a variable is unset (→ fall through to
JSON). For typed fields (numbers, booleans), an empty string fails parsing and also falls
through. For string fields, `Ok("")` (explicitly set to empty) uses `""` — it does NOT fall
through to JSON. This is correct behavior: `AXON_ACP_ADAPTER_ARGS=` in `.env` means zero args,
not "use the JSON value."

### Modified: `crates/core/config/parse/build_config.rs`

`load_axon_config()` called once at top of `into_config()`. ~50 call sites migrated.
The `qdrant_url`, `tei_url`, `chrome_remote_url`, `openai_base_url`, `openai_model`,
`neo4j_url`, `neo4j_user` etc. now also read from `ac.services.*` / `ac.llm.*` as their
JSON-layer default before falling back to hardcoded values.

### Docker: `axon.json` in the image

`axon.json` is committed to the repo root. The `docker/Dockerfile` copies the full build
context so `axon.json` is present in the image at `/workspace/axon.json`. The binary's CWD
inside the container is `/workspace`. No additional `COPY` directive needed.

Verify `WORKDIR` in `docker/Dockerfile` is `/workspace` before shipping.

---

## Next.js Integration

### Single `.env` consolidation

`apps/web/.env.local` holds 8 vars that all exist in root `.env`. In Docker, compose already
injects root `.env` into the container — the duplication only exists for local `pnpm dev`.

**`apps/web/next.config.ts`** — load root `.env` before Next.js processes its own env:

```typescript
import { config } from "dotenv"
import { resolve } from "path"

// Local dev: load root .env so pnpm dev and next build find all vars.
// Docker: no-op — compose injects vars into the container env before Node starts.
// override: false ensures injected vars win over file values.
config({ path: resolve(__dirname, "../../.env"), override: false })
```

`dotenv` is a transitive Next.js dependency — no new install required.

`NEXT_PUBLIC_*` vars in root `.env` are loaded by this call before `next build` processes them.
Build-time baking works correctly.

### New: `apps/web/lib/axon-config.ts`

```typescript
import "server-only"  // prevents accidental client-side import
import { readFileSync } from "fs"
import { join } from "path"
import type { AxonConfig } from "./axon-config-types"

// Module-level singleton — loaded once on first import, not per-request.
function loadAxonConfig(): Partial<AxonConfig> {
  const configPath = process.env.AXON_CONFIG ?? join(process.cwd(), "axon.json")
  try {
    return JSON.parse(readFileSync(configPath, "utf8")) as Partial<AxonConfig>
  } catch {
    return {}
  }
}

export const axonConfig = loadAxonConfig()
```

Server callers use standard precedence:
```typescript
// Before:
const backendUrl = process.env.AXON_BACKEND_URL ?? "http://axon-workers:49000"
// After:
const backendUrl = process.env.AXON_BACKEND_URL
  ?? axonConfig.services?.backend_url
  ?? "http://axon-workers:49000"
```

### `apps/web/.env.local` — deleted

**Breaking change (the only one):** Any existing deployment with custom values in
`apps/web/.env.local` must migrate them to root `.env`.

Migration steps:
1. Copy any custom values from `apps/web/.env.local` to root `.env`
2. Delete `apps/web/.env.local`
3. Check `docker-compose.yaml` for any `env_file: apps/web/.env.local` — update to root `.env`

---

## Precedence Chain

```
1. CLI flag           --hybrid-candidates 200
        ↓ not set
2. Environment var    AXON_HYBRID_CANDIDATES=150  (Err/unset → fall through; parse failure → fall through)
        ↓ unset or unparseable
3. axon.json value    { "search": { "hybrid_candidates": 120 } }
        ↓ key absent or file missing
4. Hardcoded default  100
```

## Migration: zero-disruption rollout

**Existing deployments do not need to change anything.** Because the env var layer (step 2)
is evaluated before `axon.json` (step 3), any value currently set in `.env` continues to win.

Example: if a deployment has `QDRANT_URL=http://myserver:6333` in `.env` today, that value
will still be used after this change — it overrides the `axon.json` default of
`http://axon-qdrant:6333`. The operator can migrate at their own pace by removing `QDRANT_URL`
from `.env` and setting it in `axon.json` instead, or leave it in `.env` indefinitely.

**The only required action** for existing deployments is the `apps/web/.env.local` deletion
(see breaking change above). All other migrations are optional and gradual.

---

## Files Changed

| File | Change |
|------|--------|
| `axon.json` | **New** — committed config with all defaults (~85 settings, 17 sections) |
| `axon.schema.json` | **New** — JSON Schema for editor validation/autocomplete (not used at runtime) |
| `crates/core/config/axon_config.rs` | **New** — serde struct tree (~250 lines) |
| `crates/core/config/parse/axon_config_loader.rs` | **New** — load function |
| `crates/core/config/parse/helpers.rs` | **Modified** — add `_or` helper variants (empty string = not set) |
| `crates/core/config/parse/build_config.rs` | **Modified** — load AxonConfig, migrate ~50 call sites |
| `crates/core/config/parse.rs` | **Modified** — declare new module |
| `crates/core/config.rs` | **Modified** — re-export `AxonConfig` |
| `apps/web/next.config.ts` | **Modified** — dotenv root load |
| `apps/web/lib/axon-config.ts` | **New** — server-side singleton (`server-only` guard) |
| `apps/web/lib/axon-config-types.ts` | **New** — TypeScript types mirroring JSON shape |
| `apps/web/*.ts` callers | **Modified** — adopt axon-config for non-secret settings |
| `.env.example` | **Modified** — strip ~85 moved vars, keep ~43, add pointer to axon.json |
| `.env` | **Modified** — strip same vars |
| `apps/web/.env.local` | **Deleted** |
| `docker-compose.yaml` | **Checked** — remove any `env_file: apps/web/.env.local` |
| `docker/Dockerfile` | **Verified** — confirm `WORKDIR /workspace` so `axon.json` is found |
| `CLAUDE.md` | **Updated** — document axon.json approach |
| `crates/core/CLAUDE.md` | **Updated** — document AxonConfig and load order |
| `docs/DEPLOYMENT.md` | **Updated** — migration instructions |

---

## Testing Plan

### Unit tests (no services required)

1. **Missing file** — `AXON_CONFIG=/nonexistent`, verify silent `AxonConfig::default()`
2. **Malformed JSON** — point at `{ bad json`, verify stderr WARNING + `AxonConfig::default()`
3. **Partial JSON** — only `{ "search": { "hybrid_candidates": 77 } }`, verify
   `ac.search.hybrid_candidates == Some(77)`, all other fields `None`
4. **`AXON_CONFIG` override** — set to custom path, verify correct file loaded
5. **Precedence — env wins**: set `AXON_HYBRID_CANDIDATES=200`, JSON=150 → expect 200
6. **Precedence — JSON wins**: unset env var, JSON=150 → expect 150
7. **Precedence — hardcoded wins**: unset env var, no JSON key → expect 100
8. **Empty string env var falls through**: `AXON_HYBRID_CANDIDATES=""`, JSON=150 → expect 150
9. **Service URL from JSON**: set `services.qdrant_url` in JSON, verify `Config.qdrant_url`
   picks it up when `QDRANT_URL` env var is unset

### Integration

10. **`cargo test`** — full suite passes unchanged
11. **`cargo clippy`** — zero new warnings

### Live tests (required before claiming completion)

12. **Worker config pickup**: start embed worker, set `embed.doc_timeout_secs: 42` in
    `axon.json`, verify value appears in startup log or `axon doctor` output
13. **Env override beats JSON**: set `AXON_EMBED_DOC_TIMEOUT_SECS=99`, verify it beats
    `axon.json` value of 42
14. **Service URL from JSON**: remove `QDRANT_URL` from env, set `axon.json services.qdrant_url`,
    verify `axon doctor` connects to Qdrant correctly
15. **Next.js local dev**: `pnpm dev` in `apps/web/` with no `.env.local`, verify server starts
    and backend connectivity works
16. **Docker**: `just up`, verify all containers start healthy, no WARNING in logs

---

## Gitignore

`axon.json` must NOT be in `.gitignore`. It contains no secrets and is committed to the repo.
Confirm before first commit.
