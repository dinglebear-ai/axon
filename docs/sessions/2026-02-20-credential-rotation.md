# Session: Redis & RabbitMQ Credential Rotation

**Date:** 2026-02-20
**Branch:** `perf/command-performance-fixes`
**Duration:** Short (< 30 min)

---

## Session Overview

Rotated weak default credentials for Redis (`changeme`) and RabbitMQ (`axonrabbit`) to cryptographically secure 64-character hex passwords. Updated `.env`, hardened `docker-compose.yaml` fallback defaults, applied changes live to running containers without downtime, and recovered `axon-workers` after the credential change broke its Redis connection.

---

## Timeline

1. Read `.env` and `docker-compose.yaml` to identify all credential reference points.
2. Generated two 32-byte hex passwords via `openssl rand -hex 32`.
3. Updated `.env` — both the `REDIS_PASSWORD` / `RABBITMQ_PASS` vars and the embedded URLs (`AXON_REDIS_URL`, `AXON_AMQP_URL`).
4. Hardened `docker-compose.yaml` — removed `:-changeme` / `:-axonrabbit` fallback defaults so missing `.env` fails loudly instead of silently using weak credentials.
5. Applied new Redis password live via `redis-cli CONFIG SET requirepass` (authenticated with old password first).
6. Applied new RabbitMQ password live via `rabbitmqctl change_password axon <new>`.
7. Verified both services accepted new credentials.
8. Observed `axon-workers` failing with `Password authentication failed - AuthenticationFailed` — workers had the old `AXON_REDIS_URL` from container start time.
9. Attempted `docker compose up -d --force-recreate axon-workers` — hit orphaned container name conflict.
10. Removed orphan (`df5d0cb57480_axon-workers`), recreated with correct name `axon-workers`.
11. Confirmed clean startup: all 8 worker lanes listening, no auth errors.

---

## Key Findings

- **`env_file` is read at container creation, not restart.** Workers running with old credentials will fail immediately after a live Redis password change. `docker compose restart` would not have helped — only `docker compose up -d` (which recreates the container) picks up new `.env` values.
- **Redis `CONFIG SET requirepass` is live and persistent within the current run** but does NOT persist across container restarts unless the new password is also in the startup command (`--requirepass`). Since `docker-compose.yaml` now uses `${REDIS_PASSWORD}` (no fallback), a container restart will correctly pick up the new value from `.env`.
- **RabbitMQ password is stored in the Mnesia database on the persistent volume.** `rabbitmqctl change_password` writes directly to the DB — no restart needed, survives container restarts.
- **Orphaned container name conflict** occurs when docker compose tries to rename a container that was previously started under a different compose project ID. Resolved by stopping + removing the orphan manually, then re-running `docker compose up -d`.

---

## Technical Decisions

- **Hex over base64** for generated passwords: 64-char hex is URL-safe without percent-encoding (no `+`, `/`, `=` that break Redis/AMQP connection strings).
- **Removed fallback defaults** from `docker-compose.yaml` rather than updating them to new values: a missing `.env` should be a hard failure, not a silent startup with any default credential.
- **Live credential update instead of restart**: Changed credentials in running Redis and RabbitMQ without service downtime, then separately restarted only the workers (which needed the new env vars anyway).

---

## Files Modified

| File | Change |
|------|--------|
| `.env` | `REDIS_PASSWORD`, `RABBITMQ_PASS`, `AXON_REDIS_URL`, `AXON_AMQP_URL` — all updated to new 64-char hex credentials |
| `docker-compose.yaml` | Removed `:-changeme` fallback from Redis `requirepass` command and healthcheck; removed `:-axon` / `:-axonrabbit` fallbacks from RabbitMQ `RABBITMQ_DEFAULT_USER` / `RABBITMQ_DEFAULT_PASS` |

---

## Commands Executed

```bash
# Generate credentials
openssl rand -hex 32   # × 2 (one for Redis, one for RabbitMQ)

# Apply live to running containers
docker exec axon-rabbitmq rabbitmqctl change_password axon <new-pass>
docker exec axon-redis redis-cli -a changeme CONFIG SET requirepass <new-pass>

# Verify new credentials work
docker exec axon-redis redis-cli -a <new-pass> ping
# → PONG
docker exec axon-rabbitmq rabbitmqctl authenticate_user axon <new-pass>
# → Success

# Recreate workers to pick up new .env
docker compose up -d --force-recreate axon-workers  # hit name conflict
docker stop df5d0cb57480_axon-workers && docker rm df5d0cb57480_axon-workers
docker compose up -d axon-workers                   # succeeded, correct name
```

---

## Behavior Changes (Before / After)

| Aspect | Before | After |
|--------|--------|-------|
| Redis password | `changeme` (weak default) | 64-char hex (cryptographically random) |
| RabbitMQ password | `axonrabbit` (weak default) | 64-char hex (cryptographically random) |
| `docker-compose.yaml` fallback | `:-changeme` / `:-axonrabbit` silently start with weak creds if `.env` missing | No fallback — container refuses to start if `REDIS_PASSWORD` / `RABBITMQ_PASS` unset |
| Worker Redis auth | Old `AXON_REDIS_URL` in container env (old password) | New `AXON_REDIS_URL` (new password) — picked up on recreation |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `redis-cli -a <new-pass> ping` | `PONG` | `PONG` | ✅ Pass |
| `rabbitmqctl authenticate_user axon <new-pass>` | `Success` | `Success` | ✅ Pass |
| `docker compose ps` | All containers healthy | All 6 containers healthy | ✅ Pass |
| Worker logs post-restart | No auth errors, all lanes listening | 8 lanes listening, no errors | ✅ Pass |

---

## Source IDs + Collections Touched

None — no Qdrant/TEI operations this session.

---

## Risks and Rollback

**Risk:** Redis `CONFIG SET requirepass` does not persist to the AOF/RDB snapshot format used by `--appendonly yes`. If the Redis container is killed (not gracefully stopped) before a BGSAVE completes, the password reverts to the one baked into the startup command (`${REDIS_PASSWORD}`) — which is now the new password from `.env`. So in practice this is safe.

**Risk:** If `.env` is lost or corrupted, `docker compose up` will now fail hard (no fallback). Keep a secure backup of `.env`.

**Rollback:** To revert to old credentials, restore old values in `.env`, run `redis-cli CONFIG SET requirepass changeme`, and `rabbitmqctl change_password axon axonrabbit`. Then recreate workers.

---

## Decisions Not Taken

- **`docker compose restart axon-workers`**: Would have reused the existing container environment (old credentials). Rejected in favor of `up -d` which recreates.
- **Rotating Postgres password simultaneously**: Not in scope for this session; Postgres password in `.env` is already non-default and strong.
- **Base64 passwords**: Rejected due to `+`, `/`, `=` characters requiring URL-encoding in connection strings.

---

## Open Questions

- Redis `CONFIG REWRITE` was not run — if Redis is restarted without a config file (which is the case here, using command-line args), the startup command already carries the correct password via `${REDIS_PASSWORD}`. Verify this holds on next Redis restart.
- `axon-webdriver` was not restarted (has `env_file: []` override — intentional, it carries no secrets).

---

## Next Steps

- No immediate follow-up required.
- Consider storing `.env` in a secrets manager (Vault, age-encrypted file) for future sessions.
- Periodic credential rotation should be documented in runbook.
