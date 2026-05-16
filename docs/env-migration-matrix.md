# Axon Env Migration Matrix

Generated: 2026-05-15 from source-derived inventory.

**This is the authoritative classification.** Docs/templates are drift outputs.

## Classification Key

| Code | Meaning |
|------|---------|
| `keep-env` | Secrets, endpoint URLs, auth/runtime state — stays in `.env` |
| `compose-env` | Compose interpolation keys; Compose cannot read from TOML |
| `move-toml` | Legitimate non-secret operator tuning — moves to `config.toml` |
| `delete` | Obsolete/stale/removed (Postgres/AMQP/Redis/ACP/legacy queues) |
| `trusted-bootstrap` | High-impact local path/config overrides; treated as trusted operator input |
| `compat-shim` | Legacy name; retained briefly with deprecation warning |
| `external/test` | Dev or test only; not allowed in production templates |
| `hard-default` | Internal tuning; not intended as a user-configurable knob |

## Runtime Placement

| Code | Meaning |
|------|---------|
| `both` | Needed on host and inside axon container |
| `host-only` | Host-side only; must not enter the axon container |
| `container` | Required inside the axon container |
| `compose-interp` | Compose file interpolation; never read by the Rust binary |
| `not-runtime` | Not read at runtime (docs-only, test, installer) |

---

## Registry Coverage (source: env_registry/*.rs)

### Endpoint URLs

| Key | Class | Placement | TOML dest | Secret | Source |
|-----|-------|-----------|-----------|--------|--------|
| `QDRANT_URL` | keep-env | both | — | no | runtime.rs |
| `TEI_URL` | keep-env | both | — | no | runtime.rs |
| `AXON_CHROME_REMOTE_URL` | keep-env | both | — | no | runtime.rs |
| `AXON_SERVER_URL` | keep-env | host-only | — | no | runtime.rs |
| `AXON_MCP_PUBLIC_URL` | keep-env | both | — | no | runtime.rs |
| `AXON_CHROME_PROXY` | keep-env | both | — | no | runtime.rs |

### Auth / MCP Security

| Key | Class | Placement | TOML dest | Secret | Source |
|-----|-------|-----------|-----------|--------|--------|
| `AXON_MCP_HTTP_TOKEN` | keep-env | container | — | **yes** | runtime.rs |
| `AXON_MCP_AUTH_MODE` | keep-env | both | — | no | runtime.rs |
| `AXON_MCP_GOOGLE_CLIENT_ID` | keep-env | both | — | no | runtime.rs |
| `AXON_MCP_GOOGLE_CLIENT_SECRET` | keep-env | both | — | **yes** | runtime.rs |
| `AXON_MCP_AUTH_ADMIN_EMAIL` | keep-env | both | — | no | runtime.rs |
| `AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS` | keep-env | both | — | no | runtime.rs |
| `AXON_MCP_ALLOWED_ORIGINS` | keep-env | both | — | no | runtime.rs |

### Third-Party Credentials

| Key | Class | Placement | TOML dest | Secret | Source |
|-----|-------|-----------|-----------|--------|--------|
| `TAVILY_API_KEY` | keep-env | both | — | **yes** | runtime.rs |
| `GITHUB_TOKEN` | keep-env | both | — | **yes** | runtime.rs |
| `REDDIT_CLIENT_ID` | keep-env | both | — | no | runtime.rs |
| `REDDIT_CLIENT_SECRET` | keep-env | both | — | **yes** | runtime.rs |
| `HF_TOKEN` | keep-env | compose-interp | — | **yes** | runtime.rs |
| `GEMINI_API_KEY` | keep-env | both | — | **yes** | runtime.rs |
| `GOOGLE_API_KEY` | keep-env | both | — | **yes** | runtime.rs |
| `GOOGLE_APPLICATION_CREDENTIALS` | trusted-bootstrap | both | — | no | advanced.rs |

### LLM / Gemini Headless

| Key | Class | Placement | TOML dest | Secret | Source |
|-----|-------|-----------|-----------|--------|--------|
| `AXON_HEADLESS_GEMINI_CMD` | keep-env | both | — | no | runtime.rs |
| `AXON_HEADLESS_GEMINI_MODEL` | keep-env | both | — | no | runtime.rs |
| `AXON_LLM_COMPLETION_CONCURRENCY` | keep-env | both | — | no | runtime.rs |
| `AXON_LLM_COMPLETION_TIMEOUT_SECS` | keep-env | both | — | no | runtime.rs |
| `AXON_HEADLESS_GEMINI_HOME` | trusted-bootstrap | both | — | no | advanced.rs |
| `OPENAI_MODEL` | compat-shim | both | — | no | runtime.rs (WarnEnvOverride) |
| `OPENAI_BASE_URL` | compat-shim | both | — | no | runtime.rs (WarnAndIgnore) |
| `OPENAI_API_KEY` | compat-shim | both | — | **yes** | runtime.rs (WarnAndIgnore) |

