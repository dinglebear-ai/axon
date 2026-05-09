# Security Guardrails -- Axon

Safety and security patterns enforced across the Axon stack.

## Credential management

### Storage

- All credentials live in `~/.axon/.env` with `chmod 600` permissions
- Never commit real `.env` files
- Use `.env.example` as a tracked template with placeholder values only
- Direct Docker Compose commands should pass `--env-file ~/.axon/.env` so the canonical env file is used for `${VAR}` interpolation; service containers also read `${AXON_HOME:-${HOME}/.axon}/.env`

### Ignore files

`.gitignore` and `.dockerignore` must include:

```
.env
*.secret
*.pem
*.key
```

### Pre-commit enforcement

Lefthook hooks verify security invariants:

| Hook | Purpose |
|------|---------|
| `cargo xtask check-env-staged` | Blocks commits that include `.env` files |
| `check_dockerignore_guards.sh` | Verifies `.dockerignore` contains required patterns |

## Web app token model

Axon uses a two-tier token architecture for the web UI:

| Token | Scope | Browser-visible |
|-------|-------|-----------------|
| `AXON_WEB_API_TOKEN` | Primary -- gates `/api/*` and `/ws` | No |
| `AXON_WEB_BROWSER_API_TOKEN` | Second-tier -- gates `/api/*` only | Yes (via `NEXT_PUBLIC_AXON_API_TOKEN`) |
| `NEXT_PUBLIC_AXON_API_TOKEN` | Client-side -- sent as `x-api-key` and `?token=` | Yes |

Rules:
- `AXON_WEB_API_TOKEN` is server-only -- never set as a `NEXT_PUBLIC_*` variable
- When `AXON_WEB_BROWSER_API_TOKEN` is set, `NEXT_PUBLIC_AXON_API_TOKEN` must match it
- When `AXON_WEB_BROWSER_API_TOKEN` is unset, `NEXT_PUBLIC_AXON_API_TOKEN` must match `AXON_WEB_API_TOKEN`
- The `?token=` query param on WebSocket URLs is a necessary limitation (upgrade requests cannot carry custom headers)

## MCP OAuth

MCP OAuth (`atk_` tokens) is a separate auth system for MCP HTTP clients. It does not interact with the web UI token model.

## Docker security

### Non-root execution

Containers use s6-overlay with PID 1 running as root (required by s6). Worker processes drop privileges via `s6-setuidgid axon` (UID 1001):

```sh
exec s6-setuidgid axon /usr/local/bin/axon crawl worker
```

### No baked environment

Docker images must not contain credentials at build time:
- No `ENV AXON_PG_URL=...` in Dockerfiles
- No `COPY .env` in Dockerfiles
- Credentials are injected at runtime via `env_file:` or container `environment:`

### Image verification

```bash
# Check for baked secrets
docker inspect axon:local | jq '.[0].Config.Env'

# Check container revision matches git SHA
./scripts/check-container-revisions.sh
```

## Network security

### HTTPS in production

- All service URLs should use `https://` in production
- HTTP is acceptable for local development and Docker-internal networking
- The Chrome CDP endpoint is HTTP-only by design (internal network)

### URL validation

`validate_url()` in `crates/core/http.rs` enforces:
- No private/loopback IPs (SSRF protection)
- No file:// or other non-HTTP schemes
- Blocked malware/phishing domains (via Spider `firewall` feature)

## Input handling

### Crawl safety

- Default `--max-pages 0` is uncapped -- `AXON_CRAWL_SIZE_WARN_THRESHOLD` warns when exceeded
- `AXON_MAX_PENDING_CRAWL_JOBS` caps the queue to prevent runaway crawls
- Auto path-prefix scoping limits crawl scope on deep URLs
- `--respect-robots` defaults to `false` -- legal implications acknowledged

### Text chunking

`chunk_text()` splits at 2000 chars with 200-char overlap. Very long pages produce many Qdrant points -- monitor collection size after large crawls.

## Logging

- Never log credentials, tokens, or API keys
- CLI outputs JSON data to stdout and progress/logs to stderr
- Log rotation: 10 MB max, 3 backups (`AXON_LOG_MAX_BYTES`, `AXON_LOG_MAX_FILES`)
- ANSI codes are stripped from web UI output via `console::strip_ansi_codes()`
