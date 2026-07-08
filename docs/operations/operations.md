# Operations Runbook
Last Modified: 2026-05-06

Operator runbook for Axon (SQLite-backed). Task-oriented: one heading per
operational task. Every command in this file is verified against the current
codebase — see the cited source paths in each section.

> **SQLite/in-process jobs are the runtime.** All jobs persist in SQLite and
> workers run in-process inside the `axon mcp` (or `axon serve`) tokio
> runtime. The only required external services are **Qdrant** and **TEI**;
> **Chrome** is required for `--render-mode chrome` / `auto-switch` and for
> `screenshot`.

## Related docs

- [`PERFORMANCE.md`](performance.md) — concurrency profiles, watchdog tuning, retrieval knobs
- [`SECURITY.md`](security.md) — SSRF, allowlist, secrets handling
- [`JOB-LIFECYCLE.md`](../reference/job-lifecycle.md) — pending/running/completed/failed/canceled state machine
- [`DEPLOYMENT.md`](deployment.md) — production deployment + env reference
- [`CONFIG.md`](../guides/configuration.md) — every env var
- [`MCP.md`](../reference/mcp/overview.md) — MCP server runtime

---

## Day 0 — Bootstrap

```bash
./scripts/dev-setup.sh
```

The script installs system tools + Rust toolchain, creates `~/.axon/.env` from
`.env.example`, keeps `AXON_DATA_DIR` on the canonical `~/.axon` appdata root
unless overridden, pre-creates persistent directories, and brings infra up.

Manual equivalent:

```bash
mkdir -m 700 -p ~/.axon
cp .env.example ~/.axon/.env            # then edit
chmod 600 ~/.axon/.env
mkdir -p "${AXON_DATA_DIR:-$HOME/.axon}"/{output,tei,artifacts,logs}
just services-up
```

Required values in `~/.axon/.env`:

| Var | Purpose |
|-----|---------|
| `AXON_DATA_DIR` | Host root for SQLite, Qdrant volume, TEI cache, output; default `~/.axon` |
| `QDRANT_URL` | Default: `http://100.120.242.29:53333` (tootie) |
| `TEI_URL` | Default: `http://127.0.0.1:52000` |
| `AXON_HEADLESS_GEMINI_CMD` | Gemini CLI command — required for `ask`/`evaluate`/`research`/`debug`/`suggest`/extract fallback |
| `AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL` | Optional Gemini synthesis model override; `AXON_HEADLESS_GEMINI_MODEL` remains a legacy alias |
| `TAVILY_API_KEY` | Tavily fallback for `search` and `research` when `AXON_SEARXNG_URL` is unset |
| `REDDIT_CLIENT_ID` / `REDDIT_CLIENT_SECRET` | Required for Reddit ingest |

---

## Start services

Infrastructure (TEI + Chrome locally; Qdrant remote on tootie by default):

```bash
just services-up
# equivalent to:
docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml up -d axon-tei axon-chrome
```

Use `just qdrant-up` only when you intentionally want an optional local Qdrant
container, and set `AXON_QDRANT_URL=http://axon-qdrant:6333` for the Axon
container in that mode.

Foreground dev loop (builds binary, starts infra, runs `axon mcp` in-process):

```bash
just dev
```

`just dev` requires the binary to build cleanly — if `cargo build` fails, no
processes are started. Workers spawn inside the `axon mcp` runtime via
`SqliteJobBackend::new_with_workers`; CLI fire-and-forget submissions are
processed by that running `axon mcp` (or `axon serve`).

> If you submit `--wait false` jobs **without** an `axon mcp` / `axon serve`
> process running, the jobs are persisted to SQLite and stay `pending`
> indefinitely. Either keep `axon mcp` running or pass `--wait true`.

---

## Health checks

```bash
./scripts/axon doctor
./scripts/axon status
curl -H "Host: axon.tootie.tv" http://127.0.0.1:40090/readyz
```

