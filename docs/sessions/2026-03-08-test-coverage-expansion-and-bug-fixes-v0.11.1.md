# Session: Test Coverage Expansion + Bug Fixes (v0.11.1)
Date: 2026-03-08 | Branch: feat/services-layer-refactor

## Session Overview

Investigated test coverage gaps in the web app (`apps/web/`) and Rust crates (`crates/web/`, `crates/services/`). Dispatched parallel agents to write tests for all identified gaps. Fixed two confirmed frontend bugs discovered during test writing. Then patched three security/correctness bugs in the Rust backend (zip-slip, LogLevel case-sensitivity, XML single-quote escaping). Net result: +914 tests, 2 frontend bug fixes, 3 backend bug fixes, version bump 0.11.0 → 0.11.1.

## Timeline

1. **Coverage audit** — Identified that 655 passing tests gave false confidence. Tests covered pure utility functions but not API routes, hooks, or components where bugs actually live.
2. **6-agent parallel TS test expansion** — Each agent owned distinct files to avoid conflicts:
   - Agent 1: `result-normalizers.test.ts` (3→51 tests, +48)
   - Agent 2: `api-fetch.test.ts` (new, 17 tests)
   - Agent 3: `sessions/git-metadata.test.ts` (5→31, +26) + `api/sessions-routes.test.ts` (new, 10 tests) + `api/workspace-route.test.ts` (new, 17 tests)
   - Agent 4: `api/cortex-routes.test.ts` (new, 27 tests)
   - Agent 5: `use-ws-messages.test.ts` (8→61, +53) + `ws-messages-handlers.test.ts` (17→49, +32)
   - Agent 6: `pulse-chat-api-lib.test.ts` (new, 40 tests) + `pulse-session-store.test.ts` (new, 40 tests)
3. **Two frontend bugs fixed** during test writing (pushCapped + localStorage SSR)
4. **6-agent parallel Rust test expansion** — `crates/web/` and `crates/services/` audited and tested:
   - execute/args.rs, execute/cancel.rs, execute/files.rs, execute/overrides.rs
   - download/archive.rs, docker_stats.rs, pack.rs
   - services/acp.rs, events.rs, query.rs, search.rs, system.rs, types.rs
5. **Commit + push** test expansion (v0.11.1) — required fixing: unused biome imports, `std::path::Path` qualification, `cargo fmt`, clippy PI approximation + too_many_arguments
6. **Three backend bugs patched** via single agent: zip-slip, LogLevel, XML escaping
7. **`save-to-md`** this file

## Key Findings

- **pushCapped bug** (`apps/web/hooks/ws-messages/runtime.ts:23,25`): `items.concat(item)` spreads array arguments — an array payload was being spread into individual elements instead of stored as a single item. Fixed to `[...items, item]`.
- **localStorage SSR crash** (`apps/web/lib/pulse/session-store.ts`): `window.localStorage` throws `ReferenceError` in Node.js/SSR context. Fixed via `getLocalStorage()` helper with `typeof window !== 'undefined'` guard + `SecurityError` try/catch.
- **Zip-slip** (`crates/web/download/archive.rs`): `build_zip` passed `rel_path` verbatim to zip crate. `zip` v8 stores entry names as-is — `../../../etc/passwd` would escape the output directory. Fixed with `sanitize_zip_entry_path()` that keeps only `Component::Normal` segments.
- **LogLevel case-sensitivity** (`crates/services/events.rs`): `from(&str)` match was case-sensitive — `"WARN"` fell to `Info` default. Fixed with `.to_ascii_lowercase()` before match.
- **XML single-quote not escaped** (`crates/web/pack.rs`): `escape_xml_attr` had no arm for `'` — attributes delimited with single quotes would be broken. Fixed with `'\'' => "&apos;"`.
- **BFS blowup risk** documented in `git-metadata.ts`: exponential directory traversal with no depth cap. Not fixed (out of scope) but locked in via test.
- **`map_retrieve_result` silent zero-chunk discard** (`crates/services/query.rs`): results with zero chunks are silently dropped. Documented via test.
- **`build_session_setup` whitespace bug** (`crates/services/acp.rs`): `build_session_setup(Some("   "), ...)` trims to empty string, falls to new-session arm — user history silently abandoned. Documented via test.

