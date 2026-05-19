# Session: axon doctor Improvements
**Date:** 2026-02-23
**Branch:** fix-crawl
**Author:** Claude (claude-sonnet-4-6)

---

## Session Overview

Identified and implemented six improvements to the `axon doctor` command, then tightened three follow-up issues in subsequent review passes. The command now probes all five pipelines (including previously missing ingest), live-tests the OpenAI endpoint, probes the Chrome CDP endpoint, shows TEI and Qdrant URLs, displays resolved queue names per pipeline, warns on stale/pending job backlog, and uses neutral symbols for optional unconfigured services instead of failure symbols.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | User ran `axon doctor`, asked for suggestions |
| +5m | Identified 7 gaps: missing ingest probe, missing Chrome probe, OpenAI not live-tested, TEI URL not shown, queue names not shown, no stale job warning, all_ok excluded OpenAI |
| +20m | Implemented all 7: `ingest_doctor()`, Chrome probe, OpenAI live probe, TEI URL, queue names, stale job query, extract LLM degraded state |
| +5m | First tightening pass: removed `query_stale_jobs` wrapper, fixed pipeline render bug (extract rendered twice), fixed optional services showing ✗ when unconfigured |
| +5m | Second tightening pass: OpenAI now optional-styled, `ingest_doctor` probes parallelized, confirmed chrome label logic clean |
| End | `cargo check` + `cargo clippy` both clean, 0 warnings |

---

## Key Findings

- **`ingest_doctor()` was missing entirely** — `ingest_jobs.rs` had no doctor function; the ingest pipeline was invisible to `axon doctor`
- **OpenAI probe was config-check only** — `openai_state()` only checked env vars; a misconfigured `OPENAI_BASE_URL` (wrong host/path) would pass as `configured` and only fail silently at runtime during `ask`/`extract`
- **Chrome CDP endpoint unprobed** — `AXON_CHROME_REMOTE_URL` was read by the crawl engine but never checked by doctor; a dead Chrome endpoint caused silent HTTP fallback with no warning
- **Pipeline render bug** — `render_pipelines_section` iterated `["crawl", "batch", "embed", "ingest"]` (skipping extract) then rendered extract again manually below with duplicated queue-label code; extract appeared twice in human output
- **Optional services showed ✗** — unconfigured webdriver/chrome/openai all went through `status_from_bool` → `"failed"` → red ✗ symbol, misleadingly implying failures

---

## Technical Decisions

- **OpenAI probe hits `/models` not `/chat/completions`** — `/models` is a lightweight GET with no request body; `/chat/completions` would require constructing a fake request and could incur LLM cost. 3s timeout keeps doctor fast.
- **Chrome probe hits `/json/version`** — canonical CDP health endpoint per Chrome DevTools Protocol spec; falls back to `/json` if that fails.
- **Stale job SQL lives in `common.rs` not `doctor.rs`** — keeps SQL in the jobs layer where sqlx is already imported; doctor.rs stays at the CLI layer without a direct sqlx dependency.
- **`extract_llm_ready` vs `extract` are separate flags** — infra health (pg/amqp/redis) and LLM readiness are orthogonal; `all_ok` only gates on infra so an unconfigured OpenAI doesn't fail a crawl-only deployment. The warning line in Pipelines makes the degraded state visible without blocking overall health.
- **`render_optional_status_line`** — single helper for webdriver/chrome/openai; `·` when unconfigured, ✗/✓ when configured. Avoids duplicating the logic three times.
- **`ingest_doctor` parallelized with `tokio::join!`** — consistent with all other pipeline doctors; no reason for sequential probes when pg/amqp/redis are independent.

---

## Files Modified

| File | Purpose |
|------|---------|
| `crates/jobs/ingest_jobs.rs` | Added `ingest_doctor()` with parallel `tokio::join!` probes for pg/amqp/redis |
| `crates/jobs/common.rs` | Added `count_stale_and_pending_jobs(cfg, stale_minutes)` — queries all 5 job tables for stuck-running and pending counts |
| `crates/cli/commands/doctor.rs` | Major rewrite: ingest pipeline, Chrome probe, OpenAI live probe, TEI URL display, queue names, stale job warning, optional service symbols, pipeline render fix |

---

## Commands Executed

```
cargo check --bin axon   → Finished (0 errors, 0 warnings) — 3 runs
cargo clippy --bin axon  → Finished (0 warnings) — 2 runs
```

---

## Behavior Changes (Before/After)