### Trusted Operator Bootstrap

| Key | Class | Placement | TOML dest | Secret | Source |
|-----|-------|-----------|-----------|--------|--------|
| `AXON_ENV_FILE` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_CONFIG_PATH` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_HOME` | trusted-bootstrap | both | — | no | advanced.rs |
| `AXON_DATA_DIR` | trusted-bootstrap | both | — | no | advanced.rs |
| `AXON_SQLITE_PATH` | trusted-bootstrap | both | — | no | advanced.rs |
| `AXON_COLLECTION` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_BIN` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_CHROME_DIAGNOSTICS_DIR` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_EXCLUDE_PATH_PREFIX` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_INSTALL_PREFIX` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_INSTALL_TMPDIR` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_INSTALL_URL` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_LOG_DIR` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_LOG_FILE` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_LOG_MAX_FILES` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_MCP_ARTIFACT_DIR` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_MCP_EMBED_ALLOWED_ROOTS` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_MCP_HTTP_HOST` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_MCP_HTTP_PORT` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_MCP_TRANSPORT` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_NEO4J_URL` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_OUTPUT_DIR` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_REPO_ROOT` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_SUGGEST_BASE_URL_LIMIT` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_SUGGEST_EXISTING_URL_LIMIT` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_TARGET_DIR` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `AXON_WEB_ALLOWED_ORIGINS` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `GEMINI_AUTH_FILES` | trusted-bootstrap | host-only | — | no | advanced.rs |
| `QDRANT_COLLECTION` | trusted-bootstrap | host-only | — | no | advanced.rs |

### Docker Compose Interpolation (compose-env)

These are read by Docker Compose from `~/.axon/.env` as variable substitution.
The Rust binary never reads most of them directly.

| Key | Class | Placement | Notes | Source |
|-----|-------|-----------|-------|--------|
| `AXON_MCP_HTTP_PUBLISH` | compose-env | compose-interp | Port mapping `${AXON_MCP_HTTP_PUBLISH:-8001}:8001` | advanced.rs |
| `AXON_IMAGE` | compose-env | compose-interp | Docker image tag override | advanced.rs |
| `AXON_LOG_COLOR` | compose-env | compose-interp | Sets `CLICOLOR_FORCE` inside container | advanced.rs |
| `AXON_IN_CONTAINER` | compose-env | container | Set by Compose to `"1"`; read by Rust binary | advanced.rs |
| `GEMINI_HOME` | compose-env | compose-interp | Volume mount path | advanced.rs |
| `TEI_EMBEDDING_MODEL` | compose-env | compose-interp | TEI server model arg | advanced.rs |
| `TEI_HTTP_PORT` | compose-env | compose-interp | TEI port binding | advanced.rs |
| `TEI_SERVER_MAX_CLIENT_BATCH_SIZE` | compose-env | compose-interp | TEI server arg (≠ client-side `TEI_MAX_CLIENT_BATCH_SIZE`) | advanced.rs |
| `TEI_MAX_BATCH_REQUESTS` | compose-env | compose-interp | TEI server concurrency | advanced.rs |
| `TEI_MAX_CONCURRENT_REQUESTS` | compose-env | compose-interp | TEI server concurrency | advanced.rs |
| `TEI_POOLING` | compose-env | compose-interp | TEI pooling strategy | advanced.rs |
| `NVIDIA_VISIBLE_DEVICES` | compose-env | compose-interp | GPU selection | advanced.rs |
| `CUDA_VISIBLE_DEVICES` | compose-env | compose-interp | GPU selection | advanced.rs |
| `CUDA_CACHE_DISABLE` | compose-env | compose-interp | GPU cache flag | advanced.rs |
| `NVIDIA_REQUIRE_CUDA` | compose-env | compose-interp | GPU requirement constraint | advanced.rs |
| `HF_HUB_CACHE` | compose-env | compose-interp | HF cache dir inside container | advanced.rs |
| `HF_HUB_ENABLE_HF_TRANSFER` | compose-env | compose-interp | HF transfer acceleration | advanced.rs |
| `TEI_MAX_BATCH_TOKENS` | compose-env | compose-interp | TEI server arg | advanced.rs |

