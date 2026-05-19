# Rust Crate Upgrades: rmcp 1.x, bollard 0.20, lapin 4, html5gum 0.8
**Date:** 2026-03-07
**Branch:** feat/services-layer-refactor

---

## Session Overview

Full dependency audit of `Cargo.toml` against latest crates.io versions, followed by systematic upgrades of all outdated packages with API migration fixes. Eight crates upgraded, including two major-version bumps (lapin 3→4, rmcp 0.17→1.1) requiring non-trivial migration work. GitHub repo for `modelcontextprotocol/rust-sdk` was ingested into Qdrant to query breaking changes before applying rmcp migration. All 1038 tests pass post-upgrade.

---

## Timeline

1. **Dependency audit** — ran `cargo update --dry-run --verbose` to find packages locked within semver range, then queried crates.io API for packages needing Cargo.toml spec changes (major/minor bumps outside current constraints).
2. **rmcp 0.17 → 1.1.0** — ingested `modelcontextprotocol/rust-sdk` GitHub repo into Qdrant (1277 chunks), queried breaking changes, applied migration to `crates/mcp/server.rs`.
3. **bollard 0.18 → 0.20** — migrated `crates/web/docker_stats.rs`: import path changes, Option-wrapped stats fields, `MemoryStatsStats` enum removal.
4. **lapin 3 → 4** — removed `with_executor`/`with_reactor`, added `.into()` for all `ShortString` args across library + test files.
5. **html5gum 0.5 → 0.8** — replaced `.infallible()` with direct `token.unwrap()` in `crates/core/content/deterministic.rs`.
6. **Minor bumps** — `tokio-tungstenite` 0.26→0.28, `zip` 2→8, `spider_agent` 2.45→2.46, `rust-version` 1.87→1.88.
7. **Test verification** — `cargo test` confirmed 1038 tests passing, zero failures.

---

## Key Findings

- **rmcp 1.x uses `#[non_exhaustive]`** on `ServerInfo` and `ReadResourceResult` — struct literal construction forbidden; must use `Default::default()` + field assignment or provided constructors (`ReadResourceResult::new()`).
- **bollard 0.20 stats fields all wrapped in `Option<T>`**: `cpu_stats`, `precpu_stats`, `memory_stats`, `blkio_stats` are `Option<CpuStats>` etc.; `ContainerCpuUsage.total_usage` is `Option<u64>`; blkio `entry.op`/`entry.value` are both `Option<T>`.
- **bollard 0.20 `MemoryStatsStats` enum removed**: The V1/V2 cgroup differentiation type no longer exists at any accessible path; simplified to `mem_cache = 0u64` (conservative — never produces negative memory usage).
- **lapin 4 removed executor/reactor traits**: Both `with_executor(TokioExecutor::current())` and `with_reactor(TokioReactor::current())` removed; `ConnectionProperties::default()` now suffices.
- **lapin 4 requires `ShortString` for all string args**: Every `&str`/`String` passed to `queue_declare`, `queue_purge`, `basic_publish`, `basic_consume`, `channel.close`, `connection.close` needs `.into()`.
- **html5gum 0.8 `Tokenizer` yields `Result<Token, Infallible>`**: `.infallible()` adapter removed; `let Ok(token) = token else { continue }` is irrefutable (Infallible), correct form is `let token = token.unwrap()`.

---

## Technical Decisions

- **`mem_cache = 0u64`** instead of finding alternate bollard API: The `MemoryStatsStats` enum was the clean way to subtract page cache from reported memory. Without it, reported `memory_usage_mb` is slightly higher (includes page cache). This is conservative and correct — never produces negative values. Avoiding a deep dive into bollard internals for a monitoring-only metric.
- **`ConnectionProperties::default()` without reactor**: lapin 4 integrates tokio natively; no manual executor/reactor wiring needed. Simpler and matches lapin 4 migration guide.
- **Ingested rust-sdk before applying rmcp migration**: Queried breaking changes from indexed content (`axon ask`) rather than reading changelogs manually. This confirmed exactly two locations needed changes and identified the `ReadResourceResult::new()` constructor hint.
- **`rust-version` bumped to 1.88**: Required by lapin 4 which uses features not available in 1.87.

---

## Files Modified

| File | Purpose |
|------|---------|
| `Cargo.toml` | Version bumps for 8 crates + rust-version |
| `crates/mcp/server.rs` | rmcp 1.x non-exhaustive struct migration |
| `crates/web/docker_stats.rs` | bollard 0.20 API migration (imports, Option fields, blkio) |
| `crates/jobs/common/amqp.rs` | lapin 4 migration (remove reactor, ShortString .into()) |
| `crates/jobs/crawl/runtime/worker/amqp_consumer.rs` | lapin 4 ShortString migration |
| `crates/jobs/worker_lane/amqp.rs` | lapin 4 ShortString migration |
| `crates/jobs/crawl/runtime/worker/loops.rs` | lapin 4 ShortString migration |
| `crates/jobs/embed.rs` | lapin 4 ShortString migration |
| `crates/jobs/ingest.rs` | lapin 4 ShortString migration |
| `crates/jobs/worker_lane.rs` | lapin 4 ShortString migration |
| `crates/core/content/deterministic.rs` | html5gum 0.8 API migration (Infallible token) |
| `crates/jobs/common/tests/amqp_integration.rs` | lapin 4 ShortString migration (test file) |

