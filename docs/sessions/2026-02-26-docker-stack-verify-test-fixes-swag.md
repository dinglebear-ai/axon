# Session: Docker Stack Verification, Test Fixes, SWAG Proxy
Date: 2026-02-26
Branch: feat/crawl-download-pack
Commit: 4e4a9d2

---

## Session Overview

Three-part session: (1) built and verified the fully containerized Docker stack (`axon-workers` + `axon-web`), shut down local dev servers; (2) fixed pre-existing Rust test failures caused by Docker hostname not resolving from the host; (3) updated stale TypeScript snapshots. Ended with creating a SWAG reverse proxy config for `axon.tootie.tv` pointing at the containerized Next.js UI on `dookie` (Tailscale `100.88.16.79:49010`), including an `axon-web` port binding fix to expose it on `0.0.0.0`.

---

## Timeline

1. **Docker stack verification** — rebuilt `axon-workers` (new `web-server` s6 service), built `axon-web` (fixing Dockerfile path bug in compose), started both, verified all s6 services and HTTP/WS endpoints.
2. **Clarified `axon serve` role** — corrected mischaracterization of `:49000` as "legacy"; it is the active WebSocket API backend the Next.js app depends on.
3. **Killed local dev servers** — `target/debug/axon serve --port 3939` (pid 1242280) and Next.js `next-server` (pid 4009188 + 1718080) shut down; only containerized processes remain.
4. **Killed stale background curl** — WebSocket test curl (bg8klyztf) left open holding a WS connection; killed via TaskStop.
5. **SWAG proxy config** — created `axon.subdomain.conf` for `axon.tootie.tv` → `100.88.16.79:49010` with MCP + Authelia; first attempt had no auth, recreated from scratch with `auth_method: authelia`.
6. **Port binding fix** — `axon-web` was bound to `127.0.0.1:49010`; changed to `0.0.0.0:49010` so SWAG on `squirts` can reach it over Tailscale.
7. **Rust test fixes** — applied `normalize_local_service_url()` to 5 test helper files; 426/0 confirmed with `--test-threads=1`.
8. **TypeScript snapshot update** — regenerated stale snapshots for `pulse-chat-pane-layout.test.ts`; 85/85 passing.
9. **Committed and pushed** — `a3b3b76` + `4e4a9d2` (changelog sha fix).

---

## Key Findings