`axon doctor` (SQLite-runtime probe at `src/core/health/doctor/sqlite.rs`) reports:

- **SQLite** — file exists at `cfg.sqlite_path`, `PRAGMA quick_check`, runtime
  IOERR count, recovery sidecars, the active lock path, whether the lock file
  exists, and whether an active owner was observed holding that lock
- **TEI** — `GET /health`, plus `/info` for embedding model + summary
- **Qdrant** — `GET /readyz`, plus `/collections/{name}` for vector mode (named/unnamed)
- **Chrome** — `chrome_remote_url` if configured
- **Gemini headless LLM** — command/config status; first `axon ask` smoke proves auth and completion
- **Vector mode mismatch** — warns if collection is unnamed but `AXON_HYBRID_SEARCH=true`

`axon status` reports per-kind job counts (Crawl / Extract / Embed / Ingest)
and recent jobs (top 10). Its JSON payload includes the same full SQLite
diagnostics as `doctor`.

`/readyz` is intentionally cheaper than `doctor`/`status`: it checks Qdrant,
TEI, and SQLite runtime health, but it does not run `PRAGMA quick_check`.
SQLite readiness is marked `not_ready` after an in-process IOERR is observed or
when an existing jobs database has no observed active owner lock.

---

## Submit work

The bundled CLI runs commands in-process against the configured Qdrant, TEI,
Chrome, and SQLite paths. Generic CLI client-to-server forwarding was removed
in 5.0.0, so `AXON_SERVER_URL`, `AXON_LOCAL_MODE`, `--local`, and
`AXON_SERVER_INSECURE` are not part of normal command execution.

For remote operation, run `axon serve` and use the first-party `/v1` REST routes
or MCP-over-HTTP directly. That server process owns its own `AXON_DATA_DIR`
(default `~/.axon`) and job database.

To diagnose stale runtime drift:

```bash
which -a axon
axon --version
ss -ltnp '( sport = :8001 )'
curl -H "x-api-key: $AXON_HTTP_TOKEN" http://127.0.0.1:8001/v1/status
```

If the binary or schema is stale, rebuild and restart the server on port
`8001` before retrying HTTP/MCP clients.

Synchronous (block until done):

```bash
./scripts/axon scrape https://example.com --wait true
./scripts/axon embed docs/architecture/overview.md --wait true
./scripts/axon crawl https://docs.rs/spider --wait true
```

Asynchronous (default) — requires a running `axon mcp` / `axon serve`:

```bash
./scripts/axon crawl https://docs.rs/spider          # enqueue, returns job ID
./scripts/axon ingest owner/repo                     # GitHub
./scripts/axon ingest r/rust                         # Reddit
./scripts/axon ingest https://www.youtube.com/watch?v=...   # YouTube
```

Track progress:

```bash
./scripts/axon status
./scripts/axon crawl list
./scripts/axon crawl status <job-id>
./scripts/axon crawl errors <job-id>
```

Same subcommand pattern for `extract`, `embed`, `ingest`.

---

## Recover stuck jobs

Two failure modes, two recovery paths.

### Process died mid-job (job stuck in `running`)

The watchdog reclaims jobs whose `updated_at` heartbeat goes stale. Defaults:
`AXON_JOB_STALE_TIMEOUT_SECS=300` + `AXON_JOB_STALE_CONFIRM_SECS=60`.

Stale reclaim runs when `SqliteJobBackend` starts and on the periodic worker
watchdog tick. To force a reclaim immediately:

```bash
./scripts/axon crawl recover
./scripts/axon extract recover
./scripts/axon embed recover
./scripts/axon ingest recover
```

This re-queues stale jobs as `pending`. Implemented in
`src/cli/commands/crawl/subcommands.rs:60` and the equivalent
ingest/extract/embed handlers via `services::jobs::recover_jobs`.

### Process alive but job hung

