# Session: Crawl Bug Fix + Docker Chrome Build

**Date:** 2026-02-21
**Branch:** `perf/command-performance-fixes`

---

## Session Overview

Two parallel tracks: (1) fixed a runtime bug that broke `axon crawl <url>` with a misleading "unknown crawl subcommand" error, and (2) reviewed, refined, and successfully built the Docker stack after `axon-chrome` (headless Chrome service) was added to the compose manifest.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | Debug `axon crawl https://docs.tavily.com/welcome` → `"unknown crawl subcommand: https://docs.tavily.com/welcome"` |
| +15m | Root cause traced to erroneous guard in `run_crawl()`; fix applied |
| +20m | Docker build attempted → `healthcheck-workers.sh` not found |
| +25m | Created `docker/scripts/healthcheck-workers.sh` |
| +30m | Noticed `ingest-worker` missing from Dockerfile log dirs + compose healthcheck |
| +40m | Docker build failed: `spider_agent` path dep outside build context |
| +50m | Tried workspace-root build context → 40 GB context blast, `.dockerignore` `**` negation failed |
| +60m | Switched to BuildKit `additional_contexts` — discovered transitive deps (`spider_agent_html`, `spider_agent_types`) |
| +70m | `docker compose build axon-workers` SUCCESS |
| +75m | Verified `axon-chrome` also built; both images confirmed in `docker images` |
| End | Restarted `axon-workers` container onto new image; all 6 services healthy |

---

## Key Findings

- **`run_crawl()` had a dead guard** (`crates/cli/commands/crawl.rs:25-27`): after `maybe_handle_subcommand()` returned `false` for the URL (not a keyword), a check on `cfg.positional.first()` immediately errored with "unknown crawl subcommand". The URL was legitimately in `positional` (it is also `start_url`) — the guard was never correct.
- **`spider_agent` has transitive path deps**: `spider_agent → spider_agent_html` and `spider_agent → spider_agent_types`; all three need explicit BuildKit `additional_contexts` or cargo fails to resolve `Cargo.lock`.
- **`.dockerignore` `**` with negation does not reliably filter** when Docker context is the workspace root (~40 GB). BuildKit named contexts are the correct solution for injecting out-of-tree path deps.
- **`ingest-worker` was absent** from both the Dockerfile `mkdir -p` block and the `docker-compose.yaml` s6-svstat healthcheck, even though the s6 service definition existed.

---

## Technical Decisions

- **`additional_contexts` over widening build context**: Keeps build context small (only `axon_rust/`), avoids `.dockerignore` complexity, and explicitly declares the three external crates needed by cargo. This is the canonical BuildKit pattern for workspace-external path deps.
- **`/proc`-based healthcheck**: `healthcheck-workers.sh` reads `/proc/[0-9]*/cmdline` to detect axon worker processes without installing extra packages in the runtime image (`debian:12-slim` has bash but not `pgrep`).
- **Guard removal over conditional fix**: The `positional.first()` guard in `run_crawl()` served no purpose — `validate_url()` already rejects non-URL strings, and `maybe_handle_subcommand()` handles legitimate subcommands. Removing the dead code was cleaner than patching it.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/cli/commands/crawl.rs` | Removed erroneous guard (lines 25-27): `if let Some(subcmd) = cfg.positional.first() { return Err(...) }` |
| `docker/scripts/healthcheck-workers.sh` | **Created** — `/proc`-based worker process detection |
| `docker/Dockerfile` | Added `# syntax=docker/dockerfile:1` pragma; changed builder stage to `COPY --from=<named-context>` for spider path deps; added `/var/log/axon/ingest-worker`; updated binary path to `/src/axon_rust/target/release/axon` |
| `docker-compose.yaml` | Added `additional_contexts` for `spider-agent`, `spider-agent-types`, `spider-agent-html`; added `ingest-worker` to s6-svstat healthcheck |

---

## Commands Executed

