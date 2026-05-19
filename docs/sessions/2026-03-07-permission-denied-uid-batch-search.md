# Session: PermissionDenied Debug, Container UID Fix, Batch Search Refactor
Date: 2026-03-07
Branch: feat/services-layer-refactor

---

## Session Overview

Three distinct workstreams resolved in one session:

1. **Root-caused and fixed** `Error: Os { code: 13, kind: PermissionDenied }` on embed step — `/home/jmagar/appdata/axon/` was owned by `root:root`; fixed with `chown`.
2. **Changed container UID** from 1001 → 1000 in `docker/Dockerfile` so the `axon` user matches the host user (`jmagar`, uid=1000), preventing future permission conflicts on bind-mounted volumes.
3. **Added batch search support** to `axon search` — multiple positional args now run as separate Tavily queries; batch logic lives in `services/search.rs::search_batch()`, not the CLI or MCP layer.

A Spider Cloud API refactor was attempted and then fully reverted after clarification (see Decisions Not Taken).

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | User ran `/axon scrape https://spider.cloud/docs/core/realtime-search`; scrape succeeded, embed failed with `PermissionDenied` |
| Phase 1 | Investigated SSRF redirect policy in `http/client.rs` as suspected cause — ruled out (Qdrant/TEI don't redirect) |
| Phase 1 | `RUST_BACKTRACE=full` showed no useful stack (error swallowed at command boundary) |
| Phase 1 | `strace -f ./target/debug/axon scrape … 2>&1 | grep -i 'mkdir\|EACCES'` → `mkdir("/home/jmagar/appdata/axon/output", 0777) = -1 EACCES` |
| Phase 1 | `sudo chown -R jmagar:jmagar /home/jmagar/appdata/axon` — fixed. Verified with `axon scrape https://example.com --wait true` → `✓ embedded 1 chunks into cortex` |
| Phase 2 | Changed `docker/Dockerfile` line 187: `useradd -u 1001` → `useradd -u 1000` |
| Phase 3 | User asked about Spider Cloud's search+scrape API; Spider Cloud refactor attempted (added `SPIDER_API_KEY`) |
| Phase 3 | Refactor fully reverted after user objected to new external API dependency |
| Phase 4 | Added batch search: multiple positional args → `search_batch()` in services layer → sequential Tavily queries → merged results |
| Phase 4 | `cargo check` clean throughout |

---

## Key Findings

- **Root cause of PermissionDenied**: `strace` line 2223: `mkdir("/home/jmagar/appdata/axon/output", 0777) = -1 EACCES`. The directory `/home/jmagar/appdata/axon/` was owned `root:root` mode 755. `AXON_DATA_DIR=/home/jmagar/appdata` in `.env` causes `output_dir` to resolve there via `build_config.rs:479-486`.
- **False lead**: `http/client.rs:26-29` constructs a synthetic `PermissionDenied` for SSRF-blocked redirects — was initially suspected but Qdrant/TEI return 200 directly without redirect.
- **Container UID**: `docker/Dockerfile:187` had `useradd -r -g axon -u 1001`. Change to 1000 aligns with host user `jmagar` (uid=1000/gid=1000), avoiding permission conflicts on any bind-mounted paths.
- **`spider_agent::TimeRange` is not `Copy`**: required `.clone()` in the `search_batch` loop in `services/search.rs:72`.
- **Spider Cloud search+scrape**: `POST https://api.spider.cloud/search` with `fetch_page_content: true` returns `[{url, content, status, costs}]` in one call. Requires `SPIDER_API_KEY` — cloud-only, no self-hosted equivalent. Our stack can replicate it: Tavily (search) + spider crate (scrape) + TEI (embed).

---

## Technical Decisions

- **`search_batch()` in services layer**: batch logic belongs in `services/search.rs`, not `cli/commands/search.rs` or MCP handlers. `search()` now delegates to `search_batch()` for single-query case, keeping the API backward-compatible.
- **Sequential rather than parallel Tavily queries**: kept simple; parallel would require `tokio::join_all` and more complex error handling. No user requirement for parallelism stated.
- **Reverted Spider Cloud refactor**: adding `SPIDER_API_KEY` introduced a new external dependency for functionality already covered by Tavily. User confirmed they want no new external API keys.
- **chown over code change**: fixing file permissions directly was the correct fix — no code change needed for the PermissionDenied bug. The code behavior (`AXON_DATA_DIR` → `output_dir`) is correct; the host filesystem state was wrong.

---

## Files Modified

| File | Change |
|------|--------|
| `docker/Dockerfile:187` | `useradd -u 1001` → `useradd -u 1000` |
| `crates/services/search.rs` | Added `search_batch()` pub fn; `search()` now delegates to it |
| `crates/cli/commands/search.rs` | `run_search()` uses `search_batch()` from services; removed inline loop; multi-positional arg support |

No new files created. No config fields added (spider_api_key added and reverted).

---

## Commands Executed

```bash
# Root cause investigation
strace -f ./target/debug/axon scrape https://spider.cloud/docs/core/realtime-search --wait true 2>&1 | grep -i 'mkdir\|EACCES'
# → mkdir("/home/jmagar/appdata/axon/output", 0777) = -1 EACCES (Permission denied)

# Fix
sudo chown -R jmagar:jmagar /home/jmagar/appdata/axon

# Verification
cargo run -q --bin axon -- scrape https://example.com --wait true
# → ✓ embedded 1 chunks into cortex

# Compile checks throughout
cargo check --bin axon  # Finished in 0.59s / 0.62s / 0.98s — all clean
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `axon search "q1" "q2"` | Only `q1` searched (positionals joined with space into one query) | Each positional runs as a separate Tavily search; results merged |
| `axon-workers` container user | UID 1001 | UID 1000 (matches host `jmagar`) |
| `/home/jmagar/appdata/axon/` owner | `root:root` (blocked embed) | `jmagar:jmagar` (embed works) |
| Embed step of `axon scrape` | `Error: Os { code: 13, kind: PermissionDenied }` | `✓ embedded N chunks into cortex` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` (after Dockerfile + search changes) | `Finished` | `Finished dev profile in 0.98s` | PASS |
| `cargo check --bin axon` (after batch search) | `Finished` | `Finished dev profile in 0.62s` | PASS |
| `cargo run -q --bin axon -- scrape https://example.com --wait true` | embed success | `✓ embedded 1 chunks into cortex` | PASS |

---

## Source IDs + Collections Touched

No Axon embed/retrieve operations were performed during the session's work (only verification of the fix via CLI binary directly, not via MCP tool calls).

---

## Risks and Rollback

- **Dockerfile UID change**: requires container rebuild (`docker compose build axon-workers`) and rechown of any files currently owned by UID 1001 in `$AXON_DATA_DIR`. Without rechown, the new container's axon user (uid=1000) won't be able to write files created by the old container (uid=1001).
  - Rollback: revert `docker/Dockerfile:187` back to `useradd -u 1001` and rebuild.
- **chown of appdata**: irreversible (previous owner was root:root — intentional change). No rollback path needed; the old state was broken.
- **`search_batch()` API**: `search()` is backward-compatible (delegates to `search_batch` with a single-element slice). MCP and all callers of `services::search::search()` are unaffected.

---

## Decisions Not Taken

- **Spider Cloud API refactor** (`SPIDER_API_KEY`): Attempted — rewrote `search_results()` to POST to `https://api.spider.cloud/search`. Reverted because it introduced a new paid external API dependency for functionality already covered by Tavily. The Spider Cloud search+scrape endpoint (search + fetch_page_content in one call) is cloud-only with no self-hosted equivalent.
- **Parallel batch queries**: `tokio::join_all` for concurrent Tavily searches — rejected, no user requirement, adds complexity, Tavily rate limits make parallelism risky.
- **Fix via code** (override output_dir default): PermissionDenied could have been worked around by changing the default `output_dir` in `build_config.rs`. Rejected — the directory permissions were the actual problem; fixing them is the correct fix.

---

## Open Questions

- **Spider Cloud search+scrape**: User asked if it's achievable with self-hosted infrastructure. Answer: the search engine query step requires an external API (Tavily, Spider Cloud, etc.). The scrape+embed steps use our own stack. User's intent for whether to pursue the combined flow is unresolved.
- **Container rebuild timing**: Dockerfile UID change is committed but containers haven't been rebuilt yet. Data dirs in `$AXON_DATA_DIR` may have files owned by UID 1001 that need rechown before the new image works correctly.
- **`axon search` auto-enqueue behavior**: Does the current search command enqueue crawl jobs for result URLs after returning? If so, batch search will enqueue jobs for all result URLs from all queries.

---

## Next Steps

1. Rebuild `axon-workers` image: `docker compose build axon-workers`
2. Rechown appdata for UID transition: `sudo find /home/jmagar/appdata/axon -uid 1001 -exec chown jmagar:jmagar {} +`
3. Decide on Spider Cloud search+scrape vs. Tavily+crawl combined flow for `axon search`
4. Consider making `axon search` synchronously scrape+embed result URLs (using existing spider crate + TEI pipeline) rather than async job queue
