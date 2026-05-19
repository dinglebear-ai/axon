# Session: `axon screenshot` Command Implementation

**Date:** 2026-02-23 | **Branch:** `fix-crawl`

## Session Overview

Implemented a first-class `axon screenshot <url>` command that captures full-page PNG screenshots via Chrome DevTools Protocol (CDP). The original plan used spider.rs's built-in screenshot support, but Chrome 134's WebSocket handshake incompatibility with spider's `chromiumoxide` fork forced a pivot to a raw CDP implementation over `tokio-tungstenite`. After debugging three layers of WebSocket connection issues, the command works end-to-end: `axon screenshot https://modelcontextprotocol.io` produces a 346KB full-page PNG.

## Timeline

1. **Config layer (Step 1):** Added `CommandKind::Screenshot`, 3 Config fields (`screenshot_full_page`, `viewport_width`, `viewport_height`), CLI args, `parse_viewport()` function, 7 new tests
2. **Engine visibility (Step 2):** Changed `resolve_cdp_ws_url` to `pub(crate)` in `engine.rs`
3. **Spider-based implementation (Steps 3-5):** Built screenshot handler using spider's `ScreenShotConfig`, `Viewport`, `crawl()` API. Wired into `commands.rs`, `lib.rs`, `common.rs`. Fixed compile errors (wrong Viewport import path, wrong `with_wait_for_selector` signature, duplicate dispatch line)
4. **Spider Chrome failure:** Spider's forked `chromiumoxide` cannot handshake with Chrome 134 — "NoResponse" / "HandshakeIncomplete". This is a pre-existing infra issue affecting ALL Chrome operations, not screenshot-specific.
5. **Pivot to raw CDP:** Rewrote `cdp_screenshot()` using `tokio-tungstenite` directly. Hit `Message::Text` Utf8Bytes type mismatch (tungstenite 0.26) and `impl Trait` not allowed in closure parameters.
6. **tokio-tungstenite `connect_async` hang:** Even after fixing compile errors, `connect_async` hung on `ws://127.0.0.1:9222`. Python `websockets` library connected fine to the same URL. Root cause: `connect_async` with `rustls-tls-native-roots` feature does something pathological even on plain `ws://` connections.
7. **Raw TCP fix:** Replaced `connect_async` with manual `TcpStream::connect` + `client_async` (raw WebSocket upgrade over TCP). This bypasses the TLS connector entirely and works perfectly.
8. **Cleanup:** Removed all debug `eprintln!` statements, ran full test suite (359 pass), clippy clean.

## Key Findings