- **Dockerfile path resolution**: `dockerfile:` in Docker Compose is relative to `context`, not the project root. `context: apps/web` + `dockerfile: docker/web/Dockerfile` looked for `apps/web/docker/web/Dockerfile` (doesn't exist). Fix: `dockerfile: ../../docker/web/Dockerfile`. (`docker-compose.yaml:204`)
- **Port binding**: `127.0.0.1:PORT` binds only to loopback; Tailscale/SWAG can't reach it. Must use `0.0.0.0:PORT` or just `PORT:PORT`. (`docker-compose.yaml:206`)
- **`normalize_local_service_url()` scope**: function is `pub(crate)` in `crates/core/config/parse.rs:33` — accessible from all test helpers via `crate::crates::core::config::parse::normalize_local_service_url`. Tests that called it individually passed because env vars weren't set (early-return skip); in parallel runs with env sourced, Docker hostnames caused auth failures.
- **Parallel DB test races**: 426 tests pass with `--test-threads=1`; parallel failures are pre-existing DDL races (multiple tests creating same tables concurrently), not caused by this session's changes.
- **Snapshot `[7m`/`[27m` codes**: these are Vitest terminal diff markers (ANSI reverse-video for character-level highlighting), not literal content in the rendered HTML. Snapshots were simply stale from the `pulse-chat-pane.tsx` rewrite (59 → 716 lines in `d1f20a49`).
- **s6-svstat path**: `s6-svstat` is not in `$PATH` inside the container — must use `/command/s6-svstat`.

---

## Technical Decisions

- **`../../docker/web/Dockerfile`** over moving the Dockerfile to `apps/web/`: keeps all Docker assets in `docker/`, avoids polluting the Next.js app directory.
- **`normalize_local_service_url()` in test helpers** (not in `make_pool`): keeps production code unchanged; only test paths rewrote URLs. Consistent with how `parse_args()` applies the same normalization for CLI use.
- **SWAG `auth_method: authelia`** passed to `create` action (not injected via `update`): the MCP tool handles the authelia include uncomment natively — no manual editing needed.
- **`0.0.0.0` binding** for `axon-web` only: `axon-workers` stays on `127.0.0.1:49000` (internal use, proxied by Next.js). Only the public-facing Next.js port needs external exposure for SWAG.

---

## Files Modified

| File | Change |
|------|--------|
| `docker-compose.yaml` | `axon-web` dockerfile path fix + port binding `127.0.0.1:49010` → `0.0.0.0:49010` |
| `crates/jobs/common/tests.rs` | Added `normalize_local_service_url` import + applied to two inline pg_url matches |
| `crates/jobs/crawl/runtime/tests.rs` | `pg_url()` helper wraps URL with `normalize_local_service_url()` |
| `crates/jobs/embed/tests.rs` | `pg_url()` helper wraps URL with `normalize_local_service_url()` |
| `crates/jobs/extract/tests.rs` | `pg_url()` helper wraps URL with `normalize_local_service_url()` |
| `crates/jobs/refresh/` (module) | `pg_url()` helper wraps URL with `normalize_local_service_url()` (part of monolith split) |
| `.env.example` | Clarified `AXON_TEST_PG_URL` comment — documents auto-normalization fallback |
| `apps/web/__tests__/__snapshots__/pulse-chat-pane-layout.test.ts.snap` | Regenerated stale snapshots (2 updated) |
| `CHANGELOG.md` | New entries for `a3b3b76` session work |
| SWAG `axon.subdomain.conf` | Created on `squirts` via SWAG MCP — `axon.tootie.tv` → `100.88.16.79:49010`, Authelia + MCP enabled |

---

## Commands Executed

```bash
# Docker
docker compose build axon-workers          # rebuilt with web-server s6 service
docker compose up -d axon-workers
docker exec axon-workers /command/s6-svstat /run/service/web-server
# → up (pid 49 pgid 49) 602 seconds

docker compose build axon-web             # fixed dockerfile path first
docker compose up -d axon-web
curl -s http://127.0.0.1:49000/           # → <title>Axon - Neural RAG Pipeline</title>
curl -sI http://127.0.0.1:49010/          # → HTTP/1.1 200 OK
curl -si --max-time 5 http://127.0.0.1:49010/ws -H "Upgrade: websocket" ...
# → HTTP/1.1 101 Switching Protocols

# Kill local servers
kill 1242280 1718079 1718080 4009188 4009476

# Rust tests
cargo check --lib                          # → Finished dev profile in 0.39s
cargo test --lib -- --test-threads=1       # → 426 passed; 0 failed

# TypeScript
pnpm vitest run __tests__/pulse-chat-pane-layout.test.ts -u
# → 2 snapshots updated
pnpm test                                  # → 17 passed (85 tests)

# Git
git add . && git commit -m "fix(docker+test): ..."   # → a3b3b76
git push                                              # → cec02a8..a3b3b76
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `axon-web` reachability | Only `127.0.0.1:49010` (loopback only) | `0.0.0.0:49010` (all interfaces incl. Tailscale) |
| `axon-web` build | `docker compose build axon-web` failed (`lstat apps/web/docker: no such file`) | Builds successfully |
| Rust integration tests | `password authentication failed` when `AXON_PG_URL` contained Docker hostname | Correctly rewrites to `127.0.0.1:53432` |
| TS snapshot tests | 2 failing (`pulse-chat-pane-layout`) | 85/85 passing |
| Public access | No reverse proxy | `axon.tootie.tv` via SWAG on squirts, Authelia-protected |
| Local dev servers | `axon serve --port 3939` + `next-server` running as local processes | Fully containerized; no local processes |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `/command/s6-svstat /run/service/web-server` | `up (pid NNN)` | `up (pid 49 pgid 49) 602 seconds` | ✅ |
| `curl -s http://127.0.0.1:49000/` | HTML response | `<title>Axon - Neural RAG Pipeline</title>` | ✅ |
| `curl -sI http://127.0.0.1:49010/` | `HTTP/1.1 200 OK` | `HTTP/1.1 200 OK` | ✅ |
| WebSocket upgrade to `:49010/ws` | `101 Switching Protocols` | `HTTP/1.1 101 Switching Protocols` | ✅ |
| `cargo test --lib -- --test-threads=1` | 0 failed | `426 passed; 0 failed` | ✅ |
| `pnpm test` (apps/web) | 0 failed | `17 passed (85 tests)` | ✅ |
| `docker compose ps axon-workers axon-web` | both healthy/up | both up, workers healthy | ✅ |
| SWAG health check on create | 200 | `200 (90ms)` | ✅ |

---

## Source IDs + Collections Touched

None (no `axon embed/retrieve/query` calls during this session).

---

## Risks and Rollback

- **`0.0.0.0` port exposure**: `axon-web` is now reachable from any network interface on `dookie`. Mitigated by Authelia on the SWAG proxy. If direct port access is a concern, add a host firewall rule or revert to `127.0.0.1:49010`.
- **Rollback docker-compose**: revert `0.0.0.0:49010` → `127.0.0.1:49010` and `dockerfile: ../../docker/web/Dockerfile` → `dockerfile: docker/web/Dockerfile`; `docker compose up -d axon-web`.
- **Rollback test normalization**: remove the `normalize_local_service_url()` calls and import from the 5 test files; tests will skip when Docker hostnames can't be resolved (original behavior).

---

## Decisions Not Taken

- **Moving `docker/web/Dockerfile` to `apps/web/Dockerfile`**: would have worked but pollutes the Next.js app directory with Docker tooling.
- **Changing context to `.` (project root)**: would require updating all `COPY` paths in the Dockerfile to `apps/web/...` — more invasive.
- **`AXON_TEST_PG_URL` pointing to `127.0.0.1:53432`**: adding this to `.env` would also fix tests, but doesn't scale — future Docker service hostnames would need the same manual treatment. `normalize_local_service_url()` is the durable fix.
- **Injecting Authelia into SWAG config manually**: user confirmed the `auth_method: authelia` parameter handles it natively — no manual editing.

---

## Open Questions

- **Parallel DB test races**: 9–13 tests fail when run in parallel (`cargo test --lib` without `--test-threads=1`). These are pre-existing DDL races, not introduced by this session. Tracked as known issue; a fix would require per-test DB isolation (e.g., unique table names per test run) or advisory lock scope widening.
- **`axon-web` no healthcheck**: the container shows `Up` but not `(healthy)` because no `HEALTHCHECK` is defined in `docker/web/Dockerfile`. Low priority — Next.js has no standard health endpoint yet.
- **`axon-workers` on `127.0.0.1:49000`**: still loopback-only. If the WS backend ever needs direct external access (bypassing the Next.js proxy), this would need the same `0.0.0.0` treatment.

---

## Next Steps

- Add `HEALTHCHECK` to `docker/web/Dockerfile` (e.g., `curl -f http://localhost:49010/` or Next.js `/api/health` route).
- Investigate parallel DB test race conditions — consider unique schema-per-test approach or `serial_test` crate.
- PR: `feat/crawl-download-pack` → `main` when branch is ready.
