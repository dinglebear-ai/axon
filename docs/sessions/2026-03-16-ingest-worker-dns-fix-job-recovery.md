# Session: Ingest Worker DNS Fix + Job Recovery
Date: 2026-03-16
Branch: feat/pulse-shell-and-hybrid-search

## Session Overview

Debugged and resolved a cascade of ingest worker failures: stale release binary (missing startup sweep function), tree-sitter blocking the tokio runtime, and finally a Tailscale MagicDNS outage causing `client error (Connect)` on all GitHub API calls. All 38 ingest jobs (which had failed across three separate root causes) are now processing successfully.

---

## Timeline

| Time (UTC) | Event |
|------------|-------|
| Session start | 36 ingest jobs in `failed` state — carried over from previous session |
| ~18:05 | `axon ingest recover` returns 0 — only reclaims stale *running* jobs, not pending |
| ~18:06 | Reset 36 failed jobs to `pending` via psql UPDATE |
| ~18:07 | Identified release binary built 2026-03-12; `worker_lane.rs` modified 2026-03-15 — `reenqueue_orphaned_pending_jobs` function missing from binary |
| ~18:08 | Debug worker killed; `cargo build --release --bin axon` started |
| ~18:15 | Build complete (v0.25.3) — includes `spawn_blocking` fix for tree-sitter |
| ~18:12 | Worker restarted; startup log: `re-enqueued 38 orphaned pending job(s)` |
| ~18:12 | All 6 lanes started, jobs began running |
| ~18:09 | All jobs fail: `Service Error: client error (Connect)` — ~20s after start |
| ~18:10 | Diagnosed: Tailscale MagicDNS (100.100.100.100) down; `~.` catch-all eating all external DNS |
| ~18:10 | Fix: `resolvectl domain tailscale0 manatee-triceratops.ts.net` — removed `~.` catch-all |
| ~18:11 | DNS confirmed fixed: `api.github.com` resolves via enp8s0/1.1.1.1 |
| ~18:11 | Reset 38 DNS-failed jobs back to `pending` via psql UPDATE |
| ~18:12 | Worker restarted; all 38 re-enqueued; jobs now running and hitting TEI successfully |
| ~18:37 | Status check: 6 running, 14 pending, 328 completed — all healthy |

---

## Key Findings

### 1. Stale Binary Root Cause
- Release binary (`target/release/axon`) was built **2026-03-12**
- `crates/jobs/worker_lane.rs` was **last modified 2026-03-15**
- `reenqueue_orphaned_pending_jobs` function (lines 302–326) did not exist in the running binary
- `Ok(0) => {}` in the caller produces **no log output** — looked identical to "found 0 orphaned jobs"
- **Detection**: Binary timestamp vs file mtime mismatch

### 2. spawn_blocking Fix (compiled into v0.25.3)
- `crates/ingest/github/files.rs:194–199` — `chunk_code` + `chunk_text` wrapped in `tokio::task::spawn_blocking`
- Tree-sitter is CPU-bound; running it on the async runtime starved other tasks and caused watchdog timeouts
- Returns `(chunks, text)` tuple so `text` ownership is preserved for line-range computation

### 3. Tailscale MagicDNS Outage
- `tailscale0` link had `DNS Domain: manatee-triceratops.ts.net lan ~.` (catch-all `~.`)
- `100.100.100.100` (MagicDNS) was down — all DNS queries timed out
- `dig @1.1.1.1 api.github.com` worked fine — upstream DNS healthy
- `resolvectl query api.github.com` failed: `All attempts to contact name servers or networks failed`
- TEI (Tailscale IP `100.74.16.82:52000`) was reachable because it uses IP directly, not DNS

### 4. axon ingest recover vs orphaned pending
- `axon ingest recover` → `reclaim_stale_running_jobs` — only reclaims jobs stuck in `running` state
- Does **not** re-enqueue orphaned `pending` jobs that have no AMQP message
- The `reenqueue_orphaned_pending_jobs` startup sweep is the correct mechanism for those

---

## Technical Decisions

- **Scoped Tailscale DNS instead of disabling it**: Changed `tailscale0` domain scope from `~.` (catch-all) to `manatee-triceratops.ts.net` only, so `.ts.net` names still resolve via MagicDNS while external DNS falls through to 1.1.1.1 on enp8s0. Non-destructive fix.
- **psql UPDATE instead of `axon ingest recover`**: `recover` only handles stale running jobs. Direct SQL `SET status='pending', error_text=NULL, started_at=NULL, finished_at=NULL, result_json=NULL` was the correct reset pattern.
- **Restarted worker to trigger startup sweep**: Rather than manually publishing to AMQP, restarting the worker triggers `reenqueue_orphaned_pending_jobs` which handles re-publication cleanly.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/ingest/github/files.rs:194–199` | Wrapped `chunk_code`/`chunk_text` in `spawn_blocking` | Prevent tree-sitter blocking tokio async runtime |

**No other source files modified this session.** DB mutations via psql only.

---

## Commands Executed

```bash
# Check TEI health
curl -v --max-time 5 http://100.74.16.82:52000/health
# → HTTP 200 OK