### Move to TOML (migration.rs)

These env vars are being migrated to `config.toml`. Env override is retained temporarily with `WarnEnvOverride` behavior — setting them in env still works but emits a deprecation warning.

| Key | Class | Placement | TOML destination | Source |
|-----|-------|-----------|-----------------|--------|
| `TEI_MAX_CLIENT_BATCH_SIZE` | move-toml | not-runtime | `tei.max-client-batch-size` | migration.rs |
| `TEI_MAX_RETRIES` | move-toml | not-runtime | `tei.max-retries` | migration.rs |
| `TEI_REQUEST_TIMEOUT_MS` | move-toml | not-runtime | `tei.request-timeout-ms` | migration.rs |
| `AXON_INGEST_LANES` | move-toml | not-runtime | `workers.ingest-lanes` | migration.rs |
| `AXON_EMBED_LANES` | move-toml | not-runtime | `workers.embed-lanes` | migration.rs |
| `AXON_EMBED_DOC_TIMEOUT_SECS` | move-toml | not-runtime | `workers.embed-doc-timeout-secs` | migration.rs |
| `AXON_QUEUE_SUMMARY_SECS` | move-toml | not-runtime | `workers.queue-summary-secs` | migration.rs |
| `AXON_QDRANT_POINT_BUFFER` | move-toml | not-runtime | `workers.qdrant-point-buffer` | migration.rs |
| `AXON_MAX_PENDING_CRAWL_JOBS` | move-toml | not-runtime | `workers.max-pending-crawl-jobs` | migration.rs |
| `AXON_MAX_PENDING_EMBED_JOBS` | move-toml | not-runtime | `workers.max-pending-embed-jobs` | migration.rs |
| `AXON_MAX_PENDING_EXTRACT_JOBS` | move-toml | not-runtime | `workers.max-pending-extract-jobs` | migration.rs |
| `AXON_MAX_PENDING_INGEST_JOBS` | move-toml | not-runtime | `workers.max-pending-ingest-jobs` | migration.rs |
| `AXON_ASK_CANDIDATE_LIMIT` | move-toml | not-runtime | `ask.candidate-limit` | migration.rs |
| `AXON_ASK_CHUNK_LIMIT` | move-toml | not-runtime | `ask.chunk-limit` | migration.rs |
| `AXON_ASK_MIN_RELEVANCE_SCORE` | move-toml | not-runtime | `ask.min-relevance-score` | migration.rs |
| `AXON_ASK_HYBRID_CANDIDATES` | move-toml | not-runtime | `search.ask-hybrid-candidates` | migration.rs |
| `AXON_HYBRID_SEARCH` | move-toml | not-runtime | `search.hybrid-enabled` | migration.rs |
| `AXON_HYBRID_CANDIDATES` | move-toml | not-runtime | `search.hybrid-candidates` | migration.rs |
| `AXON_HNSW_EF_SEARCH` | move-toml | not-runtime | `search.hnsw-ef` | migration.rs |
| `AXON_HNSW_EF_SEARCH_LEGACY` | move-toml | not-runtime | `search.hnsw-ef-legacy` | migration.rs |
| `AXON_ASK_AUTHORITATIVE_DOMAINS` | move-toml | not-runtime | `ask.authoritative-domains` | migration.rs |
| `AXON_CHROME_USER_AGENT` | move-toml | not-runtime | `chrome.user-agent` | migration.rs |
| `AXON_JOB_WAIT_TIMEOUT_SECS` | move-toml | not-runtime | `workers.job-wait-timeout-secs` | migration.rs |
| `AXON_LOG_MAX_BYTES` | move-toml | not-runtime | `logging.max-bytes` | migration.rs |

### Delete (migration.rs — legacy removed paths)

| Key | Reason | Source |
|-----|--------|--------|
| `AXON_BATCH_QUEUE` | Legacy AMQP queue name | migration.rs |
| `AXON_CRAWL_QUEUE` | Legacy AMQP queue name | migration.rs |
| `AXON_EMBED_QUEUE` | Legacy AMQP queue name | migration.rs |
| `AXON_EXTRACT_QUEUE` | Legacy AMQP queue name | migration.rs |
| `AXON_INGEST_QUEUE` | Legacy AMQP queue name | migration.rs |
| `AXON_AMQP_URL` | RabbitMQ/AMQP path removed | migration.rs |
| `AXON_LITE` | Compatibility-only; accepted but no behavior change | migration.rs |
| `AXON_PG_MCP_URL` | Postgres path removed | migration.rs |
| `AXON_PG_URL` | Postgres path removed | migration.rs |
| `AXON_REDIS_URL` | Redis path removed | migration.rs |

