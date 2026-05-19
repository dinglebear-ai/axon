# Session: Three-Round Sweep — apps/web + crates/services/ Simplify
Date: 2026-03-20
Branch: feat/pulse-shell-and-hybrid-search

---

## Session Overview

Executed a comprehensive code quality sweep triggered by `/simplify`, expanded to 9 parallel agents per round across two language surfaces:

- **Rounds 1–2**: TypeScript review + fix agents over `apps/web` (470 files)
- **Rounds 3–4**: Rust review + fix agents over `crates/services/` (with `/rust-code-review`, `/rust-best-practices`, `/rust-async-patterns` skills loaded)

All TypeScript fixes completed before the Rust review began (explicit user requirement).

---

## Timeline

| Phase | Action | Agents |
|-------|--------|--------|
| Round 1 | TS review — 9 agents, partitioned by domain across all 470 `apps/web` files | 9 parallel |
| Round 2 | TS fix — 9 agents, addressed all surfaced issues, deferred large refactors | 9 parallel |
| Round 3 | Rust review — 9 agents, reviewed all `crates/services/` files | 9 parallel |
| Round 4 | Rust fix — 9 agents, zero file overlap, fixed all actionable items | 9 parallel |

---

## Key Findings (Rust — crates/services/)

### Silent Error Swallowing
- `embed.rs`, `extract.rs`, `ingest.rs`: `unwrap_or(Value::Null)` on serialization silently discarded JSON errors from job result payloads. Changed to `.map(...).transpose()?` for proper propagation.

### Dead Code
- `acp/bridge.rs:278–297`: Turn-ID comparison check was unreachable — captured value from `Cell` and compared against same `Cell` with no `await` points between. Removed 13 lines.
- `acp_llm.rs`: `extract_completion_result` was an identity function (`fn f(x) -> x { x }`) after `AcpCompletionResponse` became a type alias for `AcpCompletionTurnResult`. Removed and inlined both call sites.
- `debug.rs`: `resolve_openai_model(cfg)` was a one-liner wrapper for `cfg.openai_model.clone()`. Removed wrapper; second call site changed to reference first result.

### TOCTOU + Blocking Syscall
- `acp/config.rs`: `read_codex_default_model()` and `read_gemini_default_model()` called blocking `path.exists()` in async context, then called `tokio::fs::read_to_string()` — TOCTOU window between the two. Fixed: eliminated `exists()` check entirely; now `tokio::fs::read_to_string()` direct with `ErrorKind::NotFound => return None` guard.

### Unbounded Scroll Risk (7M+ points)
- `graph.rs`: `qdrant_indexed_urls(cfg, None)` for domain-scoped graph builds had no limit on the `cortex` collection (7,063,563 points). Capped with `Some(GRAPH_BUILD_URL_LIMIT)`.

### Mutex Consolidation
- `acp/session_cache.rs`: Dual `Mutex<Vec<String>>` + `Mutex<usize>` guarding closely related state consolidated into single `Mutex<(Vec<String>, usize)>`. Eliminates potential inconsistency window between two separate lock acquisitions.

### Panic Risk in spawn_blocking
- `acp/persistent_conn.rs`: `.expect("[acp_conn] failed to build tokio runtime")` inside `spawn_blocking` would panic silently (swallowed by unmonitored `_join` handle). Changed to `match` with `tracing::error!` + early return.

### Duplicate Iteration
- `search.rs`: `research_payload` iterated `.skip(offset).take(limit)` twice on the same data to produce `extractions` and `search_results_json` separately. Merged into single pass with `.unzip()`.

---

## Technical Decisions

### spawn_blocking + Nested current_thread Runtime — NOT Fixed
`acp_llm::complete_text` and related functions use `?Send` ACP SDK traits (`#[async_trait(?Send)]`). Futures propagate `!Send` upward. The enclosing functions (`complete_text`, `complete_streaming` in `services/`) need to be `Send` for multi-threaded tokio runtime. The `spawn_blocking` + `current_thread` runtime pattern is the correct architectural solution — NOT a code smell here. Dropped as a fix candidate after analysis.