In-process workers track per-job heartbeats. When `result_json` does not
advance for 6 × 30s = 3 min the worker logs a warning; at 20 × 30s = 10 min
the worker kills the job and marks it `failed`. No operator action is
required — see `src/jobs/CLAUDE.md` "Liveness Enforcement (Two Tiers)".

### Cancel a running job

```bash
./scripts/axon crawl cancel <job-id>
```

Cancellation flips status to `canceled` and signals the spider control
channel. Workers honor the signal between page batches; an in-flight HTTP
fetch will complete first.

---

## Clear and clean up jobs

```bash
./scripts/axon crawl cleanup            # removes terminal jobs (completed/failed/canceled)
./scripts/axon crawl clear              # DESTRUCTIVE: removes ALL crawl jobs (prompts unless --yes)
```

Same pattern for `extract`, `embed`, `ingest`. `clear` requires confirmation
unless `--yes` is set or stdout is not a TTY (see `src/core/ui.rs::confirm_destructive`).

For non-interactive automation:

```bash
./scripts/axon crawl clear --yes
```

---

## Backup and restore — SQLite jobs DB

The jobs database lives at:

```
${AXON_DATA_DIR}/jobs.db          # default: ~/.axon/jobs.db
```

Override path with `AXON_SQLITE_PATH`. Schema is auto-created at startup by
`ensure_schema()` in each `src/jobs/store.rs`-driven worker — there
is no migration file to apply manually.

### Hot backup (process running)

SQLite supports online snapshots via the `.backup` command. Stop submitting
new work first (or accept that in-flight jobs will be replayed by the
watchdog on restore).

```bash
DB="${AXON_DATA_DIR:-$HOME/.axon}/jobs.db"
sqlite3 "$DB" ".backup '/path/to/backup/jobs-$(date -u +%Y%m%dT%H%M%SZ).db'"
```

### Cold backup (process stopped)

Stop `axon mcp` / `axon serve` first, then copy:

```bash
just stop
cp "${AXON_DATA_DIR:-$HOME/.axon}/jobs.db"      backup/jobs.db
cp "${AXON_DATA_DIR:-$HOME/.axon}/jobs.db-wal"  backup/  2>/dev/null || true
cp "${AXON_DATA_DIR:-$HOME/.axon}/jobs.db-shm"  backup/  2>/dev/null || true
```

Always copy `-wal` and `-shm` alongside the main file when the process was
running — they hold uncommitted writes.

### Restore

```bash
just stop
cp backup/jobs.db      "${AXON_DATA_DIR:-$HOME/.axon}/jobs.db"
rm -f                  "${AXON_DATA_DIR:-$HOME/.axon}/jobs.db-wal" \
                       "${AXON_DATA_DIR:-$HOME/.axon}/jobs.db-shm"
just dev
```

On startup the watchdog reclaims any rows still in `running` — they will
restart from `pending`.

---

## Backup and restore — Qdrant

Qdrant data is persisted at the host bind mount declared in `docker-compose.prod.yaml`
on the host running Qdrant, normally tootie:

```bash
${AXON_HOME:-$HOME/.axon}/qdrant
```

Two options:

### Snapshot via Qdrant API (preferred)

```bash
# Create a named snapshot (runs while Qdrant is up)
curl -s -X POST "${QDRANT_URL}/collections/${AXON_COLLECTION:-axon}/snapshots"

# List
curl -s "${QDRANT_URL}/collections/${AXON_COLLECTION:-axon}/snapshots"

# Download (path returned by the create call lives under /qdrant/snapshots)
ssh tootie 'docker exec axon-qdrant ls /qdrant/snapshots'
ssh tootie 'docker cp axon-qdrant:/qdrant/snapshots/<file> /tmp/<file>'
scp tootie:/tmp/<file> ./backup/
```

