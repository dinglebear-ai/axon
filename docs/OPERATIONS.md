# Operations Runbook
Last Modified: 2026-05-06

Operator runbook for Axon (lite-mode, SQLite-backed). Task-oriented: one heading per
operational task. Every command in this file is verified against the current
codebase — see the cited source paths in each section.

> **Lite mode is the only supported runtime.** All jobs persist in SQLite and
> all workers run in-process inside the `axon mcp` (or `axon serve`) tokio
> runtime. The only required external services are **Qdrant** and **TEI**;
> **Chrome** is required for `--render-mode chrome` / `auto-switch` and for
> `screenshot`. There is no Postgres, Redis, or AMQP/RabbitMQ.

## Related docs

- [`PERFORMANCE.md`](PERFORMANCE.md) — concurrency profiles, watchdog tuning, retrieval knobs
- [`SECURITY.md`](SECURITY.md) — SSRF, allowlist, secrets handling
- [`JOB-LIFECYCLE.md`](JOB-LIFECYCLE.md) — pending/running/completed/failed/canceled state machine
- [`DEPLOYMENT.md`](DEPLOYMENT.md) — production deployment + env reference
- [`CONFIG.md`](CONFIG.md) — every env var
- [`MCP.md`](MCP.md) — MCP server runtime

---

## Day 0 — Bootstrap

```bash
./scripts/dev-setup.sh
```

The script installs system tools + Rust toolchain, copies `.env.example` →
`.env`, prompts for `AXON_DATA_DIR`, pre-creates persistent directories, and
brings infra up.

Manual equivalent:

```bash
cp .env.example .env                    # then edit
mkdir -p "${AXON_DATA_DIR:-$HOME/.axon}"/{qdrant,output,tei}
just services-up
```

Required values in `.env`:

| Var | Purpose |
|-----|---------|
| `AXON_DATA_DIR` | Host root for SQLite, Qdrant volume, TEI cache, output |
| `QDRANT_URL` | Default: `http://127.0.0.1:53333` |
| `TEI_URL` | Default: `http://127.0.0.1:52000` |
| `AXON_HEADLESS_GEMINI_CMD` | Gemini CLI command — required for `ask`/`evaluate`/`research`/`debug`/`suggest`/extract fallback |
| `OPENAI_BASE_URL` / `OPENAI_API_KEY` / `OPENAI_MODEL` | LLM endpoint used as model override |
| `TAVILY_API_KEY` | Required for `search` and `research` |
| `REDDIT_CLIENT_ID` / `REDDIT_CLIENT_SECRET` | Required for Reddit ingest |

---

## Start services

Infrastructure (Qdrant + TEI + Chrome) only:

```bash
just services-up
# equivalent to:
docker compose -f config/docker-compose.services.yaml up -d
```

Foreground dev loop (builds binary, starts infra, runs `axon mcp` in-process):

```bash
just dev
```

`just dev` requires the binary to build cleanly — if `cargo build` fails, no
processes are started. Workers spawn inside the `axon mcp` runtime via
`LiteBackend::new_with_workers`; CLI fire-and-forget submissions are
processed by that running `axon mcp` (or `axon serve`).

> If you submit `--wait false` jobs **without** an `axon mcp` / `axon serve`
> process running, the jobs are persisted to SQLite and stay `pending`
> indefinitely. Either keep `axon mcp` running or pass `--wait true`.

---

## Health checks

```bash
./scripts/axon doctor
./scripts/axon status
```

`axon doctor` (lite-mode probe at `crates/core/health/doctor/lite.rs`) reports:

- **SQLite** — file exists at `cfg.sqlite_path`
- **TEI** — `GET /health`, plus `/info` for embedding model + summary
- **Qdrant** — `GET /healthz`, plus `/collections/{name}` for vector mode (named/unnamed)
- **Chrome** — `chrome_remote_url` if configured
- **Gemini headless LLM** — base URL + model reachability
- **Vector mode mismatch** — warns if collection is unnamed but `AXON_HYBRID_SEARCH=true`

`axon status` reports per-kind job counts (Crawl / Extract / Embed / Ingest)
and recent jobs (top 10).

---

## Submit work

Synchronous (block until done):

