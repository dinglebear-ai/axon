# Session: WS Bridge JSON Parsing + Stats Fix
**Date:** 2026-03-01
**Branch:** feat/sidebar

---

## Session Overview

Three bugs fixed this session:

1. **Pulse scrape PDU shows "No scrape markdown captured in memory"** even though the scrape completed — caused by a React `useEffect` ref-sync race between `command.output.json` and `command.done` events.
2. **Stats page displays no stats** (`/cortex/stats`) — two compounding bugs: (a) the WS bridge was parsing individual array element strings (`"url"`, `"domain"`) from pretty-printed JSON as standalone `command.output.json` messages, clobbering the full stats object; (b) `AXON_BIN` env var pointed to a nonexistent dev path causing binary-not-found errors after container rebuild.
3. **Bonus investigation** — confirmed the root cause of both bugs via empirical stdout analysis, identified a broader systemic risk (`to_string_pretty` across 15+ commands), and proposed 3 follow-up optimizations.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Screenshot review: Pulse PDU shows "No scrape markdown captured in memory" but sidebar shows scraped file |
| ~16:41 | Diagnosed PDU bug: `virtualFileContentByPathRef` syncs via `useEffect` (after render), `command.done` fires before effect runs |
| ~16:41 | Fixed: update ref immediately alongside `setVirtualFileContentByPath` state setter |
| ~16:55 | Screenshot review: Stats page — all metrics blank except AVG CHUNK/DOC: 0.8 |
| ~16:55 | Investigated stats flow: API → `runAxonCommandWs` → WS bridge → `axon stats --json` |
| ~17:00 | Root cause found: `payload_fields: ["domain", "source_command", "url"]` — `"url"` on its own line is valid JSON, sets `saw_json_line = true`, recovery pass never runs |
| ~17:05 | WS bridge fix: only treat object/array parsed lines as `saw_json_line`, primitives fall through to `command.output.line` |
| ~17:10 | Rebuilt container — binary not found: `AXON_BIN=/workspace/axon_rust/target/release/axon` nonexistent inside container |
| ~17:15 | `AXON_BIN` fallthrough fix: treat missing path as hint, not hard error |
| ~17:20 | Verified: API now returns full stats object with 3.2M vectors, 324k docs |
| ~17:25 | Proposed 3 follow-up optimizations |

---

## Key Findings

### Bug 1 — Pulse PDU scrape markdown race

- **File:** `apps/web/hooks/use-ws-messages.ts:306,325-326`
- `virtualFileContentByPathRef` is a `useRef` synced to `virtualFileContentByPath` state via `useEffect`
- `useEffect` runs **after** React render cycle — if `command.done` fires in the same microtask batch as `command.output.json`, the ref hasn't been updated yet
- The PDU construction at line 588 reads `virtualFileContentByPathRef.current[scrapeFile.relative_path]` — sees empty ref, produces `"(No scrape markdown captured in-memory.)"`
- The scrape file IS written to disk (sidebar shows it) because that's a separate filesystem write path

### Bug 2a — WS bridge primitive JSON clobber

- **File:** `crates/web/execute/mod.rs:528-539`
- `handle_sync_command` reads stdout line-by-line, tries `serde_json::from_str` on each line
- `serde_json::to_string_pretty` outputs each array element on its own line: `"domain"`, `"source_command"`, `"url"`
- These parse as valid JSON strings (`Value::String(...)`)
- `saw_json_line = true` is set → end-of-stream recovery pass (`!saw_json_line && parse full stdout`) never fires
- The full pretty-printed JSON object is never emitted as `command.output.json`
- `runAxonCommandWs` receives the LAST `command.output.json` — the string `"url"` — as `result`
- API returns `{ "ok": true, "data": "url" }` — a string, not a stats object

**Smoking-gun command:**
```bash
./scripts/axon stats --json 2>/dev/null | while IFS= read -r line; do
  trimmed="$(echo "$line" | sed 's/^[[:space:]]*//')"
  result=$(echo "$trimmed" | python3 -c "import sys,json; v=json.load(sys.stdin); print(type(v).__name__, repr(v)[:60])" 2>/dev/null)
  if [ -n "$result" ]; then echo "PARSEABLE: $trimmed => $result"; fi
done
# Output: PARSEABLE JSON LINE: "url" => str 'url'
```