Restore: copy the snapshot back into the container and POST to
`/collections/{name}/snapshots/recover` with the snapshot location. See
[Qdrant docs](https://qdrant.tech/documentation/concepts/snapshots/).

### Volume-level cold backup

Stop Qdrant, then archive the bind mount:

```bash
ssh tootie 'docker stop axon-qdrant'
ssh tootie 'tar -czf /tmp/qdrant-backup.tgz -C "${AXON_HOME:-$HOME/.axon}" qdrant/'
scp tootie:/tmp/qdrant-backup.tgz ./backup/
ssh tootie 'docker start axon-qdrant'
```

This is faster for one-shot full backups and includes index state.

---

## Logs and diagnostics

### Application logs (Rust)

`init_tracing()` in `crates/axon-core/src/logging.rs` writes to two sinks:

- **stderr**, default level `WARN` (overridable with `RUST_LOG=info`)
- **size-rotated JSON file** at `AXON_LOG_PATH` when set, otherwise
  `${AXON_DATA_DIR}/logs/axon.log` (`~/.axon/logs/axon.log`), `INFO` level. Rotation triggers
  when the active file exceeds `AXON_LOG_MAX_BYTES` (default 10 MiB);
  archives are renamed `<file>.1`, `<file>.2`, … up to `AXON_LOG_MAX_FILES`
  (default `3`). The oldest archive is pruned on each rotation.

```bash
# tail the active log
tail -f "${AXON_DATA_DIR:-$HOME/.axon}/logs/axon.log"

# noisier output for one run
RUST_LOG=info,axon::jobs=debug just dev
```

`tracing` filters honor `RUST_LOG`. CDP decoder noise is suppressed by
`init_tracing`.

### Container logs

```bash
docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml logs -f axon-tei axon-chrome
# Qdrant logs live on tootie:
ssh tootie 'docker logs -f axon-qdrant'
```

Container logs are JSON-formatted, capped at 10 MB × 3 files (see
`docker-compose.prod.yaml:21-25`).

### Chrome diagnostics

When debugging Chrome/CDP issues:

```bash
AXON_CHROME_DIAGNOSTICS=1 \
AXON_CHROME_DIAGNOSTICS_DIR=.cache/chrome-diagnostics \
./scripts/axon crawl https://example.com --render-mode chrome --wait true
```

Enables screenshot + event capture. See `src/core/health.rs`.

---

## Performance tuning

Pick a profile per-command:

```bash
./scripts/axon crawl https://docs.rs/spider --performance-profile extreme --wait true
```

Profiles: `high-stable` (default), `balanced`, `extreme`, `max`. Granular
overrides: `--crawl-concurrency-limit`, `--batch-concurrency`,
`--request-timeout-ms`, `--fetch-retries`, `--retry-backoff-ms`.

For the full performance model see [`PERFORMANCE.md`](performance.md).

Watchdog and queue cap tuning:

| Env var | Default | Effect |
|---|---|---|
| `AXON_JOB_STALE_TIMEOUT_SECS` | 300 | Heartbeat staleness before reclaim |
| `AXON_JOB_STALE_CONFIRM_SECS` | 60 | Grace before stale rows are reclaimed |
| `AXON_INGEST_LANES` | 2 | Concurrent ingest worker lanes (clamped 1-16) |
| `AXON_EMBED_DOC_TIMEOUT_SECS` | 300 | Per-document embed timeout |
| `AXON_MAX_PENDING_CRAWL_JOBS` | 100 | Reject submission when N pending; `0` = unlimited |
| `AXON_MAX_PENDING_EMBED_JOBS` | 50 | Same, for embed |
| `AXON_MAX_PENDING_EXTRACT_JOBS` | 50 | Same, for extract |
| `AXON_MAX_PENDING_INGEST_JOBS` | 50 | Same, for ingest |

Implemented in `src/jobs/ops/enqueue.rs`.

---

## Reindex a Qdrant collection

When the embedding model, chunk strategy, source-doc planner, or payload schema
changes you must rebuild from source. The normalized source-doc planner owns
file/code/markdown/plain-text chunking and emits schema-versioned payload fields
such as `chunk_content_kind`, `chunk_locator`, `source_range`,
`chunking_fallback`, and `code_chunk_source`; old chunks will not gain those
fields until the source is re-embedded or re-ingested.

```bash
# 1. Rename the live collection (or pick a new name)
curl -X DELETE "${QDRANT_URL}/collections/${AXON_COLLECTION}"

# 2. Re-embed everything you care about — the collection is recreated by
#    ensure_collection() on first upsert.
./scripts/axon embed /path/to/source-tree --wait true
./scripts/axon ingest owner/repo --wait true
```

For document-level surgery (single URL):

```bash
./scripts/axon retrieve https://example.com/page    # see what's stored
./scripts/axon embed https://example.com/page --wait true   # re-embeds the URL
```

Embedding the same URL again uses deterministic point IDs and upserts first,
then deletes stale tail chunks whose old `chunk_index` is beyond the new chunk
count. Cleanup failures now fail the embedding operation so partial replacement
does not look successful.

---

## Migrate to named-vector hybrid search

Older collections store an unnamed dense vector. Hybrid Reciprocal-Rank-Fusion
search requires named `dense` + `bm42` sparse vectors. Migrate with:

```bash
./scripts/axon migrate --from cortex --to cortex_v2
```

The migrate command (handler at `src/cli/commands/migrate.rs:11`) scrolls
all points from the source, computes BM42 sparse vectors locally from the
existing `chunk_text` payload (no TEI calls), and writes named-mode points to
the destination. After it finishes:

```bash
# Update .env
AXON_COLLECTION=cortex_v2

# CRITICAL: restart axon mcp / axon serve
just stop
just dev
```

**You must restart all worker processes after migration.** The process-wide
`VectorMode` cache is invalidated for the just-migrated source/destination
in-process, but separate worker processes retain stale `Unnamed` mode until
restart and silently fall back to dense-only search. See `axon doctor`'s
`mode_mismatch_warning` field.

The `from` collection must be unnamed; named source collections are rejected.
The `to` collection is created if missing; re-running is idempotent.

---

## Common errors and fixes

| Symptom | Likely cause | Fix |
|---|---|---|
| `axon doctor` shows `tei.ok=false` | TEI container down or model still loading | `docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml ps`; wait for healthcheck (`start_period: 20s`); check `docker logs axon-tei` for CUDA OOM |
| `tei` returns `429`/`503` mid-run | Model overload / CUDA pressure, or `TEI_MAX_BATCH_REQUESTS` lower than the active client fanout | For RTX 4070 + Qwen3-Embedding-0.6B, keep `TEI_MAX_BATCH_TOKENS=196608`, `TEI_MAX_BATCH_REQUESTS=512`, `TEI_MAX_CLIENT_BATCH_SIZE=128`, `AXON_EMBED_POOL_MAX_INPUTS=512`, and `AXON_TEI_MAX_IN_FLIGHT_INPUTS=320`. If TEI OOMs during warmup, reduce `TEI_MAX_BATCH_TOKENS`. TEI client auto-retries on 429/5xx. |
| Qdrant upserts are slow | Oversized requests, too many tiny requests, or active indexing/optimizer work competing with writes | Default to `AXON_QDRANT_UPSERT_BATCH_SIZE=1024` and `AXON_QDRANT_UPSERT_PARALLELISM=1` for local docs workloads. For larger or remote imports, test `AXON_QDRANT_UPSERT_BATCH_SIZE=256` with `AXON_QDRANT_UPSERT_PARALLELISM=2-4`. For fresh large imports, run with `AXON_QDRANT_BULK_LOAD=true` so HNSW indexing is delayed until after upload, then restored automatically. Check `bench-embed` Qdrant request deltas, `optimizer_status`, and `segments_count`. |
| Fresh docs collection setup is slow | Full payload-index set or high-cost HNSW build settings | For docs-only collections, benchmark `AXON_QDRANT_PAYLOAD_INDEX_PROFILE=core`; for ingest-speed experiments, benchmark lower `AXON_QDRANT_HNSW_M` / `AXON_QDRANT_HNSW_EF_CONSTRUCT`, then validate retrieval quality before keeping them. |
| `qdrant connection refused` | Qdrant not started on tootie or `QDRANT_URL` wrong | Verify `curl ${QDRANT_URL}/readyz`; use `just qdrant-up` only for an intentional local fallback |
| `queue cap exceeded` on submit | `AXON_MAX_PENDING_*` reached | Run `axon <kind> list` to inspect; `axon <kind> cleanup` removes terminal rows; raise the cap or set to `0` |
| Jobs sit `pending` forever | No `axon mcp` / `axon serve` process running | Start `just dev` or pass `--wait true` |
| Job stuck `running` past 10 min | Worker hang | Heartbeat watchdog will mark `failed` automatically; or run `axon <kind> recover` |
| Hybrid search returns dense-only results | Collection is in legacy unnamed mode | `axon doctor` will surface `mode_mismatch_warning`; run `axon migrate --from <old> --to <new>` and restart |
| Most pages flagged as thin | Site is JS-rendered | `--render-mode chrome` or `auto-switch`; do NOT change `readability: false` in `src/core/content.rs` (confirmed regression) |
| `Chrome` probe fails in doctor | `axon-chrome` container down | `docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml restart axon-chrome` |
| Streaming `ask` panics on multibyte char | Out-of-date binary | Pull `main`, rebuild — `src/vector/ops/commands/ask/context/retrieval.rs` uses `.get(i..)` |

---

## Safe shutdown

```bash
just stop                # kills axon mcp / workers (pkill against 'axon.*(mcp|... worker)')
just services-down       # docker compose down (preserves volumes)
```

For a clean shutdown that lets in-flight work finish:

```bash
# 1. Stop accepting new submissions (don't run any new axon commands)
# 2. Watch jobs drain
watch -n 2 './scripts/axon status'
# 3. Once all jobs are terminal:
just stop
just services-down
```

Volumes (`${AXON_HOME:-$HOME/.axon}/{qdrant,tei,...}`) are bind-mounted on the host that runs each service and persist across restarts. Keep `AXON_HOME` aligned with `AXON_DATA_DIR` unless relocating the whole Axon appdata tree; with remote Qdrant, the qdrant volume is on tootie.

---

## Qdrant OOM Alerting

`axon-qdrant` uses `restart: unless-stopped` and recovers automatically from OOM kills, but the restart event is silent unless you wire alerting. With ~4.5M+ points and ~15–16 GiB memory capped, OOM risk is real — especially during scroll-heavy operations like `dedupe` or `sources`.

### Alert via LoggiFly + Apprise/Gotify

If your stack includes [LoggiFly](https://github.com/nicklan/loggifly) and Apprise or Gotify:

1. **Add `axon-qdrant` to your LoggiFly watch config** (`~/.lab/config.toml` or equivalent):

   ```toml
   [[containers]]
   name = "axon-qdrant"
   # Alert on OOM kills and crash restarts
   patterns = [
     "SIGSEGV",
     "exiting",
     "killed",
     "OOM",
   ]
   apprise_url = "gotify://your-gotify-host/your-token"
   ```

2. **Restart LoggiFly** to pick up the new container:
   ```bash
   docker compose restart loggifly
   ```

### Alert via Docker health + Apprise directly

If you don't run LoggiFly, use a cron job that watches the container restart count:

```bash
# ~/.axon/scripts/check-qdrant.sh
#!/usr/bin/env bash
COUNT=$(docker inspect axon-qdrant --format '{{.RestartCount}}' 2>/dev/null || echo 0)
LAST=$(cat ~/.axon/.qdrant-restart-count 2>/dev/null || echo 0)
if [[ "$COUNT" -gt "$LAST" ]]; then
    curl -s "https://your-apprise-url/notify" \
        -d "body=axon-qdrant restarted (count=${COUNT}) — possible OOM kill" \
        -d "tag=homelab"
fi
echo "$COUNT" > ~/.axon/.qdrant-restart-count
```

Add to crontab: `*/5 * * * * /home/user/.axon/scripts/check-qdrant.sh`

### Levers to reduce OOM pressure

| Action | Effect |
|--------|--------|
| `axon dedupe` off-peak | Dedup scroll loads large point slices — run outside business hours |
| Reduce `mem_limit` in `docker-compose.prod.yaml` increments of 512m | Forces earlier OOM before host pressure |
| `axon sources --limit 1000` instead of full facet | Lower memory than `axon sources` with no limit |
| Prune stale collections | `curl -X DELETE ${QDRANT_URL}/collections/<name>` frees allocated segments |

See also: `~/.axon/memory/qdrant-oom-crashloop.md` for historical OOM context on this deployment.

---

## Backup and Restore

Axon's knowledge base and job state live in two places:

| Data | Location | Backup tool |
|------|----------|-------------|
| Vector corpus | `axon-qdrant` container volume | Qdrant snapshot API |
| Job queue + history | `~/.axon/jobs.db` | SQLite `.backup` |

Use `scripts/axon-backup.sh` for both in a single operation:

```bash
./scripts/axon-backup.sh                        # interactive
./scripts/axon-backup.sh --yes                  # non-interactive (cron-safe)
./scripts/axon-backup.sh --collection my_col    # specific collection
./scripts/axon-backup.sh --output-dir /mnt/nas/axon-backups
```

The script creates timestamped archives in `~/.axon/backups/` (override with `--output-dir` or `AXON_BACKUP_DIR`), prints SHA-256 checksums, and cleans up the server-side Qdrant snapshot. On ZFS hosts that replicate to a backup box (e.g. `shart`), the backup directory is automatically replicated — no separate transfer step.

**Restore:**

```bash
# Qdrant — stop axon first, then:
curl -X POST "${QDRANT_URL}/collections/axon/snapshots/recover" \
  -H "Content-Type: application/json" \
  -d "{\"location\": \"file:///path/to/axon-20260101-0200.tar.gz\"}"

# SQLite — stop workers first, then:
cp ~/.axon/backups/sqlite/jobs-20260101-0200.db ~/.axon/jobs.db
```

---

## Source map

| Path | Purpose |
|------|---------|
| `docker-compose.prod.yaml` | Axon, local TEI/Chrome services, and optional local Qdrant service |
| `Justfile` | `services-up`, `services-down`, `stop`, `dev` |
| `scripts/axon` | Wrapper that auto-sources `~/.axon/.env` and runs `cargo run --bin axon` |
| `scripts/dev-setup.sh` | First-run bootstrap |
| `src/cli/commands/doctor.rs` | `axon doctor` entry point |
| `src/core/health/doctor/sqlite.rs` | SQLite-runtime doctor probe |
| `src/cli/commands/status.rs` | `axon status` entry point |
| `src/cli/commands/crawl/subcommands.rs` | `crawl status/cancel/errors/list/cleanup/clear/recover` |
| `src/cli/commands/migrate.rs` | `axon migrate` |
| `src/jobs/ops/enqueue.rs` | Queue caps, `AXON_MAX_PENDING_*` |
| `src/jobs/store.rs` | SQLite schema bootstrap + lifecycle SQL |
| `crates/axon-core/src/logging.rs` | `init_tracing`, `AXON_LOG_PATH`, size-based rotation (`AXON_LOG_MAX_BYTES`, `AXON_LOG_MAX_FILES`) |
| `src/core/logging/size_rotating.rs` | `SizeRotatingFile`: byte-budget rotation writer |
| `src/core/paths.rs` | `axon_data_dir()`, `axon_data_base_dir()` |
