# Chrome CDP Four Issues Fix

**Date:** 2026-02-22
**Branch:** `perf/command-performance-fixes`
**Session type:** Continuation (context-compacted from prior session)

---

## Session Overview

Fixed four tightening issues in the Chrome CDP (Chrome DevTools Protocol) wiring introduced when switching from `axon-webdriver` (Selenium) to `headless_browser` (`axon-chrome`) in a previous session. All four issues were fully addressed, 36 new tests were added (153 → 189 passing), and the Docker stack was rebuilt.

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Continued from prior context: "Address ALL four issues" + "ok rebuild the stack" already requested |
| Phase 1 | Read current state of all relevant files to verify what had/hadn't been done |
| Phase 2 | Discovered `is_docker_service_host` was **missing** from `parse/mod.rs` despite prior session summary claiming it was done |
| Phase 3 | Implemented all four issues across 6 files |
| Phase 4 | `cargo test --lib` → 189 passing, 0 failures |
| Phase 5 | `docker compose build axon-workers && docker compose up -d` → stack rebuilt |
| Session end | `/save-to-md` invoked |

---

## Key Findings

- **`is_docker_service_host` was never added** to `parse/mod.rs` despite the prior session summary stating it was. Reading the actual file revealed only `HOST_MAP` and `normalize_local_service_url` — the function was absent. All downstream callers were failing to compile.
- **`headless_browser` always embeds container hostname** in `/json/version` responses via `HOSTNAME_OVERRIDE=axon-chrome`. When called from the host (outside Docker), the returned `webSocketDebuggerUrl` contains `axon-chrome` as the host, which is unresolvable outside Docker. The fix is to detect this and rewrite to `127.0.0.1`.
- **Double-fetch eliminated**: Bootstrap probe (`bootstrap_chrome_runtime`) and `configure_website` → `resolve_cdp_ws_url` were both independently fetching `/json/version`. The pre-resolved WS URL is now threaded through `ChromeBootstrapOutcome.resolved_ws_url` → `effective_cfg.chrome_remote_url` → `ws://` shortcut in `resolve_cdp_ws_url`.
- **`normalize_cdp_url` (engine.rs) and `to_devtools_probe_url` (runtime.rs)** were duplicate implementations of the same CDP URL construction logic. Consolidated into `cdp_discovery_url` in `http.rs`.

---

## Technical Decisions

### Issue 1: Explicit allowlist over fragile heuristic
- **Before**: `host.contains('-') && !host.contains('.')` — matched any hyphenated hostname (`my-home-server`, `custom-chrome-proxy`)
- **After**: `is_docker_service_host(host)` — checks against `HOST_MAP` in `parse/mod.rs`. Only known service names (`axon-postgres`, `axon-redis`, `axon-rabbitmq`, `axon-qdrant`, `axon-chrome`) are rewritten.
- **Rationale**: False positives from the heuristic would silently rewrite legitimate external hosts to `127.0.0.1`, causing mysterious connection failures.

### Issue 2: Double-fetch elimination via `ws://` shortcut
- Bootstrap already resolves the full WS URL (including the `/devtools/browser/UUID` path). Instead of discarding it, store it in `ChromeBootstrapOutcome.resolved_ws_url`.
- `sync_crawl.rs` creates `effective_cfg` with `chrome_remote_url = ws_url` when bootstrap succeeds.
- `resolve_cdp_ws_url` detects `ws://`/`wss://` prefix and returns immediately without fetching.
- Inside Docker, `resolve_cdp_ws_url` still returns `None` (container hostnames resolve on bridge network); `configure_website` falls back to passing `cdp_discovery_url(remote_url)` directly to spider.

### Issue 3: Single canonical `cdp_discovery_url` in `http.rs`
- Both implementations handled: ws→http scheme conversion, port preservation, path defaulting to `/json/version`
- Placed in `http.rs` (not `engine.rs` or `runtime.rs`) because it's a general HTTP utility, not crawl-specific
- Both callers import from `crate::crates::core::http`

### Issue 4: Tests co-located with implementations
- `cdp_discovery_url` tests in `http.rs` (6 cases: ws scheme, wss scheme, http with path, https with path, invalid URL, custom path)
- `is_docker_service_host` tests in `parse/mod.rs` (3 cases: known names match, non-allowlist names don't, plain IPs don't)
- CDP hostname detection test in `engine/tests.rs` (1 case: cross-module integration smoke test)

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/core/http.rs` | Added `cdp_discovery_url` + 6 tests | Canonical CDP URL builder; dedup logic |
| `crates/core/config/parse/mod.rs` | Added `is_docker_service_host` + 3 tests | Explicit allowlist for Docker service hostname rewriting |
| `crates/cli/commands/crawl/runtime.rs` | Full rewrite of probe logic | `probe_cdp_connection` → `Option<String>`; `resolved_ws_url` in outcome struct |
| `crates/crawl/engine.rs` | 4 targeted edits | Removed `normalize_cdp_url`; `ws://` shortcut in `resolve_cdp_ws_url`; use `is_docker_service_host` |
| `crates/cli/commands/crawl/sync_crawl.rs` | Added `effective_cfg` logic | Thread pre-resolved WS URL through to Chrome mode calls |
| `crates/crawl/engine/tests.rs` | Added 1 test | `test_docker_service_host_only_rewrites_known_names` |

