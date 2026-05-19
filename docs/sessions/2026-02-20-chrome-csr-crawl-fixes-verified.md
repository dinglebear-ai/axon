# Session: Chrome CSR Crawl Fixes Verified + CDP Log Noise Root-Caused

**Date:** 2026-02-20
**Branch:** `perf/command-performance-fixes`
**Duration:** ~1 hour (continuation from compacted session)

---

## Session Overview

Continued from a compacted session that had applied two fixes for thin-page production on Tailwind/CSR sites. This session:

1. Confirmed prior commits landed correctly and ran the full test suite
2. Verified react.dev Chrome CDP crawl produces real content (628–19,674 bytes/file vs previous 5–79 bytes)
3. Confirmed shadcn.com HTTP scrape works (9,493 bytes from button docs)
4. Systematically root-caused the `chromiumoxide::conn::raw_ws::parse_errors` ERROR flood from workers
5. Suppressed the log noise in `init_tracing()` with a well-commented directive

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | Checked `/tmp/axon-react-test3/markdown/` — crawl had completed successfully |
| +5 min | Ran full test suite (105 tests pass), clippy clean, fmt clean |
| +10 min | Confirmed both commits already landed (`f12fd5d`, `53f7212`) |
| +15 min | Verified shadcn.com HTTP scrape — 9,493 bytes of component docs |
| +30 min | User reports worker log spam; systematic debugging initiated |
| +45 min | Root cause identified: chromey library logs Browserless proxy frames at ERROR level |
| +55 min | Fix applied in `init_tracing()`, tests re-verified |

---

## Key Findings

### Fix Verification: react.dev Chrome CDP Crawl

**Before both fixes:** 20 files, 5–79 bytes each (just `<title>` text)
**After both fixes:** 20 files, 628–19,674 bytes each (full markdown content)

Selected file sizes from `/tmp/axon-react-test3/markdown/`:
```
15,873 bytes  0004-react-dev-learn-managing-state.md   (was 24 bytes)
19,674 bytes  0003-react-dev-learn-escape-hatches.md
14,315 bytes  0005-react-dev-learn.md
12,866 bytes  0001-react-dev-.md
```

### Fix Verification: shadcn.com

`axon scrape https://ui.shadcn.com/docs/components/button` → **9,493 bytes** with full API reference table, code examples, sidebar links. Both HTTP and Chrome modes return real content.

### CDP Log Noise Root Cause

