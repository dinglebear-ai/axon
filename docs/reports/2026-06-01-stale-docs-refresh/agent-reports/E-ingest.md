# Agent E — ingest (command docs + ingest guides + watch/sessions) report

## Files reviewed
- docs/commands/ingest.md — minor fixes (collection default, `--include-source` no-op)
- docs/commands/github.md — accurate (removed-command redirect stub; correct)
- docs/commands/reddit.md — minor fix (collection default)
- docs/commands/youtube.md — minor fix (collection default)
- docs/commands/sessions.md — accurate (verified flags `--claude/--codex/--gemini/--project`, server-mode, env vars)
- docs/commands/watch.md — **major fixes** (interval bounds, refresh semantics, missing auto-fire scheduler)
- docs/ingest/github.md — minor fix (collection default); file-fetch mechanism (git clone) verified correct
- docs/ingest/gitlab.md — accurate (matches source; shared `--no-source`/`--include-source` framing correct)
- docs/ingest/ingest.md — minor fix (added `sessions` to source_type list)
- docs/ingest/reddit.md — accurate (defaults, OAuth flow, metadata fields all verified)
- docs/ingest/sessions.md — minor fix (stale dev-guide flag name)
- docs/ingest/youtube.md — accurate (sequential playlist loop + `--playlist-end 500` verified against source)

## Fixes made

**docs/commands/ingest.md**
- `--collection` default `cortex` → `axon` (verified: `src/core/config/types_tests.rs:63` asserts `cfg.collection == "axon"`; `types/config.rs` default).
- `--include-source` reworded: it is a **no-op**, not "redundant but functional." Verified `src/core/config/parse/build_config/command_dispatch.rs:333-337` — only `--no-source` mutates `github_include_source`; `--include-source` is explicitly a no-op (code comment: "`--include-source` is now a no-op").
- `--no-source` clarified to apply to **all Git providers**, not just GitHub. Verified `classify_target(&target, cfg.github_include_source)` (`src/cli/commands/ingest.rs:63`) passes the same flag into `IngestSource::{Github,Gitlab,Gitea,GenericGit}`, each of which carries `include_source: bool` (`src/jobs/ingest/types.rs:10-25`). The CLI `--help` text labels these "(GitHub only)" but that help string is wrong — the code applies the flag to every git source, so the doc's broader framing is kept.

**docs/commands/reddit.md / youtube.md / ingest/github.md**
- `--collection` / `AXON_COLLECTION` default `cortex` → `axon`.

**docs/ingest/ingest.md**
- Added `sessions` to the `source_type` enum list in the storage-schema row (`sessions` is a valid `IngestSource` variant / `axon_ingest_jobs.source_type` value).

**docs/ingest/sessions.md**
- Dev-guide step 4 referenced a nonexistent `--sessions-<provider>` flag → corrected to `--<provider>` (`--claude`/`--codex`/`--gemini`) in `SessionsArgs`. Verified `src/core/config/cli.rs:525-543`.

**docs/commands/watch.md (major)**
- Interval bound "must be >= 1" → "between `30` and `604800` (7 days)". Verified `MIN_WATCH_INTERVAL_SECS=30`, `MAX_WATCH_INTERVAL_SECS=604800`, `validate_every_seconds()` in `src/jobs/watch.rs:38-54`.
- `--task-type` reworded: `refresh` is the only **supported** type, others rejected at create. Verified `SUPPORTED_TASK_TYPES = ["refresh"]` + `validate_task_type()` (`src/jobs/watch.rs:16-34`).
- Refresh task-payload semantics corrected: a refresh run **scrapes each URL inline** (via `services::scrape::scrape`) — it does NOT dispatch a downstream job — and **fails** (status `failed`, run-now errors) when `urls` is empty/missing. The old text ("records a run but does not dispatch a downstream refresh job") was misleading. Verified `run_watch_task()` (`src/jobs/watch.rs:502-538`).
- Added a new **Automatic Firing** section + scheduler env-var table — the doc previously framed watches as manual-only. Verified `lease_due_watches()` (`src/jobs/watch.rs:251-278`) and the in-process loop `src/jobs/workers/watch_scheduler.rs` (spawned by `spawn_workers`, active under `serve`/`mcp`), advancing `next_run_at` by `every_seconds` each `AXON_WATCH_TICK_SECS` (default 15) with `AXON_WATCH_LEASE_SECS` (default 300) lease TTL.
- Added a Note that `create` always makes an **enabled** watch, and since `pause`/`resume`/`delete`/`update` are unimplemented, there is no CLI path to disable/remove a watch. Verified `handle_watch_create()` sets `enabled: true` (`src/cli/commands/watch.rs`) and the four stubs return "not yet implemented" errors.