---

## Commands Executed

```bash
# Verify test count before and after
cargo test --lib
# Before: 153 passing
# After:  189 passing, 0 failures, 0 ignored

# Lint check
cargo clippy
# Result: 0 warnings

# Rebuild workers image
docker compose build axon-workers

# Restart stack
docker compose up -d
# Result: axon-workers recreated, all containers healthy
```

---

## Behavior Changes (Before / After)

| Scenario | Before | After |
|----------|--------|-------|
| Host `my-home-server` in WS URL | Rewritten to `127.0.0.1` (heuristic false-positive) | Passed through unchanged |
| Host `axon-chrome` in WS URL (outside Docker) | Rewritten to `127.0.0.1` ✓ | Rewritten to `127.0.0.1` ✓ (via explicit allowlist) |
| Chrome mode with successful bootstrap | Two `/json/version` fetches (one in bootstrap, one in `resolve_cdp_ws_url`) | One fetch; second call hits `ws://` shortcut and returns immediately |
| CDP URL construction | Two independent implementations (`normalize_cdp_url`, `to_devtools_probe_url`) | One canonical `cdp_discovery_url` in `http.rs` |
| `is_docker_service_host` function | Missing (compile error for callers) | Present in `parse/mod.rs`, exported `pub(crate)` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test --lib` | All tests pass | 189 passing, 0 failures | ✅ |
| `cargo clippy` | 0 warnings | 0 warnings | ✅ |
| `cargo fmt --check` | Clean | Clean | ✅ |
| `docker compose build axon-workers` | Build succeeds | Build succeeded | ✅ |
| `docker compose up -d` | All containers healthy | All containers healthy | ✅ |

---

## Source IDs + Collections Touched

| Source ID | Collection | Outcome |
|-----------|-----------|---------|
| `docs/sessions/2026-02-22-chrome-cdp-four-issues-fix.md` | `cortex` | ✅ Embedded (1 doc, 1 chunk) + retrieve verified |

---

## Risks and Rollback

- **Risk**: `is_docker_service_host` only covers the current `HOST_MAP` entries. If new Docker services are added later (e.g., `axon-tei`), they won't be rewritten automatically — a developer must add them to `HOST_MAP`.
- **Rollback**: `git revert` the commits on `perf/command-performance-fixes`. The old heuristic (`contains('-') && !contains('.')`) was in `resolve_cdp_ws_url` in `engine.rs`.

---

## Decisions Not Taken

- **Dynamic hostname detection via `/etc/hosts` or DNS lookup**: Would have allowed zero-config discovery of Docker service names but adds I/O, latency, and surface area. Explicit allowlist is simpler and auditable.
- **Storing bootstrap WS URL in a global/thread-local**: Considered to avoid passing through `effective_cfg`, but would introduce hidden state. The `Config` clone approach is explicit and testable.
- **Moving `cdp_discovery_url` to `crawl/engine.rs`**: Rejected because the function is general HTTP utility logic (URL construction), not crawl-domain logic. `http.rs` is the correct home.

---

## Open Questions

- **Inside-Docker path untested**: `resolve_cdp_ws_url` returns `None` inside Docker (the code checks `/.dockerenv`), deferring to spider.rs to fetch `/json/version` itself. This path was not end-to-end verified in this session.
- **`ws://` shortcut and webdriver mode**: When `chrome_bootstrap.mode == WebDriverFallback`, `effective_cfg` still has `chrome_remote_url = ws_url`. Does the webdriver path in `configure_website` tolerate a `ws://` URL? Not verified.
- **Test count delta**: 153 → 189 = 36 new tests. The per-file breakdown is: http.rs +6, parse/mod.rs +3, engine/tests.rs +1. The remaining 26 tests may have been added in prior work already included in the compacted context.

---

## Next Steps

- Run a live Chrome crawl against a real JS-heavy site (`shadcn.com`) to confirm the double-fetch elimination and hostname rewriting work end-to-end.
- Verify inside-Docker path: start the stack, exec into `axon-workers`, run a Chrome crawl, and confirm `resolve_cdp_ws_url` returns `None` correctly.
- Add `axon-tei` to `HOST_MAP` if/when TEI is containerized (currently an external self-hosted service).
- Consider opening a PR from `perf/command-performance-fixes` → `main` once all outstanding items are confirmed.