- **Spider chromiumoxide + Chrome 134:** `spider_chromiumoxide` (spider's fork of `chromiumoxide`) has a WebSocket handshake incompatibility with Chrome 134. Both the CDP proxy (port 9222) and raw Chrome (port 9223) fail. This blocks ALL Chrome features in spider, not just screenshots. (`crates/crawl/engine.rs:257-270`)
- **tokio-tungstenite `connect_async` hangs on ws://:** When compiled with `rustls-tls-native-roots`, `connect_async` appears to hang indefinitely on `ws://` URLs (non-TLS). The fix is to manually create a `TcpStream` and use `client_async` for the WebSocket upgrade. (`crates/cli/commands/screenshot.rs:266-301`)
- **tungstenite 0.26 `Utf8Bytes`:** `Message::Text` no longer accepts `String` directly — requires `.into()` for the `Utf8Bytes` conversion. (`tungstenite-0.26.2/src/protocol/message.rs:160`)
- **CDP flattened sessions:** `Target.attachToTarget` with `flatten: true` sends `Target.attachedToTarget` events interleaved with the response. The CDP message loop must skip non-matching IDs.
- **`resolve_cdp_ws_url` reqwest hang:** The engine's resolve function uses `reqwest` which can take several seconds on first call (LazyLock HTTP client initialization with rustls). A 5-second timeout wrapper prevents indefinite hangs.

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Raw CDP over tokio-tungstenite instead of spider | Spider's chromiumoxide fork incompatible with Chrome 134; raw CDP is more reliable and debuggable |
| Manual TCP + `client_async` instead of `connect_async` | `connect_async` with rustls features hangs on `ws://` URLs; raw TCP bypasses the connector |
| `base64` + `tokio-tungstenite` deps (both already in Cargo.toml) | `base64` was already listed; `tokio-tungstenite` was added with `rustls-tls-native-roots` feature |
| Inner `async fn session_cmd` instead of closure | Rust doesn't allow `impl Trait` in closure parameters; inner async fns do support it |
| 5s timeout on `resolve_cdp_ws_url` with direct fallback | Engine's resolve can hang; fallback queries `/json/version` directly with proper Docker hostname rewriting |

## Files Modified

| File | Action | Purpose |
|------|--------|---------|
| `crates/core/config/types.rs` | Edit | Added `Screenshot` to `CommandKind`, 3 Config fields, defaults, Debug impl, 2 tests |
| `crates/core/config/cli.rs` | Edit | Added `Screenshot(ScrapeArgs)` variant, `screenshot_full_page` + `viewport` global args |
| `crates/core/config/parse.rs` | Edit | Added `parse_viewport()`, match arm, field wiring, 5 viewport tests |
| `crates/crawl/engine.rs` | Edit | Changed `resolve_cdp_ws_url` visibility to `pub(crate)` |
| `crates/cli/commands/screenshot.rs` | **New** | Raw CDP screenshot handler (~370 lines): `run_screenshot`, `cdp_screenshot`, `cdp_send`, `session_cmd`, pure logic fns, 7 tests |
| `crates/cli/commands.rs` | Edit | Added `pub mod screenshot; pub use screenshot::run_screenshot;` |
| `lib.rs` | Edit | Added `run_screenshot` import + `CommandKind::Screenshot` dispatch |
| `crates/cli/commands/common.rs` | Edit | Added `CommandKind::Screenshot` to `start_url_from_cfg` URL extraction match |
| `docker-compose.yaml` | Edit | Added port `9223:9223` mapping for raw Chrome DevTools |
| `Cargo.toml` | Edit | `tokio-tungstenite` was already present; `base64` was already present |

## Commands Executed

```bash
# Compile checks (multiple iterations)
cargo check                          # Clean after all fixes
cargo clippy                         # 0 warnings
cargo test screenshot                # 10 tests pass
cargo test                           # 359 pass, 0 fail

# Screenshot tests
cargo run --bin axon -- screenshot https://modelcontextprotocol.io --output /tmp/mcp-screenshot.png
# Result: 346,738 bytes PNG saved

# Chrome debugging
curl -s http://127.0.0.1:6000/json/version  # Returns browser UUID
curl -s http://127.0.0.1:9222/json/version  # Returns browser UUID (same)
# Python websockets: Connected and created target on 9222 (confirmed proxy works)
```

## Behavior Changes (Before/After)

| Before | After |
|--------|-------|
| No `screenshot` command exists | `axon screenshot <url>` takes full-page PNG screenshots |
| Screenshots only available as crawl side-effect | First-class standalone command with viewport/fullPage control |
| No direct CDP WebSocket usage in codebase | Raw CDP protocol implementation via tokio-tungstenite |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | Clean compile | `Finished dev profile` | PASS |
| `cargo clippy` | 0 warnings | `Finished dev profile` | PASS |
| `cargo test screenshot` | 10 tests pass | `10 passed; 0 failed` | PASS |
| `cargo test` | All tests pass | `359 passed; 0 failed` | PASS |
| `axon screenshot https://modelcontextprotocol.io --output /tmp/mcp-screenshot.png` | PNG file created | 346,738 bytes PNG saved | PASS |
| Screenshot content | MCP homepage visible | Full-page render with nav, content, footer | PASS |

## Source IDs + Collections Touched

No Axon embed/retrieve operations were performed during the implementation session itself.

## Risks and Rollback

- **Low risk:** New command, no existing behavior changed. Rollback: `git checkout main -- crates/ lib.rs Cargo.toml docker-compose.yaml`
- **Port 9223 exposure:** Added `127.0.0.1:9223:9223` in docker-compose. Only bound to loopback, matches existing pattern.
- **AtomicU64 CDP_ID:** Process-wide monotonic counter. No risk of collision in single-binary usage.
- **Timeout values:** 5s for TCP/WS connect, 15s for browser CDP commands, 30s for session CDP commands, `timeout_secs + 15` for page load. These are generous but could be tuned.

## Decisions Not Taken

| Alternative | Why Rejected |
|-------------|-------------|
| Fix spider's chromiumoxide fork | Out of scope; upstream issue with Chrome 134 WebSocket handshake |
| Use Puppeteer/Playwright via subprocess | Adds Node.js dependency; raw CDP is simpler and has zero external deps |
| Use `connect_async` with explicit `NoConnector` | `tokio-tungstenite` doesn't expose a clean way to disable TLS connector; raw TCP is more reliable |
| Add `url` crate dependency for URL parsing | Used `reqwest::Url` (re-exported from `url` crate already in dep tree) |

## Open Questions

- **Why does `connect_async` hang on `ws://` with `rustls-tls-native-roots`?** Confirmed Python websockets connects fine. May be a tungstenite 0.26 + rustls interaction bug. Worth investigating upstream.
- **Will Chrome 135+ fix the chromiumoxide handshake?** Unknown. Spider's fork may need updating.
- **Should `--screenshot-full-page false` be renamed to `--full-page`?** Current flag name is verbose but unambiguous.
- **`log_done` / `log_info` output not visible without `RUST_LOG`:** The tracing subscriber filters below `info` by default. User doesn't see "saved: ..." message unless `RUST_LOG=info` is set. Consider using `println!` for user-facing output.

## Next Steps

- Clean up debug `eprintln!` in `resolve_browser_ws_url` (already done in final cleanup)
- Consider adding `--wait-for-selector` support (screenshot waits for specific CSS selector)
- Consider `--format jpeg` support (CDP supports it; just change `captureScreenshot` params)
- Fix `log_done` visibility — either set default `RUST_LOG` or use `println!` for user-facing messages
- Commit changes on `fix-crawl` branch