### Deferred: Large Structural Refactors
Per user instruction ("defer large refactors"), these were identified but not addressed:
- `serde_json::Value` overuse in service result types (should be typed structs)
- `Box<dyn Error>` everywhere (should be typed error enums at internal boundaries)
- `Display` / `Serialize` divergence on `AcpSessionUpdateKind`
- Missing `Serialize` derives on some result types

---

## Files Modified

| File | Change |
|------|--------|
| `crates/services/embed.rs` | `unwrap_or(Value::Null)` → `.map(...).transpose()?` in `embed_status` |
| `crates/services/extract.rs` | Same fix in `extract_status` |
| `crates/services/ingest.rs` | Same fix in `ingest_status` |
| `crates/services/acp/bridge.rs` | Removed dead turn-ID check (13 lines, lines ~278–297) |
| `crates/services/acp/config.rs` | Replaced blocking `path.exists()` with async read + NotFound guard in both model readers |
| `crates/services/graph.rs` | Capped `qdrant_indexed_urls(cfg, None)` with `Some(GRAPH_BUILD_URL_LIMIT)` |
| `crates/services/acp_llm.rs` | Removed `extract_completion_result` identity fn; inlined both call sites |
| `crates/services/acp/session_cache.rs` | Consolidated dual mutex → single `Mutex<(Vec<String>, usize)>` |
| `crates/services/debug.rs` | Removed trivial `resolve_openai_model` wrapper; reference reuse |
| `crates/services/acp/persistent_conn.rs` | `.expect()` → `match` + `tracing::error!` + early return |
| `crates/services/search.rs` | Merged duplicate iteration → single `.unzip()` pass in `research_payload` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --lib` | Clean | Clean (0 errors) | ✅ PASS |
| `cargo clippy --lib` | 0 warnings | 0 warnings | ✅ PASS |
| `cargo test services` | All pass | All pass | ✅ PASS |
| `cargo test --lib` | All pass | 2 pre-existing errors in `crates/web/execute/sync_mode/` | ⚠️ PRE-EXISTING |

---

## Pre-Existing Issue (Not Introduced)

`crates/web/execute/sync_mode/` has compilation errors:
- `dispatch_acp_event` function signature mismatch (`Option<&str>` vs `&str`)
- Missing imports for `parse_pipe_delimited_args` / `is_fatal_adapter_error`

These predate this session and are on the `feat/pulse-shell-and-hybrid-search` branch. Not related to `crates/services/` changes.

---

## Risks and Rollback

| Change | Risk | Rollback |
|--------|------|---------|
| Mutex consolidation in session_cache.rs | Low — all 13 session_cache tests pass | `git revert` the single commit |
| Removing dead turn-ID check | Low — check was unreachable (no await between capture and compare) | `git revert` |
| Removing identity fn in acp_llm.rs | Negligible — pure refactor, no logic change | `git revert` |
| TOCTOU fix in acp/config.rs | Low — behavior unchanged for existing files; missing files now handled by single io::Error | `git revert` |
| Graph scroll cap | Medium — graph builds on very large domains will now stop earlier | Remove `Some(GRAPH_BUILD_URL_LIMIT)` |

---

## Decisions Not Taken

- **`spawn_blocking` + nested runtime removal**: Initially flagged as unnecessary complexity, but analysis revealed it is architecturally required for `?Send` ACP trait bounds. Dropped.
- **`Box<dyn Error>` → typed error enums**: Identified as improvement but classified as large refactor. Deferred per user instruction.
- **`serde_json::Value` → typed result structs**: Same — deferred as large refactor.
- **`AcpSessionUpdateKind` Display/Serialize unification**: Deferred.

---

## Open Questions

- What is the intended value for `GRAPH_BUILD_URL_LIMIT`? Currently a constant in `graph.rs` — should it be configurable via env var like `AXON_SUGGEST_INDEX_LIMIT`?
- The pre-existing `crates/web/execute/sync_mode/` compilation errors — when will those be addressed?

---

## Next Steps

1. Fix pre-existing `crates/web/execute/sync_mode/` compilation errors (function signature mismatch + missing imports)
2. Commit the `crates/services/` fixes as a single clean commit
3. Address deferred large refactors in a separate PR: typed errors, `serde_json::Value` elimination, `Serialize` derives
4. Consider making `GRAPH_BUILD_URL_LIMIT` env-var configurable