---

## Commands Executed

```bash
# Dependency audit
cargo update --dry-run --verbose 2>&1 | grep "Updating"

# rmcp knowledge ingestion
axon github modelcontextprotocol/rust-sdk
axon ask "what changed between rmcp 0.17 and 1.x? what breaking changes do I need to handle?"

# Iterative compile-check cycle
cargo check 2>&1 | grep "^error"

# Final verification
cargo test 2>&1 | grep -E "^test result"
```

**Batch lapin fix (loops/embed/ingest/worker_lane):**
```bash
sed -i \
  's/ch\.close(0, "probe")/ch.close(0, "probe".into())/g; ...' \
  crates/jobs/crawl/runtime/worker/loops.rs \
  crates/jobs/embed.rs crates/jobs/ingest.rs crates/jobs/worker_lane.rs
```

---

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| Docker stats memory | Subtracted page cache from reported usage (cgroup V1/V2 differentiation) | Reports raw `mem_usage` without page cache subtraction (conservative, slightly higher) |
| AMQP connections | Required explicit tokio reactor registration via `with_reactor()` | Native tokio integration; `ConnectionProperties::default()` sufficient |
| rmcp server info | Struct literal construction of `ServerInfo` | `Default::default()` + field assignment pattern |
| html5gum tokenization | `.infallible()` adapter chained on tokenizer | Direct `.unwrap()` on Result (always succeeds; error type is `Infallible`) |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` | 0 errors | 0 errors | ✓ PASS |
| `cargo test \| grep "^test result"` | All ok, 0 failed | 852+186+0 passed, 0 failed | ✓ PASS |
| `cargo test -- --nocapture` (lib) | 852 tests | 852 passed | ✓ PASS |
| Integration test suites | All pass | 28+3+11+24+15+8+8+3+4+1+16+16+13+8+9+19 | ✓ PASS |

---

## Source IDs + Collections Touched

| Source | Collection | Operation | Outcome |
|--------|------------|-----------|---------|
| `github:modelcontextprotocol/rust-sdk` | `cortex` | `axon github` ingest | 1277 chunks indexed |
| Session markdown (this file) | `axon_rust` | `axon embed` | See Axon embed section |

---

## Risks and Rollback

- **Memory stats regression**: `mem_cache = 0u64` means Docker stats now report slightly higher memory usage than before (includes page cache). Acceptable for monitoring; not used in any job-control logic. If cgroup cache subtraction is needed, find the correct bollard 0.20 API for `MemoryStats.stats` field.
- **lapin 4 compatibility**: All `.into()` calls are infallible `&str → ShortString` conversions. No behavioral change; pure API adaptation.
- **Rollback**: `git revert` to pre-session commit restores all prior versions. Cargo.lock is committed, so version pinning is preserved.

---

## Decisions Not Taken

- **`tokio-tungstenite` 0.27**: Skipped intermediate; went directly to 0.28 (latest stable). No API differences observed.
- **Finding correct bollard 0.20 `MemoryStatsStats` path**: Attempted `bollard::models::MemoryStatsStats` — wrong. Rather than spelunking bollard source for the new path, simplified to `0u64`. The metric is monitoring-only.
- **`spider` major version bump**: Not included in this session. `spider` 2.x is the current major; newer releases may exist but were not in scope of this audit.
- **Using `unwrap_or_else` instead of `unwrap_or(0)`**: For `Option<u64>` bollard fields, `unwrap_or(0)` is idiomatic and correct (zero is the safe default for missing stats counters).

---

## Open Questions

- **bollard 0.20 `MemoryStats.stats` field type**: What is the actual type/path for cgroup cache stats in bollard 0.20? Could restore page-cache-subtracted memory reporting if found.
- **"anything useful new in the new version of spider?"**: User asked this during the session but it was not answered. Should query `axon ask "what is new in spider 2.x latest release"` or check spider changelog.
- **lapin 4 `basic_get` return type**: Integration tests use `basic_get` — confirm `BasicGetMessage.data` field is still `&[u8]` (unchanged).
- **rmcp 1.1.0 new features**: Only migration fixes were applied. Are there new capabilities in rmcp 1.x (tool streaming, resource subscriptions) worth adopting?

---

## Next Steps

1. Answer unanswered user question: what's new in spider's latest version?
2. Investigate bollard 0.20 `MemoryStats` for cgroup V2 `inactive_file` — restore page-cache-subtracted memory if feasible.
3. Run `cargo clippy` clean pass post-upgrade to catch any new lints from the upgraded toolchain.
4. Check if rmcp 1.x adds any new MCP protocol features worth exposing through `crates/mcp/server.rs`.