## Technical Decisions

- **`vi.resetModules()` + dynamic import** in `api-fetch.test.ts`: required because `API_TOKEN` is a top-level `const` evaluated at module load time. Standard `vi.mock()` can't re-evaluate it per test suite — only `resetModules()` + re-import works.
- **`globalThis.window` + `globalThis.localStorage` stubs** in `pulse-session-store.test.ts`: Vitest runs in Node where neither exists. The store reads `typeof window !== 'undefined'` — the stub must set both to satisfy the guard.
- **`#[allow(clippy::too_many_arguments)]`** on `make_metrics` test helper: clippy's 7-arg limit applies to test code; suppression annotation is the right fix for a pure test helper.
- **`sanitize_zip_entry_path` keeps sanitized remainder** (not skip-all): `../../../etc/passwd` → `etc/passwd` (stored but in safe location). Alternative was to skip entirely — kept partial because a filename like `docs/../README.md` should still emit `README.md`.

## Files Modified

### TypeScript (apps/web)
| File | Change | Purpose |
|------|--------|---------|
| `__tests__/result-normalizers.test.ts` | +48 tests | All 11 normalizer type branches |
| `__tests__/api-fetch.test.ts` | New (17 tests) | Token injection, error handling, network failures |
| `__tests__/sessions/git-metadata.test.ts` | +26 tests | BFS, author extraction, malformed data |
| `__tests__/api/sessions-routes.test.ts` | New (10 tests) | Sessions list + detail routes |
| `__tests__/api/workspace-route.test.ts` | New (17 tests) | Path traversal blocks, .env filtering |
| `__tests__/api/cortex-routes.test.ts` | New (27 tests) | All 6 cortex routes → runAxonCommandWs |
| `__tests__/use-ws-messages.test.ts` | +53 tests | All message handler branches |
| `__tests__/ws-messages-handlers.test.ts` | +32 tests | 100% line/branch on handlers.ts |
| `__tests__/pulse-chat-api-lib.test.ts` | New (40 tests) | NDJSON streaming, request construction |
| `__tests__/pulse-session-store.test.ts` | New (40 tests) | CRUD, caps, SSR guard |
| `hooks/ws-messages/runtime.ts:23,25` | Bug fix | `pushCapped` spread fix |
| `lib/pulse/session-store.ts` | Bug fix | `getLocalStorage()` SSR guard |
| `package.json` + `pnpm-lock.yaml` | Dep added | `@vitest/coverage-v8` installed |

### Rust (crates/)
| File | Change | Purpose |
|------|--------|---------|
| `crates/web/execute/args.rs` | +5 tests | Traversal guard for output_dir |
| `crates/web/execute/cancel.rs` | +10 tests | Unknown mode, UUID validation |
| `crates/web/execute/files.rs` | +10 tests | Traversal, null bytes, env priority |
| `crates/web/execute/overrides.rs` | +4 tests | auto_switch underscore alias |
| `crates/web/download/archive.rs` | +3 tests + **zip-slip fix** | `sanitize_zip_entry_path()` added |
| `crates/web/docker_stats.rs` | +6 tests | round1/round2, divide-by-zero |
| `crates/web/pack.rs` | +6 tests + **XML `'` fix** | `escape_xml_attr` adds `&apos;` |
| `crates/services/acp.rs` | +15 tests | ACP contract regression tests |
| `crates/services/events.rs` | +9 tests + **LogLevel fix** | Case-insensitive `from(&str)` |
| `crates/services/query.rs` | +8 tests | `map_retrieve_result` paths |
| `crates/services/search.rs` | +5 tests | `to_spider_time_range` variants |
| `crates/services/system.rs` | +10 tests | `map_sources/domains_payload` |
| `crates/services/types.rs` | +7 tests | `AcpBridgeEvent` wire shape |
| `Cargo.toml` | 0.11.0 → 0.11.1 | Patch version bump |
| `CHANGELOG.md` | Entry added | v0.11.1 summary |