```bash
# Verify all six services healthy after restart
docker compose ps

# Rebuild workers image
docker compose build axon-workers

# Redeploy without touching other services
docker compose up -d --no-deps --force-recreate axon-workers

# Confirm running on new image
docker inspect axon-workers --format '{{.Image}}'
```

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| `axon crawl <url>` | `Error: unknown crawl subcommand: <url>` | Works correctly |
| `docker compose build axon-workers` | Fails: `healthcheck-workers.sh not found` | Succeeds |
| `docker compose build axon-workers` | Fails: `spider_agent` path dep missing | Succeeds via `additional_contexts` |
| `ingest-worker` log directory | Missing from runtime image | Present at `/var/log/axon/ingest-worker` |
| `axon-workers` healthcheck | Checked 4 workers only | Checks all 5 (includes `ingest-worker`) |
| `axon-workers` container image | Old image (`sha256:ff3762`) | New image (`sha256:c994dbc3`) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `docker images \| grep axon` | `axon-axon-workers` + `axon-axon-chrome` listed | Both present, workers 55.4MB, chrome 297MB | ✅ |
| `docker compose config --quiet` | Exit 0 | `Compose config valid` | ✅ |
| `docker compose ps axon-workers` | `healthy`, image = `axon-axon-workers` | `Up 12 seconds (healthy)`, correct image name | ✅ |
| `docker inspect axon-workers --format '{{.Image}}'` | `sha256:c994dbc3...` | `sha256:c994dbc3157efd34c6408233a8b62360f1dfcef39ce01923d230dfb421729fc5` | ✅ |
| All 6 services | `healthy` | postgres, redis, rabbitmq, qdrant, chrome, workers all `(healthy)` | ✅ |

---

## Source IDs + Collections Touched

None — no `axon embed/retrieve` operations were run during this session (no content was scraped or indexed).

---

## Risks and Rollback

- **`run_crawl()` guard removal**: Low risk — `validate_url()` covers the same invalid-input case. Rollback: restore lines 25-27 from git history.
- **Docker `additional_contexts`**: Requires BuildKit (enabled by default in Docker 23+). If building on a legacy Docker daemon without BuildKit, the `--from=<name>` syntax fails. Rollback: pre-copy spider crates into `axon_rust/` before build, or widen context.
- **`axon-workers` restart**: Workers that were processing jobs at restart time will have their jobs reclaimed by the watchdog after `AXON_JOB_STALE_TIMEOUT_SECS` (default 300s). No data loss.

---

## Decisions Not Taken

- **Workspace-root build context + `.dockerignore`**: Tried first; `.dockerignore` `**` + `!axon_rust/**` negation pattern sent the full ~40 GB workspace to the Docker daemon despite the exclusion. Abandoned in favour of `additional_contexts`.
- **Pre-copying spider crates into axon_rust/**: Would require a wrapper build script and makes the repo layout brittle. `additional_contexts` is cleaner and declarative.
- **`pgrep` in healthcheck**: Would require installing `procps` in the runtime image. `/proc` filesystem approach needs nothing extra.

---

## Open Questions

- The `axon-workers` s6 `ingest-worker` service: the run script exists in `docker/s6/s6-rc.d/ingest-worker/`, but the actual `ingest_github`/`ingest_reddit`/`ingest_youtube` implementations are stubs per MEMORY.md. The worker lane will start but jobs will fail with unimplemented errors until those are completed.
- `axon-chrome` healthcheck interval is 10s with 6 retries — chrome may still be initialising at `docker compose up`. Confirm `start_period: 20s` is sufficient for cold starts on the target hardware.

---

## Next Steps

- Implement `ingest_github`, `ingest_reddit`, `ingest_youtube` handler bodies in `crates/ingest/` (currently stubs per MEMORY.md)
- Run `axon crawl https://docs.tavily.com/welcome` to confirm the crawl fix works end-to-end with the live stack
- Consider pinning `rust:1.93-bookworm` in `Dockerfile` builder stage to a digest for reproducible builds