### Services section
**Before:**
```
Services
  ✓ postgres completed postgresql://***:***@127.0.0.1:53432/axon
  ✓ redis completed redis://***:***@127.0.0.1:53379
  ✓ amqp completed amqp://***:***@127.0.0.1:45535
  ✓ tei completed http 200
    model: Qwen/Qwen3-Embedding-0.6B
    info: model_sha=null, ...
  ✓ qdrant completed http 200
  ✗ webdriver failed not configured (optional fallback)
  ✓ openai completed configured
```

**After:**
```
Services
  ✓ postgres completed postgresql://***:***@127.0.0.1:53432/axon
  ✓ redis completed redis://***:***@127.0.0.1:53379
  ✓ amqp completed amqp://***:***@127.0.0.1:45535
  ✓ tei completed http 200
    url: http://host:52000
    model: Qwen/Qwen3-Embedding-0.6B
    info: model_sha=null, ...
  ✓ qdrant completed http://127.0.0.1:53333
  · webdriver not configured (optional fallback)
  · chrome not configured (optional)
  · openai not configured          ← or ✓/✗ with live probe result
```

### Pipelines section
**Before:**
```
Pipelines
  ✓ crawl completed
  ✓ batch completed
  ✓ extract completed
  ✓ embed completed
```

**After:**
```
Pipelines
  ✓ crawl completed (axon.crawl.jobs)
  ✓ batch completed (axon.batch.jobs)
  ✓ extract completed (axon.extract.jobs)
    ⚠ openai not configured — extract jobs will fail at LLM step   ← only if unconfigured
  ✓ embed completed (axon.embed.jobs)
  ✓ ingest completed (axon.ingest.jobs)
```

### Job Backlog section (new, only appears when non-zero)
```
Job Backlog
  ✗ 2 job(s) stuck in running >15 min — consider `axon crawl recover`
  · 5 job(s) pending — are workers running?
```

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | 0 errors, 0 warnings | 0 errors, 0 warnings | ✓ PASS |
| `cargo clippy --bin axon` | 0 warnings | 0 warnings | ✓ PASS |
| `cargo check --bin axon` (post-tightening) | 0 errors, 0 warnings | 0 errors, 0 warnings | ✓ PASS |
| `cargo clippy --bin axon` (post-tightening) | 0 warnings | 0 warnings | ✓ PASS |

---

## Source IDs + Collections Touched

None — no embed/retrieve operations performed this session.

---

## Risks and Rollback

- **OpenAI probe adds ~3s latency to doctor when OpenAI is configured but slow** — timeout is hardcoded at 3s; acceptable for a diagnostic command. Rollback: reduce timeout or gate the probe behind a flag.
- **`count_stale_and_pending_jobs` runs 10 SQL queries** (2 per table × 5 tables) — sequential within `gather_doctor_probes` which is itself inside `tokio::join!`. At worst adds a few ms. Rollback: remove `stale_jobs` from probes struct.
- **`all_ok` now requires ingest pipeline** — if ingest AMQP/pg is down, `all_ok` = false. Previously doctor didn't check ingest at all. Deployments without ingest configured will now show overall failed. Rollback: remove ingest from `report_overall_ok`.

---

## Decisions Not Taken

- **Qdrant collection count in doctor** — would require authenticated Qdrant `/collections` API call; adds latency and complexity for marginal signal. Skipped.
- **TEI embedding dimension check** — would require parsing TEI `/info` response more deeply; dimension is in the `info` field already shown. Skipped.
- **RabbitMQ queue depth via management API** — would require separate HTTP probe to management port (15672); already have AMQP connectivity check which is sufficient. Skipped.
- **`AXON_DOCTOR_STALE_MINUTES` env var** — stale threshold is a diagnostic heuristic, not a business rule; env var would just be noise. Hardcoded at 15 min.
- **Making `all_ok` include OpenAI** — OpenAI is optional for crawl/scrape/map/query deployments; including it would break `all_ok` for valid partial deployments. Used `extract_llm_ready` flag instead.

---

## Open Questions

- Does the Chrome probe (`/json/version`) work correctly against the `axon-chrome` management port (6000) vs the CDP proxy port (9222)? The probe uses `cfg.chrome_remote_url` which is set to the management URL — unverified in this session.
- Should `all_ok` exclude ingest for deployments where `AXON_INGEST_QUEUE` is intentionally not configured? Currently any AMQP failure on the ingest queue fails overall health.

---

## Next Steps

- Run `axon doctor` in a live environment to validate the new output format end-to-end
- Consider whether Chrome probe should also verify CDP WebSocket connectivity (not just HTTP reachability of management port)