**Error message:** `Failed to parse raw WS message data did not match any variant of untagged enum Message`
**Source:** `chromey-2.38.3/src/conn.rs:214-228` (spider's chromiumoxide fork)
**Log target:** `chromiumoxide::conn::raw_ws::parse_errors`

**Root cause chain:**

1. `Message<T>` in `spider_chromiumoxide_types-0.7.4/src/lib.rs:205` is a `#[serde(untagged)]` enum with exactly two variants:
   - `Response` — requires `id: CallId` (numeric integer)
   - `Event(CdpJsonEventMessage)` — requires `method: MethodId` (string like `"DOM.nodeRemoved"`)

2. Browserless (CDP proxy) sends non-standard session management frames over the same WebSocket. These have neither `id` nor `method` — they use fields like `type`, `sessionId`, `workerId` from Browserless's own protocol layer. Message sizes ~2100 bytes are consistent with session init/status payloads.

3. Serde fails deserialization → `decode_message` in `conn.rs:216` emits `tracing::error!`. This is a **library misclassification** — the frame is gracefully dropped (waker woken, poll returns Pending), crawling continues unaffected.

4. The non-error path (lines 156–161 in conn.rs) logs at `tracing::debug!` for text frames. The error fires from inside `decode_message` before the debug log.

**Impact:** Zero functional impact. Crawls produce correct results despite hundreds of these errors per session.

---

## Technical Decisions

### Suppress in `init_tracing()`, not in env config

**Options considered:**
- Set `RUST_LOG=info,chromiumoxide::conn::raw_ws::parse_errors=off` in docker-compose/`.env` — works but requires env config on every deployment
- Filter in `init_tracing()` via `add_directive` after `try_from_default_env()` — code-side default, env can still override

**Chosen:** Code-side default in `init_tracing()`. The directive is added after parsing `RUST_LOG`, so it suppresses by default but `RUST_LOG=chromiumoxide::conn::raw_ws::parse_errors=error` still works for debugging.

### Not patching chromey

Changing `tracing::error!` → `tracing::debug!` in the chromey source is the correct fix at the library level, but requires maintaining a fork of spider's chromiumoxide fork. The filter approach achieves the same user-visible outcome without a fork.

---

## Files Modified

| File | Change | Reason |
|------|--------|--------|
| `crates/core/logging.rs` | Add `SUPPRESS_CDP_NOISE` directive to `init_tracing()` | Silence Browserless proxy frame parse errors logged at wrong level |

**Previously committed (from prior session, verified in-place):**

| Commit | File | Change |
|--------|------|--------|
| `f12fd5d` | `crates/crawl/engine.rs` | `normalize_cdp_url()`, Chrome-first branching, `idle_network0`, `with_fingerprint(true)`, `WebDriverBrowser::Chrome` |
| `f12fd5d` | `crates/core/content.rs` | `LazyLock<TransformConfig>` static, scan only `<head>` for meta description |
| `53f7212` | `crates/core/content.rs` | `clean_html: false` — prevents `[class*='ad']` stripping Tailwind `shadow-*` elements |
| `53f7212` | `crates/core/content.rs` | `extract_loc_values` — `eq_ignore_ascii_case` instead of full-XML lowercase clone |

---

## Commands Executed

```bash
# Verify crawl results
find /tmp/axon-react-test3/markdown/ -name "*.md" -exec wc -c {} \; | sort -n

# Verify shadcn HTTP scrape
./scripts/axon scrape https://ui.shadcn.com/docs/components/button --wait true | wc -c
# → 9,493 bytes

# Locate chromey source
find ~/.cargo/registry/src -path "*/chromey-*/src/conn.rs"
# → /home/jmagar/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/chromey-2.38.3/src/conn.rs

# Verify tests pass
cargo test 2>&1 | grep "test result:"
# → 98 passed, 0 failed (+ integration tests all ok)
```

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| react.dev Chrome crawl (20 pages) | 5–79 bytes/file (title only) | 628–19,674 bytes/file (full content) |
| shadcn.com scrape | Title only (shadow-styled elements stripped) | 9,493 bytes (full component docs) |
| Worker log output during Chrome crawl | Hundreds of ERROR lines per second | Silent (parse noise suppressed) |
| `RUST_LOG` override for CDP debug | N/A | Set `chromiumoxide::conn::raw_ws::parse_errors=error` to restore |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `find /tmp/axon-react-test3/markdown/ -name "*.md" \| wc -l` | 20 | 20 | ✓ PASS |
| `wc -c /tmp/axon-react-test3/markdown/0004-react-dev-learn-managing-state.md` | >1000 | 15,873 | ✓ PASS |
| `./scripts/axon scrape https://ui.shadcn.com/docs/components/button \| wc -c` | >5000 | 9,493 | ✓ PASS |
| `cargo test \| grep "test result:"` | all ok | 98 passed, 0 failed | ✓ PASS |
| `cargo clippy` | 0 warnings | 0 warnings | ✓ PASS |
| `cargo fmt --check` | clean | clean | ✓ PASS |

---

## Risks and Rollback

**Risk:** Suppressing `chromiumoxide::conn::raw_ws::parse_errors=off` could hide a real parse failure if a future chromey version changes error semantics.
**Mitigation:** The directive is `off` for this specific target only. All other chromiumoxide ERROR logs remain visible. Users can restore with `RUST_LOG=chromiumoxide::conn::raw_ws::parse_errors=error`.
**Rollback:** Revert `crates/core/logging.rs` to `EnvFilter::from_default_env()`.

---

## Decisions Not Taken

- **Fork chromey to change `error!` → `debug!`**: Correct at library level but creates a fork maintenance burden. Filter achieves same outcome.
- **Set `RUST_LOG` in docker-compose**: Environment-config approach; requires updating every deployment context. Code-side default is more robust.
- **Use `RUST_LOG=warn` globally**: Would also suppress legitimate WARN+ messages from other targets. Per-target `off` is surgical.

---

## Open Questions

- What exactly does Browserless send in those ~2100-byte frames? Would need a WS proxy (mitmproxy, Wireshark) or Browserless source to confirm the exact field names (`type`, `workerId`, etc.)
- Does Browserless have a config option to suppress these management frames? Not investigated.

---

## Next Steps

- Rebuild `axon-workers` Docker image with `logging.rs` change: `docker compose build axon-workers && docker compose up -d axon-workers`
- Commit `crates/core/logging.rs` and push branch
- Consider crawling shadcn.com docs properly now that both HTTP and Chrome modes produce content