---

## Direct Env Reads Covered by Registry

These keys are read directly in source code but already have registry classifications. This list is a cross-check, not an action queue.

### Direct Reads in src/

| Key | Read site | Registry class | Notes |
|-----|-----------|---------------|-------|
| `AXON_ASK_AUTHORITATIVE_DOMAINS` | `src/core/config/parse/build_config/config_literal.rs:243` | move-toml → `ask.authoritative-domains` | Registered in migration.rs |
| `AXON_CHROME_PROXY` | `src/core/config/parse/build_config/config_literal.rs:101` | keep-env | Registered in runtime.rs |
| `AXON_CHROME_USER_AGENT` | `src/core/http/client.rs:13,20` + `config_literal.rs:105` | move-toml → `chrome.user-agent` | Registered in migration.rs |
| `AXON_DOMAINS_DETAILED` | `src/cli/commands/domains.rs:21` | trusted-bootstrap | Registered in advanced.rs |
| `AXON_DOMAINS_FACET_LIMIT` | `src/cli/commands/domains.rs:30` | trusted-bootstrap | Registered in advanced.rs |
| `AXON_JOB_WAIT_TIMEOUT_SECS` | `src/jobs/backend.rs:148` | move-toml → `workers.job-wait-timeout-secs` | Registered in migration.rs |
| `AXON_LOG_FULL_QUERIES` | `src/services/search.rs:45` | trusted-bootstrap | Registered in advanced.rs |
| `AXON_LOG_MAX_BYTES` | `src/core/logging.rs:272` | move-toml → `logging.max-bytes` | Registered in migration.rs |
| `AXON_NO_WIPE` | `src/crawl/engine/dir_ops.rs:111` | trusted-bootstrap | Registered in advanced.rs |
| `AXON_SETUP_SKIP_SMOKE` | `src/services/setup/local/runtime.rs:94` | trusted-bootstrap | Registered in advanced.rs |
| `AXON_TEST_QDRANT_URL` | tests/ | delete/test-only | Registered in advanced.rs |
| `COLUMNS` | `src/core/config/help.rs` | hard-default | Standard Unix terminal env |
| `HOME` | multiple | hard-default | Standard Unix env; not user-settable |
| `NO_COLOR` | `src/core/logging.rs:122` | hard-default | Standard no-color.org env |

### Keys in .env.example Not in Registry

All keys previously in `.env.example` but missing from registry have been resolved:

| Key | Resolution |
|-----|-----------|
| `GEMINI_API_KEY` | ✅ Added to runtime.rs as KeepEnv (ztqd.1) |
| `GOOGLE_API_KEY` | ✅ Added to runtime.rs as KeepEnv (ztqd.1) |

### Keys in Live ~/.axon/.env Not in Registry or .env.example

| Key | Status | Action |
|-----|--------|--------|
| `AXON_WEB_API_TOKEN` | ✅ Added to runtime.rs registry as KeepEnv (ztqd.1) | Done |
| `CHROME_URL` | ✅ Added to migration.rs as Delete (ztqd.1) | Done |
| `TEI_MAX_BATCH_TOKENS` | ✅ Added to advanced.rs as ComposeEnv (ztqd.1) | Done |

---

## TOML [services] Section — Action Required

`src/core/config/parse/toml_config.rs` still has:
```rust
pub services: TomlServicesSection,  // qdrant_url, tei_url, chrome_remote_url
```

These are marked `#[allow(dead_code)]` with deprecation doc comments but the parser accepts them. Per the boundary: service URLs are **env only**. The migration path must be one of:
- parse-warn-and-ignore (current behavior, acceptable short-term)
- parse-warn-and-move-to-env (setup/repair writes them to .env)
- clear repairable error naming the env replacements

Current status: parse-warn-ignore is already implemented (`#[allow(dead_code)]` + deprecation comments). The parser accepts them silently. **The open work is to emit a warning at startup when these fields are set.**

---

## ACP Variables (pending axon_rust-387)

The following are documented in `docs/` but no longer have source reads in `src/`.
They should be deleted from `.env.example`, docs, and live `~/.axon/.env` as part of
`axon_rust-387` (remove ACP / standardize Gemini headless):

- `AXON_ACP_*` (all variants)
- `AXON_ASK_AGENT`
- `AXON_ASK_BACKEND`

---

## Live ~/.axon/.env vs .env.example Delta