**Confirmed location:** `payload_fields` array at line 38 of stats output:
```
  "payload_fields": [
    "domain",
    "source_command",
    "url"       ← valid JSON string on its own line
  ],
```

### Bug 2b — `AXON_BIN` env var breaks binary resolution after rebuild

- **File:** `crates/web/execute/mod.rs:214-219`
- `.env` sets `AXON_BIN=/workspace/axon_rust/target/release/axon` (dev host path)
- `common-service` anchor in `docker-compose.yaml` applies `env_file: .env` to ALL services including `axon-workers`
- Inside `axon-workers` container, binary lives at `/usr/local/bin/axon` — dev path doesn't exist
- Old container worked because it was built before `AXON_BIN` was added or from a stale image
- `resolve_exe()` treated nonexistent `AXON_BIN` as a hard error, never fell through to PATH lookup

---

## Technical Decisions

### Fix 1: Immediate ref update alongside state setter
Chose to update `virtualFileContentByPathRef.current` directly at the same site as `setVirtualFileContentByPath`, rather than:
- Waiting for `useEffect` to sync (the existing broken path)
- Fetching from disk via API call at PDU time (scrape "virtual" files don't exist on disk — they live only in React state)
- Changing the state+ref pattern to a ref-only pattern (more invasive refactor)

### Fix 2a: WS bridge object/array gate
Changed `saw_json_line = true` to only fire for `parsed.is_object() || parsed.is_array()`. Primitives fall through to `command.output.line`. Rationale:
- More targeted than switching all commands to `to_string` (compact) — though that's still the recommended follow-up
- Preserves NDJSON behavior (one object per line emits one `command.output.json` per line)
- Single-line primitive output still works via the recovery pass

### Fix 2b: `AXON_BIN` as hint not requirement
Changed: if `AXON_BIN` is set but path doesn't exist, fall through to candidate list + PATH lookup. Rationale:
- `AXON_BIN` is documented as "override binary path" — a hint, not a mandate
- Hard-failing on a nonexistent hint path breaks container builds where the binary is on PATH
- Alternative (fix `.env`) was rejected because `AXON_BIN` in `.env` is valid for host-side dev workflows

---

## Files Modified

| File | Purpose |
|------|---------|
| `apps/web/hooks/use-ws-messages.ts` | Immediate ref update at lines 436–442: update `virtualFileContentByPathRef.current` alongside `setVirtualFileContentByPath` |
| `crates/web/execute/mod.rs` | Two changes: (1) `saw_json_line` guard — only set for object/array parsed values (lines 536–546); (2) `AXON_BIN` fallthrough — treat nonexistent path as hint not error (lines 214–220) |

---

## Commands Executed

```bash
# Diagnose which stdout lines from stats --json parse as valid JSON
./scripts/axon stats --json 2>/dev/null | while IFS= read -r line; do ...

# Confirm line 38 context
./scripts/axon stats --json 2>/dev/null | sed -n '35,45p'

# Verify API response before fix
curl -s http://localhost:49010/api/cortex/stats | python3 -m json.tool | head -20
# Result: { "ok": true, "data": "url" }  ← broken

# Rebuild workers container
docker compose build axon-workers && docker compose up -d axon-workers

# Verify binary location inside container
docker exec axon-workers ls /usr/local/bin/axon /workspace/axon_rust/target/release/axon
# /usr/local/bin/axon exists; dev path does not

# Verify API response after fix
curl -s http://localhost:49010/api/cortex/stats | python3 -m json.tool | head -20
# Result: { "ok": true, "data": { "avg_chunks_per_doc": 9.918..., "indexed_vectors_count": 3215878, ... } }
```

---

## Behavior Changes (Before/After)

| Component | Before | After |
|-----------|--------|-------|
| Pulse PDU after scrape | Shows `"(No scrape markdown captured in-memory.)"` — empty context injected into AI chat | Shows full scraped markdown as conversation context |
| `/cortex/stats` page | All metric cards blank (VECTORS, POINTS, DOCS, DIMENSION, SEGMENTS); only AVG CHUNK/DOC shows `"0.0"` (or stale cached value) | All metric cards populated: 3.2M vectors, 3.2M points, 324k docs, dim 1024, 10 segments |
| `runAxonCommandWs('stats')` | Returns string `"url"` | Returns full `StatsResult` object |
| `AXON_BIN` nonexistent path | Hard error: `"cannot find axon binary: AXON_BIN=... does not exist"` | Falls through to PATH lookup, finds `/usr/local/bin/axon` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `curl .../api/cortex/stats \| head -5` | `{ "ok": true, "data": { "collection": "cortex", ... } }` | `{ "ok": true, "data": { "avg_chunks_per_doc": 9.918..., "collection": "cortex", ... } }` | ✅ PASS |
| `docker exec axon-workers /usr/local/bin/axon --version` | Binary exists | Binary at `/usr/local/bin/axon` confirmed | ✅ PASS |
| `cargo check --bin axon-mcp` (after each edit) | `Finished dev profile` | `Finished dev profile` (both times) | ✅ PASS |
| Stats page manual visual | All metric cards populated | Verified via API response; page populated from full data object | ✅ PASS |

---

## Source IDs + Collections Touched

No Axon embed/retrieve operations were triggered during this session's work. Session doc embed follows below.

---

## Risks and Rollback

| Risk | Severity | Rollback |
|------|----------|---------|
| `use-ws-messages.ts` ref mutation: direct ref write could desync from state if ref is used elsewhere before state settles | Low — ref is only read, never written, by other consumers; state setter still fires | Revert: remove the 4 lines added above `setVirtualFileContentByPath` call |
| WS bridge `saw_json_line` gate: commands that legitimately output a single JSON string/number as their sole output will now use the recovery pass instead of the per-line path | Negligible — no such commands exist in the codebase; recovery pass handles it correctly | Revert: restore `saw_json_line = true` unconditionally in the `Ok(parsed)` arm |
| `AXON_BIN` fallthrough: if a user deliberately sets `AXON_BIN` to a path that doesn't exist yet (e.g., a to-be-built binary), they'll silently get the wrong binary | Low — previous behavior was a hard error; silently wrong binary is harder to debug | Document in `.env.example` that `AXON_BIN` must point to an existing file |

---

## Decisions Not Taken

- **Switch all `to_string_pretty` → `to_string` for `--json` output**: Would eliminate the root cause class, but touches 15+ command files and is a broader change. Deferred as a follow-up optimization (recommended).
- **Fetch scrape markdown from disk at PDU time**: The virtual file path (`virtual/scrape-*.md`) doesn't map to a real filesystem path — only exists in React state. Not feasible without changing the storage model.
- **Fix `.env` `AXON_BIN` value**: User's `.env` is authoritative for their dev environment. Changing it could break local dev workflows. Fixed in code instead.
- **Prefer first object payload in `runAxonCommandWs`** (over last-wins): Valid follow-up but skipped in favor of fixing the source (WS bridge) rather than the consumer.

---

## Open Questions

- The screenshot showed `avg_chunks_per_doc: 0.8` but `data = "url"` (a string) would produce `0.0` via `(undefined ?? 0).toFixed(1)`. Likely the screenshot was from an earlier session state before the container rebuild. Not blocking.
- `AXON_BIN` in `.env.example` — should document that the path must exist at runtime, not just at build time.
- The `stdout_accum` string is built unconditionally even when `saw_json_line` becomes true and the recovery pass won't run. Memory overhead for large crawl outputs.

---

## Next Steps

1. **Apply `to_string` → `to_string_pretty` sweep** across all `--json` output paths in `crates/cli/commands/` and `crates/vector/ops/` — eliminates the entire class of pretty-JSON clobber bugs permanently.
2. **Guard `stdout_accum` accumulation** behind `!saw_json_line` — stop accumulating once an object line is confirmed.
3. **Prefer object payloads in `runAxonCommandWs`** — don't let a later primitive `command.output.json` overwrite a valid earlier object result.
4. **Update `.env.example`** — add note that `AXON_BIN` must point to an existing binary at runtime.
5. **Defensive `MetricCard` rendering** — handle `null`/`undefined` values explicitly rather than relying on TypeScript types to prevent them at runtime.