## Verified-accurate (no change needed)
- Watch implemented vs stub split: `create`/`list`/`run-now`/`history` work; `get`/`update`/`pause`/`resume`/`delete`/`artifacts` parse but return "not yet implemented." Confirmed in `src/cli/commands/watch.rs` (explicit error arms) and `src/services/watch.rs` (only CRUD shims for the implemented set). `artifacts` correctly takes `<run_id>` (not `<id>`).
- `AXON_SERVER_URL` server-mode + `--local` flag in ingest.md/sessions.md are real. `--local` is a **global** flag (`src/core/config/cli/global_args.rs:214-223`, env `AXON_LOCAL_MODE`) — it doesn't appear in per-command `--help` dumps because globals aren't expanded there, but it exists.
- Reddit flag defaults (sort=hot, time=day, max-posts=25, min-score=0, depth=2) — all match `src/core/config/cli.rs:505-523`.
- GitHub file ingest uses `git clone --depth=1` (`src/ingest/github/files.rs` → `clone_repo`), matching github.md. (Note: `src/ingest/CLAUDE.md` still claims the old reqwest tree-fetch — see report-only below.)
- YouTube playlist processing is **sequential** (`for` loop, `src/ingest/youtube.rs:410`) with `MAX_PLAYLIST_VIDEOS=500`, matching youtube.md.
- Supported source types github/gitlab/gitea/git/reddit/youtube + sessions — all confirmed against `IngestSource` enum and `axon ingest --help`.

## Gaps / missing docs (for Phase 2)
- **No `docs/ingest/gitea.md` or generic-git deep-dive.** ingest/ingest.md routes Gitea/Forgejo and generic Git to "see commands/ingest.md" instead of a deep-dive. `src/ingest/CLAUDE.md` has solid Gitea + generic-git material (target forms, 403/404 degradation, payload schema) that would justify real deep-dives — currently these sources have CLI coverage but no implementation/troubleshooting doc to match github.md/gitlab.md.
- **No `docs/commands/gitlab.md` / `gitea.md`** command stubs paralleling github.md/reddit.md/youtube.md "removed — use ingest" redirects. Not strictly needed (they were never standalone commands), but the per-source command-doc set is asymmetric.

## Reorg observations (for Phase 2)
- **REPORT-ONLY, out of lane:** `src/ingest/CLAUDE.md` is stale on two points and should be fixed in Phase 2: (1) GitHub section claims "raw reqwest for file content fetching… files fetched tree-first (one API call)" — the code now uses `git clone --depth=1` (`files/clone.rs`); (2) YouTube section claims "N=5 concurrent via FuturesUnordered" — the playlist loop is a sequential `for`. Not one of my 12 files; flagged here only.
- docs/ingest/reddit.md and docs/ingest/sessions.md carry stray `Version: 1.0.0 / Last Updated: 01:26:53 | 02/25/2026 EST` header blocks (cosmetic, inconsistent with every other doc's single `Last Modified:` line). Candidate for normalization in reorg.
- github.md is the only ingest deep-dive with full Qdrant metadata-field tables; gitlab.md/reddit.md/youtube.md vary in depth. A consistent per-source template (What Gets Indexed → How It Works → Metadata Fields → Limitations → Troubleshooting) would help.
- ingest/youtube.md back-links to `commands/youtube.md` (a removed-command redirect stub) rather than `commands/ingest.md`; the other ingest deep-dives back-link to `commands/ingest.md`. Inconsistent target.

## Cross-reference notes
Links FROM my docs TO other docs:
- commands/ingest.md → ingest/{ingest,github,gitlab,reddit,youtube}.md
- commands/github.md → commands/ingest.md, ingest/github.md
- commands/reddit.md/youtube.md → commands/ingest.md, ingest/{reddit,youtube}.md
- commands/sessions.md → ingest/sessions.md
- ingest/ingest.md → ingest/{github,gitlab,reddit,youtube,sessions}.md and commands/ingest.md
- ingest/{github,reddit,sessions}.md → commands/ingest.md (or commands/sessions.md)
- ingest/youtube.md → **commands/youtube.md** (a removed-command stub — see reorg note above)

Code→doc path references noticed (all verified current): `src/jobs/watch.rs`, `src/jobs/workers/watch_scheduler.rs`, `src/ingest/github/meta.rs`, `src/vector/ops/input/{code,classify}.rs`, `src/ingest/reddit/meta.rs`, `src/jobs/migrations/` — all exist.