Keys in live env but not in .env.example (stale or operator-specific):
- `AXON_HOME` — redundant with `AXON_DATA_DIR`; trusted-bootstrap
- `AXON_MCP_HTTP_HOST` — set by Compose; trusted-bootstrap for local overrides
- `AXON_MCP_HTTP_PORT` — set by Compose; trusted-bootstrap for local overrides
- `AXON_WEB_ALLOWED_ORIGINS` — in advanced.rs registry; add to .env.example
- `AXON_WEB_API_TOKEN` — secret; registered in runtime.rs; add to .env.example if the sample should advertise web API auth
- `CHROME_URL` — stale alias; delete from live .env
- `OPENAI_API_KEY` / `OPENAI_BASE_URL` — compat-shim; in registry as WarnAndIgnore
- `TEI_MAX_BATCH_REQUESTS`, `TEI_MAX_BATCH_TOKENS`, `TEI_MAX_CONCURRENT_REQUESTS`, `TEI_TOKENIZATION_WORKERS` — compose-env server args; already in registry

Keys in .env.example but not in live env (user hasn't set them):
- `AXON_IMAGE` — compose override; optional
- `GEMINI_API_KEY`, `GOOGLE_API_KEY` — optional alternate credential paths

---

## Acceptance Criteria Status

| Criterion | Status |
|-----------|--------|
| Migration matrix built from source-derived inventory | ✅ this doc |
| Each key classified with class + placement + secret risk | ✅ registry covers 101 keys |
| TOML destination for move-toml keys | ✅ migration.rs |
| Registry gaps identified | ✅ 14 src/ reads + 2 .env.example + 3 live .env |
| Direct env reads outside config parser documented | ✅ Gaps table above |
| No secret values printed | ✅ key names only |
| Container injection risk noted | ✅ compose-env / both placement |
| AXON_ENV_FILE / AXON_CONFIG_PATH shadowing risk noted | ✅ trusted-bootstrap with HostOnly |
| TOML [services] URL behavior documented | ✅ section above |
| Deprecation warnings wired for CompatibilityShim vars | ✅ ztqd.4: env_migration.rs emits tracing::warn! during `axon setup` migration |
| .env.example cleaned of Delete/CompatibilityShim/ACP keys | ✅ ztqd.4: trimmed to 34 lines matching target structure |
| Stale Delete-classified keys confirmed absent from active src/ | ✅ ztqd.4: only appear in #[cfg(test)] fixtures in parse.rs |
| GEMINI_API_KEY and GOOGLE_API_KEY classified in registry | ✅ ztqd.1: added to runtime.rs as KeepEnv |
| CHROME_URL stale alias classified for deletion | ✅ migration.rs: Delete entry added |
| TEI_MAX_BATCH_TOKENS added to registry | ✅ advanced.rs: ComposeEnv entry added |

### Scope Note (ztqd.4)

Deprecation warnings for `CompatibilityShim` vars (`OPENAI_MODEL`, `OPENAI_BASE_URL`, `OPENAI_API_KEY`) fire during `axon setup` env migration, not on every CLI invocation. The build_config/ path that runs at every startup is excluded from the allowed file ownership for this bead. If every-invocation coverage is needed, a follow-up bead should wire `env_registry::warn_compatibility_shims()` from within `into_config_with_sources()` in `build_config.rs`.

### Keys Intentionally Dropped from .env.example (ztqd.4)

The following KeepEnv/CompatibilityShim keys were in the old `.env.example` but are omitted from the new minimal target. They remain valid and work when set — they are just not shown in the example template:

| Key | Class | Reason omitted |
|-----|-------|---------------|
| `AXON_DATA_DIR` | trusted-bootstrap | Operator-specific path override; not default setup |
| `AXON_SERVER_URL` | keep-env | Rarely set; host-only URL override |
| `AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS` | keep-env | OAuth detail; omitted for brevity |
| `AXON_MCP_ALLOWED_ORIGINS` | keep-env | CORS detail; omitted for brevity |
| `GEMINI_API_KEY` | keep-env | Optional alternate Google credential path |
| `GOOGLE_API_KEY` | keep-env | Optional alternate Google credential path |
| `GOOGLE_APPLICATION_CREDENTIALS` | trusted-bootstrap | Service account path; not typical setup |
| `TEI_SERVER_MAX_CLIENT_BATCH_SIZE` | compose-env | TEI tuning; advanced use only |
| `OPENAI_MODEL` | compat-shim | Deprecated; WarnEnvOverride |
| `OPENAI_BASE_URL` | compat-shim | Deprecated; WarnAndIgnore |
| `OPENAI_API_KEY` | compat-shim | Deprecated; WarnAndIgnore |