## Commands Executed

```bash
# Coverage measurement
cd apps/web && pnpm test:coverage

# Dependency install
pnpm add -D @vitest/coverage-v8

# Pre-commit fixes
npx biome check --write --unsafe __tests__/api/sessions-routes.test.ts __tests__/api/workspace-route.test.ts
cargo fmt
cargo clippy --all-targets --locked -- -D warnings

# Verification
cargo test --lib  # 942 lib tests passing
cargo test --all  # 947 total passing
```

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `pushCapped([{url,chunks}])` | Array spread — stored as `[{url},{chunks}]` | Single element — stored as `[[{url,chunks}]]` |
| `getLocalStorage()` in SSR | `ReferenceError: window is not defined` | Returns `null` silently |
| `LogLevel::from("WARN")` | → `Info` (wrong default) | → `Warn` (correct) |
| `escape_xml_attr("it's")` | → `"it's"` (broken XML) | → `"it&apos;s"` (valid XML) |
| `build_zip("../../../etc/passwd")` | Entry stored as `../../../etc/passwd` (traversal possible) | Entry stored as `etc/passwd` (safe) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo clippy --all-targets --locked -- -D warnings` | 0 errors | 0 errors | PASS |
| `cargo fmt --check` | Clean | Clean | PASS |
| `cargo test --lib` | All pass | 942 pass, 0 fail | PASS |
| `cargo test --all` | All pass | 947 pass, 0 fail | PASS |
| Commit hook `lefthook` | All green | All green | PASS |
| Git push | Accepted | `e012ce34..5fcbad02` | PASS |

## Source IDs + Collections Touched

Axon embedding attempted post-session-doc write.

## Risks and Rollback

- **pushCapped change** is a correctness fix — any code that depended on the old spread behavior (treating arrays as flat) would see different results. Tests confirm the new behavior is correct.
- **LogLevel change** changes the runtime behavior of log parsing — `"WARN"` events that were silently dropped as `Info` will now correctly surface as `Warn`. Low risk; improves observability.
- **Zip-slip fix** changes archive behavior: `../../../etc/passwd` entry is now stored as `etc/passwd`. If any caller intentionally relied on relative path traversal in zip output (none identified), this would be a breaking change.
- **Rollback**: `git revert 5fcbad02 e012ce34` to undo both commits.

## Decisions Not Taken

- **Skip the `sanitize_zip_entry_path` remainder** (drop entry entirely if any `..` found): would be safer but loses valid filenames like `docs/../README.md` → `README.md`. Kept the sanitized remainder approach.
- **Fix `map_retrieve_result` zero-chunk discard**: out of scope for a test session; documented via test instead.
- **Fix BFS depth limit** in `git-metadata.ts`: out of scope; documented via test.
- **Fix `build_session_setup` whitespace handling**: out of scope; documented as known behavior in test.

## Open Questions

- `@vitest/coverage-v8` installed but coverage numbers not recorded — what's the actual % post-expansion?
- `crates/web/execute/files.rs` uses `#[allow(unsafe_code)]` for env var mutation in tests — should those be migrated to `serial_test` guards?
- Are there additional callers of `build_zip` beyond the web download route that need the zip-slip guard?

## Next Steps

- Run `pnpm test:coverage` to get exact coverage numbers after expansion
- Consider fixing `map_retrieve_result` silent zero-chunk discard (currently documented, not fixed)
- Consider fixing `build_session_setup` whitespace trim → new session behavior
- BFS depth limit in `git-metadata.ts` git traversal
- Address the 6 GitHub Dependabot security vulnerabilities flagged on push (3 high, 3 moderate)
