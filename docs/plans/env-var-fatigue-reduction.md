# Plan: Eliminate Environment Variable Fatigue

## Context

`.env.example` has **112 active environment variables across 376 lines**. Nearly all have sensible defaults in the Rust code (`unwrap_or()`, `env_usize_clamped()`, etc.) or Docker Compose files (`${VAR:-default}`). Only ~10 are truly required — the app will error on startup without them. A new user faces a wall of configuration when in reality they need to set ~10 values and fill in their host paths.

**Goal:** Reduce `.env.example` to only what must be configured, with a compact commented reference section for expert tuning. No Rust code changes.

## Research Findings

### Verified: Required (no universal default, app errors without them)
- `AXON_PG_URL` — `build_config.rs:244` returns `Err()` if missing
- `AXON_REDIS_URL` — `build_config.rs:253` returns `Err()` if missing
- `AXON_AMQP_URL` — `build_config.rs:262` returns `Err()` if missing
- `POSTGRES_USER`, `POSTGRES_PASSWORD`, `POSTGRES_DB` — compose healthcheck + services.env
- `REDIS_PASSWORD` — compose `--requirepass` (has `:-changeme` fallback but insecure)
- `RABBITMQ_USER`, `RABBITMQ_PASS` — compose auth (has `:-axon`/`:-axonrabbit` fallbacks but insecure)
- `AXON_WEB_API_TOKEN` — API auth gate (`crates/web.rs:129`)

### Verified: Host-Specific (varies per machine)
- `HOST_HOME` — used by 8+ volume mounts in docker-compose.yaml (has `:-${HOME}` fallback)
- `AXON_WORKSPACE` — workspace mount (`:-${HOME}/workspace` fallback)
- `AXON_DATA_DIR` — data root (`:-./data` fallback)
- **`HOST_WORKSPACE` is ORPHANED** — only exists in .env.example lines 44-45, NOT referenced by any compose file or code. Remove entirely.

### Verified: Infrastructure Endpoints (have defaults, override for non-Docker)
- `QDRANT_URL` — default `http://127.0.0.1:53333` in `build_config.rs:381`
- `TEI_URL` — default empty (degrades gracefully) in `build_config.rs:376`
- `AXON_CHROME_REMOTE_URL` — default None (Chrome features disabled) in `build_config.rs:296`
- `AXON_BACKEND_URL` — hardcoded `http://axon-workers:49000` in compose line 109

### Verified: Everything Else Has Defaults
Confirmed in `build_config.rs` — every other env var uses `.unwrap_or()`, `.unwrap_or_default()`, `.unwrap_or_else()`, `env_bool(name, default)`, or `env_usize_clamped(name, default, min, max)`. None will error if missing.

### Compose Interpolation Safety
- `docker-compose.yaml`: ALL vars use `${VAR:-default}` — safe to omit
- `docker-compose.services.yaml`: ALL vars use `${VAR:-default}` EXCEPT:
  - Line 40: `${POSTGRES_USER}` and `${POSTGRES_DB}` in healthcheck — **needs `:-axon` fallback added**
  - Line 153: `${HF_TOKEN}` — Docker Compose substitutes empty string for undefined vars (warning only, not error). TEI treats empty as "no token".

### cont-init.d/10-load-axon-env
- Parses .env line-by-line, skips comments (`#` prefix) — commented-out lines are safe
- Only sets vars NOT already in environment (`if [[ -z "${!key+x}" ]]`) — compose `environment:` overrides win
- Uses `AXON_OUTPUT_DIR`, `AXON_MCP_ARTIFACT_DIR`, `AXON_CHROME_DIAGNOSTICS_DIR` with safe fallbacks

## Work Units

### Unit 1: Rewrite `.env.example`
**File:** `.env.example` (376 lines → ~130 lines)

Structure:
```
Section 1: CREDENTIALS (~10 active vars)
  - POSTGRES_USER/PASSWORD/DB, REDIS_PASSWORD, RABBITMQ_USER/PASS
  - AXON_PG_URL, AXON_REDIS_URL, AXON_AMQP_URL
  - AXON_WEB_API_TOKEN

Section 2: HOST PATHS (~3 active vars, all with noted fallbacks)
  - AXON_DATA_DIR (fallback: ./data)
  - HOST_HOME (fallback: $HOME)
  - AXON_WORKSPACE (fallback: $HOME/workspace)
  - REMOVE HOST_WORKSPACE (orphaned — not used anywhere)

Section 3: INFRASTRUCTURE ENDPOINTS (~4 active vars, Docker defaults shown)
  - QDRANT_URL, TEI_URL, AXON_CHROME_REMOTE_URL, AXON_BACKEND_URL
  - NEXT_PUBLIC_AXON_API_TOKEN (must match AXON_WEB_API_TOKEN)

Section 4: OPTIONAL REFERENCE (commented out, one line per var)
  - Grouped by subsystem: feature creds, Neo4j, queues, TEI, workers, hybrid, web, ACP, MCP, Chrome, ask, serve, logging, testing, build
```

**Remove entirely (not even in reference):**
- `HOST_WORKSPACE` — orphaned, not used by compose or code
- `AXON_BIN` — only relevant inside Docker, hardcoded in compose
- `AXON_SERVE_HOST` — internal only, default 127.0.0.1 in code
- `NVIDIA_VISIBLE_DEVICES` / `CUDA_VISIBLE_DEVICES` — compose has `:-0` defaults
- `CHROME_URL` — duplicate of `AXON_CHROME_REMOTE_URL`

### Unit 2: Update `CLAUDE.md` Environment Variables section
**File:** `CLAUDE.md` (lines 284-398)

Changes:
- Replace the 67-line bash block (lines 293-360) with a condensed ~20-line block showing ONLY required vars
- Add note: "See `.env.example` for all optional tuning knobs."
- Keep the Web App Security prose table (lines 364-371) — it documents auth architecture
- Trim the Web App Security bash block (lines 374-398) to just `AXON_WEB_API_TOKEN` + `NEXT_PUBLIC_AXON_API_TOKEN` with a reference to .env.example
- Keep the "Dev vs Container URL Resolution" section unchanged

### Unit 3: Add compose healthcheck fallbacks
**File:** `docker-compose.services.yaml` (line 40)

Change:
```yaml
# Before:
"pg_isready -h 127.0.0.1 -p 5432 -U ${POSTGRES_USER} -d ${POSTGRES_DB}",
# After:
"pg_isready -h 127.0.0.1 -p 5432 -U ${POSTGRES_USER:-axon} -d ${POSTGRES_DB:-axon}",
```

One-line fix. Makes compose work even if POSTGRES_USER/DB are commented out.

## Verification

```bash
# 1. Compose resolves with minimal .env (no warnings for missing vars)
docker compose -f docker-compose.services.yaml config > /dev/null 2>&1
docker compose config > /dev/null 2>&1

# 2. No Rust code changes → no build/test needed
# 3. cont-init.d/10-load-axon-env handles comments correctly (verified by reading the script)
```

## Files NOT Changed (verified no updates needed)
- `docker/CLAUDE.md` — no env var content, just s6 supervision docs
- `docker/README.md` — no env var content, just directory layout
- `docs/README.md` — documentation index, no env var content
- All Rust code — no changes, all env vars keep their reading logic and defaults
- `services.env` — gitignored, user's real secrets, not a template