```bash
./scripts/axon scrape https://example.com --wait true
./scripts/axon embed docs/ARCHITECTURE.md --wait true
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

> **Stale reclaim is startup-only.** A periodic watchdog is not implemented —
> the reclaim runs when `LiteBackend` is created (i.e. when `axon mcp` /
> `axon serve` boots). To force a reclaim mid-run:

```bash
./scripts/axon crawl recover
./scripts/axon extract recover
./scripts/axon embed recover
./scripts/axon ingest recover
```

This re-queues stale jobs as `pending`. Implemented in
`crates/cli/commands/crawl/subcommands.rs:60` and the equivalent
ingest/extract/embed handlers via `services::jobs::recover_jobs`.

### Process alive but job hung

In-process workers track per-job heartbeats. When `result_json` does not
advance for 6 × 30s = 3 min the worker logs a warning; at 20 × 30s = 10 min
the worker kills the job and marks it `failed`. No operator action is
required — see `crates/jobs/CLAUDE.md` "Liveness Enforcement (Two Tiers)".

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
unless `--yes` is set or stdout is not a TTY (see `crates/core/ui.rs::confirm_destructive`).

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
`ensure_schema()` in each `crates/jobs/lite/store.rs`-driven worker — there
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

Qdrant data is persisted at the host bind mount declared in
`config/docker-compose.services.yaml:36`:

```
${AXON_DATA_DIR}/qdrant          # default: ~/.axon/qdrant
```

Two options:

### Snapshot via Qdrant API (preferred)

```bash
# Create a named snapshot (runs while Qdrant is up)
curl -s -X POST "${QDRANT_URL}/collections/${AXON_COLLECTION:-cortex}/snapshots"

# List
curl -s "${QDRANT_URL}/collections/${AXON_COLLECTION:-cortex}/snapshots"

# Download (path returned by the create call lives under /qdrant/snapshots)
docker compose -f config/docker-compose.services.yaml \
    exec axon-qdrant ls /qdrant/snapshots
docker cp axon-qdrant:/qdrant/snapshots/<file> ./backup/
```

Restore: copy the snapshot back into the container and POST to
`/collections/{name}/snapshots/recover` with the snapshot location. See
[Qdrant docs](https://qdrant.tech/documentation/concepts/snapshots/).

### Volume-level cold backup

Stop Qdrant, then archive the bind mount:

```bash
just services-down
tar -czf qdrant-backup.tgz -C "${AXON_DATA_DIR}/axon" qdrant/
just services-up
```

This is faster for one-shot full backups and includes index state.

---

## Logs and diagnostics

### Application logs (Rust)

`init_tracing()` in `crates/core/logging.rs` writes to two sinks:

- **stderr**, default level `WARN` (overridable with `RUST_LOG=info`)
- **size-rotated JSON file** at `${AXON_LOG_DIR}/${AXON_LOG_FILE}` (defaults
  to `${AXON_DATA_DIR}/logs/axon.log` → `~/.axon/logs/axon.log`), `INFO` level. Rotation triggers
  when the active file exceeds `AXON_LOG_MAX_BYTES` (default 10 MiB);
  archives are renamed `<file>.1`, `<file>.2`, … up to `AXON_LOG_MAX_FILES`
  (default `3`). The oldest archive is pruned on each rotation.

```bash
# tail the active log
tail -f "${AXON_DATA_DIR:-$HOME/.axon}/logs/axon.log"

# noisier output for one run
RUST_LOG=info,crates::jobs::lite=debug just dev
```

`tracing` filters honor `RUST_LOG`. CDP decoder noise is suppressed by
`init_tracing`.

### Container logs

```bash
docker compose -f config/docker-compose.services.yaml logs -f axon-qdrant axon-tei axon-chrome
```

Container logs are JSON-formatted, capped at 10 MB × 3 files (see
`config/docker-compose.services.yaml:21-25`).

### Chrome diagnostics

When debugging Chrome/CDP issues:

```bash
AXON_CHROME_DIAGNOSTICS=1 \
AXON_CHROME_DIAGNOSTICS_DIR=.cache/chrome-diagnostics \
./scripts/axon crawl https://example.com --render-mode chrome --wait true
```

Enables screenshot + event capture. See `crates/core/health.rs`.

---

## Performance tuning

Pick a profile per-command:

```bash
./scripts/axon crawl https://docs.rs/spider --performance-profile extreme --wait true
```

Profiles: `high-stable` (default), `balanced`, `extreme`, `max`. Granular
overrides: `--crawl-concurrency-limit`, `--batch-concurrency`,
`--request-timeout-ms`, `--fetch-retries`, `--retry-backoff-ms`.

For the full performance model see [`PERFORMANCE.md`](PERFORMANCE.md).

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
| `AXON_CRAWL_SIZE_WARN_THRESHOLD` | 10000 | After uncapped crawl, warn if pages > N (`0` disables) |

Implemented in `crates/jobs/lite/ops/enqueue.rs`.

---

## Reindex a Qdrant collection

When the embedding model, chunk strategy, or payload schema changes you must
rebuild from source.

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
./scripts/axon embed https://example.com/page --wait true   # re-embeds; predelete on
```

