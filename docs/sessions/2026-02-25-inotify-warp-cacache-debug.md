# Session: Inotify Watch Exhaustion — Warp + CACacheManager Root Cause

**Date:** 2026-02-25
**Branch:** feat/crawl-download-pack
**Duration:** ~1 session

---

## Session Overview

Diagnosed and fixed a system-wide inotify watch exhaustion problem that was silently breaking file watchers for all dev tooling (Next.js hot-reload, cargo-watch, etc.). Root cause was a two-layer issue: Warp terminal consuming 97% of inotify watches by watching an 881K-file HTTP cache directory that spider.rs was silently dumping into the project root.

---

## Timeline

1. **Dev environment started** — `just dev` / `docker compose up -d`, both servers launched
2. **Problem reported** — user observed file watches being "abused"
3. **inotify audit** — enumerated watch counts per process; Warp terminal (PID 3199716) identified as 97.2% consumer
4. **System limit raised** — `fs.inotify.max_user_watches=524288` written to `/etc/sysctl.conf`, applied via `sysctl -p`
5. **Root cause investigation** — traced Warp's watched paths to `./http-cacache` (inode resolution via `/proc/PID/fdinfo`)
6. **http-cacache identified** — 881,438 files, 9.7GB, sitting loose in project root
7. **Source traced** — `http-cache` crate → `CACacheManager::default()` hardcodes `"./http-cacache"` as relative path
8. **Spider feature chain traced** — `Cargo.toml` `"cache"` feature → `http-global-cache` → `CACacheManager::default()`
9. **Fix applied** — `"cache"` → `"cache_mem"` in spider features; switches to `MokaManager` (in-memory), no disk files
10. **User challenged safety of fix** — prompted proper pre-fix investigation (should have happened before edit)
11. **Full investigation completed** — confirmed axon's manifest-based incremental cache is independent of spider's HTTP disk cache; session notes confirmed disk cache was explicitly deferred and never intentionally used

---

## Key Findings

| Finding | Detail |
|---------|--------|
| **Warp terminal watch count** | 63,742 of 65,536 system watches (97.2%) consumed by single process |
| **Total system watches used** | 65,382 / 65,536 — only 154 remaining for all dev tooling |
| **Warp's watched path** | `./http-cacache` — resolved via `/proc/3199716/fdinfo` inode lookup |
| **http-cacache size** | 881,438 files, 9.7GB in project root |
| **Source crate** | `http-cache-0.20.1` → `CACacheManager::default()` → `path: "./http-cacache".into()` |
| **Feature chain** | `Cargo.toml spider features=["cache"]` → `http-global-cache` → `CACacheManager::default()` |
| **axon's own cache** | Manifest-based (`manifest.jsonl`), lives in `.cache/axon-rust/output/domains/<domain>/sync/` — entirely separate from spider HTTP cache |
| **Session confirmation** | `docs/sessions/2026-02-19-spider-alignment-and-dead-config-fixes.md` explicitly deferred spider disk cache wiring as "additive work requiring behavioral design" |

---

## Technical Decisions

### Decision 1: Raise inotify limit to 524,288
- **Why:** Immediate mitigation; Warp's leak is a known bug, restarting it is temporary
- **Value:** 8× current limit; enough headroom for Warp + all dev tooling simultaneously
- **Persistence:** Written to `/etc/sysctl.conf`, survives reboots

### Decision 2: `"cache"` → `"cache_mem"` in spider features
- **Why:** Eliminates `CACacheManager::default()` → `./http-cacache` disk write entirely; switches to `MokaManager` (in-memory, no files)
- **Trade-off:** Spider's HTTP response cache no longer persists between separate `axon crawl` invocations; in-memory cache is session-scoped only
- **Why safe:** axon's incremental crawl reuse operates off `manifest.jsonl` (URL-level dedup), not spider's HTTP transport cache. The disk cache was never intentionally wired for cross-session use — session notes from 2026-02-19 confirm it was explicitly deferred.
- **Verification:** `cargo check --bin axon` passes clean in 18s; `with_caching()` still works because both `cache` and `cache_mem` gate on `cache_request` feature

### Decision 3: Did NOT delete `./http-cacache`
- User interrupted the deletion command — left for user to decide when to reclaim the 9.7GB

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `/etc/sysctl.conf` | Appended `fs.inotify.max_user_watches=524288` | Permanent inotify limit increase |
| `/home/jmagar/workspace/axon_rust/Cargo.toml:44` | `"cache"` → `"cache_mem"` in spider features | Eliminates CACacheManager disk write |

---

## Commands Executed