# Check DNS resolution
resolvectl query api.github.com
# → FAILED: All attempts to contact name servers or networks failed

dig +time=3 api.github.com @1.1.1.1
# → 140.82.112.5 (WORKS)

dig +time=3 api.github.com @100.100.100.100
# → communications error to 100.100.100.100#53: timed out

# Fix: scope Tailscale DNS to .ts.net only
sudo resolvectl domain tailscale0 manatee-triceratops.ts.net

# Verify fix
resolvectl query api.github.com
# → 140.82.112.5 -- link: enp8s0

curl -s --max-time 5 https://api.github.com/zen
# → "Practicality beats purity."

# Reset DNS-failed jobs to pending
docker exec axon-postgres psql -U axon -d axon -c "
UPDATE axon_ingest_jobs
SET status='pending', error_text=NULL, started_at=NULL,
    finished_at=NULL, updated_at=NOW(), result_json=NULL
WHERE status='failed' AND error_text LIKE '%client error (Connect)%'
"
# → UPDATE 38

# Start worker
RUST_LOG=info ./target/release/axon ingest worker > /tmp/axon-ingest-worker.log 2>&1 &
# → PID: 711672
# Startup log: "re-enqueued 38 orphaned pending job(s) from before broker restart"
```

---

## Behavior Changes (Before/After)

| Dimension | Before | After |
|-----------|--------|-------|
| DNS resolution | All external DNS failing (Tailscale catch-all eating queries) | External DNS routes via enp8s0 → 1.1.1.1; .ts.net via Tailscale |
| Ingest worker | Running stale binary (2026-03-12) missing `reenqueue_orphaned_pending_jobs` | Running v0.25.3 (2026-03-15) with correct startup sweep |
| Ingest job state | 38 jobs in `failed` state with `client error (Connect)` | 38 jobs processing: 6 running, ~14 pending, 328+ completed |
| tree-sitter chunking | Running on async runtime, starving other tasks | Offloaded to `spawn_blocking` pool |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `curl http://100.74.16.82:52000/health` | HTTP 200 | HTTP 200 | ✅ |
| `curl https://api.github.com/zen` | Response body | "Practicality beats purity." | ✅ |
| Worker startup log | `re-enqueued N orphaned` | `re-enqueued 38 orphaned pending job(s)` | ✅ |
| DB status after restart | 6 running, pending jobs | 6 running, 28 pending | ✅ |
| `tei_embed done` in worker log | TEI calls succeeding | `vectors=1 duration_ms=~1500ms` | ✅ |
| 30s stability check | No new failures | 6 running, 14 pending, 328 completed | ✅ |

---

## Source IDs + Collections Touched

No new embeddings initiated this session (ingest jobs embed their own chunks via the worker pipeline). Ingest jobs write to collection `cortex`.

---

## Risks and Rollback

### DNS Change
- **Risk**: If Tailscale MagicDNS recovers, `.ts.net` resolution still works (it's explicitly scoped). No regression.
- **Caveat**: `resolvectl domain` changes are **not persistent** across reboots or network-manager events. Tailscale daemon may reset this on reconnect.
- **Permanent fix**: Add `Domains = manatee-triceratops.ts.net` (without `~.`) in Tailscale admin DNS settings, or configure systemd-resolved to override the catch-all.

### Job State Mutations
- **Rollback**: No clean rollback — jobs that complete successfully are committed to Qdrant. Jobs that fail again will need the same psql reset pattern.

---

## Decisions Not Taken

- **Disabling Tailscale DNS entirely**: Would break `.ts.net` hostname resolution. Scoping is less disruptive.
- **Using `axon ingest recover`**: Only handles stale running jobs, not orphaned pending. Wrong tool.
- **Manually publishing to AMQP**: Cleaner to restart the worker and let `reenqueue_orphaned_pending_jobs` handle it — that's what the function exists for.

---

## Open Questions

1. **Tailscale DNS persistence**: The `resolvectl domain` fix is not persistent. Will it survive a Tailscale reconnect or `tailscaled` restart? May need to configure permanently via Tailscale admin panel or `/etc/systemd/resolved.conf.d/`.
2. **rust-lang/rust and RustPython/RustPython**: These two jobs remain in `failed` state with `watchdog reclaimed stale running ingest job (marker=startup)` — were they running during a previous crash? They haven't been reset — should they be?
3. **Large repo completion**: huggingface/transformers and rustdesk/rustdesk have been running 45+ minutes. Normal for large repos, but worth monitoring for stale watchdog reclaim.

---

## Next Steps

1. **Make DNS fix permanent**: Configure Tailscale to not advertise `~.` catch-all, or add explicit systemd-resolved override so the fix survives reboots.
2. **Reset rust-lang/rust and RustPython/RustPython** if desired — those are large repos that were stale-reclaimed during earlier crashes.
3. **Monitor running jobs** until completion — transformers/rustdesk are large, expected to take 1–2 hours total.
4. **`worker_lane.rs` architectural improvement** (from previous session): Migrate `join_all` to `JoinSet` with `tokio::spawn` per lane so sweep timers are fully independent tasks.