`AXON_EMBED_STRICT_PREDELETE=true` (default) deletes existing points for the
URL before upsert.

---

## Migrate to named-vector hybrid search

Older collections store an unnamed dense vector. Hybrid Reciprocal-Rank-Fusion
search requires named `dense` + `bm42` sparse vectors. Migrate with:

```bash
./scripts/axon migrate --from cortex --to cortex_v2
```

The migrate command (handler at `crates/cli/commands/migrate.rs:11`) scrolls
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
| `axon doctor` shows `tei.ok=false` | TEI container down or model still loading | `docker compose -f config/docker-compose.services.yaml ps`; wait for healthcheck (`start_period: 20s`); check `docker logs axon-tei` for CUDA OOM |
| `tei` returns `503` mid-run | Model overload / CUDA pressure | Reduce `TEI_MAX_BATCH_TOKENS` / `TEI_MAX_CONCURRENT_REQUESTS` in `services.env`; lower `--batch-concurrency`; TEI client auto-retries on 429/5xx (see CLAUDE.md "TEI retries") |
| `qdrant connection refused` | Qdrant not started or `QDRANT_URL` wrong | `just services-up`; verify `curl ${QDRANT_URL}/healthz` |
| `queue cap exceeded` on submit | `AXON_MAX_PENDING_*` reached | Run `axon <kind> list` to inspect; `axon <kind> cleanup` removes terminal rows; raise the cap or set to `0` |
| Jobs sit `pending` forever | No `axon mcp` / `axon serve` process running | Start `just dev` or pass `--wait true` |
| Job stuck `running` past 10 min | Worker hang | Heartbeat watchdog will mark `failed` automatically; or run `axon <kind> recover` |
| Hybrid search returns dense-only results | Collection is in legacy unnamed mode | `axon doctor` will surface `mode_mismatch_warning`; run `axon migrate --from <old> --to <new>` and restart |
| Most pages flagged as thin | Site is JS-rendered | `--render-mode chrome` or `auto-switch`; do NOT change `readability: false` in `crates/core/content.rs` (confirmed regression) |
| `Chrome` probe fails in doctor | `axon-chrome` container down | `docker compose -f config/docker-compose.services.yaml restart axon-chrome` |
| `Pulse "Claude CLI exited 1"` (web UI) | Root-owned `~/.claude` from prior container run | Remove root-owned files in the data dir; see project memory note on cont-init script |
| Streaming `ask` panics on multibyte char | Out-of-date binary | Pull `main`, rebuild — `crates/vector/ops/commands/ask/context/retrieval.rs` uses `.get(i..)` |

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

Volumes (`${AXON_DATA_DIR}/{qdrant,tei,...}`, default `~/.axon/{qdrant,tei,...}`) are bind-mounted on the
host and persist across restarts.

---

## Source map

| Path | Purpose |
|------|---------|
| `config/docker-compose.services.yaml` | Infra services (qdrant, tei, chrome) |
| `Justfile` | `services-up`, `services-down`, `stop`, `dev` |
| `scripts/axon` | Wrapper that auto-sources `.env` and runs `cargo run --bin axon` |
| `scripts/dev-setup.sh` | First-run bootstrap |
| `crates/cli/commands/doctor.rs` | `axon doctor` entry point |
| `crates/core/health/doctor/lite.rs` | Lite-mode doctor probe |
| `crates/cli/commands/status.rs` | `axon status` entry point |
| `crates/cli/commands/crawl/subcommands.rs` | `crawl status/cancel/errors/list/cleanup/clear/recover` |
| `crates/cli/commands/migrate.rs` | `axon migrate` |
| `crates/jobs/lite/ops/enqueue.rs` | Queue caps, `AXON_MAX_PENDING_*` |
| `crates/jobs/lite/store.rs` | SQLite schema bootstrap + lifecycle SQL |
| `crates/core/logging.rs` | `init_tracing`, `AXON_LOG_DIR`/`AXON_LOG_FILE`, size-based rotation (`AXON_LOG_MAX_BYTES`, `AXON_LOG_MAX_FILES`) |
| `crates/core/logging/size_rotating.rs` | `SizeRotatingFile`: byte-budget rotation writer |
| `crates/core/paths.rs` | `axon_data_dir()`, `axon_data_base_dir()` |