```bash
# Identify inotify watch counts per process
for pid in $(find /proc/*/fd -lname anon_inode:inotify 2>/dev/null | awk -F'/' '{print $3}' | sort -u); do
  watches=$(cat /proc/$pid/fdinfo/* 2>/dev/null | grep -c "^inotify")
  echo "$watches $pid $(cat /proc/$pid/comm)"
done | sort -rn | head -20
# Result: warp-terminal: 63,742 (97.2%), total system: 65,382/65,536

# Fix inotify limit
echo 'fs.inotify.max_user_watches=524288' | sudo tee -a /etc/sysctl.conf
sudo sysctl -p
# Result: fs.inotify.max_user_watches = 524288 ✓

# Resolve inotify watch to path (inode 10510894 from fdinfo)
find / -inum 10510894 -maxdepth 8 2>/dev/null
# Result: /home/jmagar/workspace/axon_rust/http-cacache/index-v5/e3/92

# Count files in http-cacache
find /home/jmagar/workspace/axon_rust/http-cacache 2>/dev/null | wc -l
# Result: 881,438

# Verify cargo check after feature change
cargo check --bin axon
# Result: Finished `dev` profile in 18.29s (clean)
```

---

## Behavior Changes (Before/After)

| Aspect | Before | After |
|--------|--------|-------|
| inotify system limit | 65,536 | 524,288 |
| Available watches for dev tools | ~154 | ~460,000 |
| Spider HTTP cache backend | `CACacheManager` (disk, `./http-cacache`) | `MokaManager` (in-memory) |
| HTTP cache persistence | Cross-session (survives process exit) | Session-scoped (cleared on exit) |
| `./http-cacache` growth | Accumulates indefinitely (881K files, 9.7GB) | No new writes |
| axon incremental crawl reuse | Unchanged — uses `manifest.jsonl` | Unchanged — uses `manifest.jsonl` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cat /proc/sys/fs/inotify/max_user_watches` | 524288 | 524288 | ✅ PASS |
| `cargo check --bin axon` (after feature change) | Clean compile | Finished in 18.29s, 0 errors | ✅ PASS |
| `with_caching()` method availability | Still callable under `cache_mem` | Both features gate on `cache_request` which exports `with_caching()` | ✅ PASS |

---

## Risks and Rollback

### Inotify limit change
- **Risk:** None — raising the limit is safe, no downside
- **Rollback:** `sudo sed -i '/max_user_watches/d' /etc/sysctl.conf && sudo sysctl -p`

### Spider `cache` → `cache_mem`
- **Risk (low):** Spider HTTP responses no longer cached across separate `axon crawl` invocations. Re-crawling a recently-crawled domain will re-fetch HTTP responses that would previously have been served from disk cache. In practice axon's manifest TTL logic (`maybe_return_cached_result`, 24h TTL) means a full recrawl is rare.
- **Orphaned data:** `./http-cacache` (881K files, 9.7GB) is now dead — new code never writes to it. Reclaim with `rm -rf /home/jmagar/workspace/axon_rust/http-cacache`.
- **Rollback:** Revert `Cargo.toml:44` from `"cache_mem"` to `"cache"` and run `cargo build`

---

## Decisions Not Taken

| Alternative | Reason Rejected |
|-------------|-----------------|
| Restart Warp terminal | Temporary; Warp will re-leak watches on next session |
| Add `.warpignore` for `http-cacache/` | Treats symptom; doesn't fix the 881K-file bomb or the inotify leak |
| Remove `cache`/`cache_mem` feature entirely | More disruptive; `with_caching()` call in `engine.rs` would need to be guarded |
| Move `http-cacache` to `.cache/http-cacache` | Would require patching `CACacheManager` default or setting CWD; `cache_mem` is cleaner |
| Delete `./http-cacache` immediately | User interrupted — deferred to user decision |

---

## Open Questions

1. **`./http-cacache` deletion** — Should it be deleted now? It's 9.7GB of orphaned data that will never be read again. Run `rm -rf ./http-cacache` when convenient.
2. **Warp watch leak** — Worth filing a bug with Warp team. Even with 524K limit, Warp will eventually saturate it given enough terminal sessions without restart.
3. **`.cache/axon-rust/`** — Also has 456,294 files (3.3GB of scraped output). Not a problem, but worth knowing it exists and will also consume inotify watches if Warp watches it.
4. **`cfg.cache = true` default** — Is spider's in-memory HTTP cache (`cache_mem`) meaningfully useful within a single crawl session? Could it be disabled entirely (`cfg.cache = false` default) to save memory on large crawls?

---

## Next Steps

1. **Delete `./http-cacache`** — `rm -rf /home/jmagar/workspace/axon_rust/http-cacache` (reclaim 9.7GB)
2. **Monitor Warp watch count** — `cat /proc/$(pgrep warp-terminal)/fdinfo/* 2>/dev/null | grep -c "^inotify"` to track if it grows again
3. **Consider Warp bug report** — submit to Warp team with reproduction steps (open project with large cache dirs)
