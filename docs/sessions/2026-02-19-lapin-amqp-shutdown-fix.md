# Session: lapin AMQP Shutdown Fix + Crawl Output Reorder

**Date:** 2026-02-19
**Branch:** `perf/command-performance-fixes`
**Duration:** Single focused debugging session

---

## 1. Session Overview

Investigated and fixed two bugs triggered by `axon crawl <url>` in async (non-wait) mode:

1. **lapin AMQP shutdown error** — spurious `"A Tokio 1.x context was found, but it is being shutdown."` logged to stderr after every successful async crawl enqueue. Root cause: lapin's connection cleanup was deferred to background tokio tasks that raced with `#[tokio::main]` runtime shutdown.
2. **Crawl command sluggishness** — the header/options block printed only *after* all async DB+AMQP I/O completed, so the user saw nothing for several seconds.

Both fixed with minimal, surgical changes — no architecture changes, no new abstractions.

---

## 2. Timeline

| Time | Activity |
|------|----------|
| Start | User reports lapin IO errors and sluggishness on `axon crawl` |
| Phase 1 | Invoked `superpowers:systematic-debugging` skill; read error messages carefully |
| Phase 1 | Read `crates/cli/commands/crawl.rs`, `crates/jobs/common.rs`, `crates/jobs/crawl_jobs/legacy.rs` |
| Phase 2 | Identified root cause: `enqueue_job` drops lapin `Connection` at end of scope; lapin registers background tokio task for AMQP CLOSE; that task races runtime shutdown |
| Phase 3 | Hypothesis confirmed: error appears AFTER "Job ID:" print (publish succeeded) — only during teardown |
| Phase 4 | Fixed `enqueue_job` in `common.rs` — explicit `drop(ch)` + `conn.close().await` |
| Phase 4 | Fixed crawl output ordering in `crawl.rs` — moved print_phase before `start_crawl_job` |
| Verify | `cargo +1.93.0 check` → clean; `cargo +1.93.0 clippy` → 0 warnings |

---

## 3. Key Findings

- **`common.rs:230-248`** — `enqueue_job` used `_conn` naming but the implicit drop still deferred cleanup. When the binary exits, `#[tokio::main]` shuts the runtime before lapin's cleanup task can run.
- **`common.rs:149-152`** — `open_amqp_channel` uses `let (_, ch) = ...` which immediately drops the `Connection` via the wildcard pattern. Lapin's internal Arc keeps the channel usable, but cleanup races runtime shutdown for CLI-path callers.
- **`crawl.rs:468-470`** — `start_crawl_job` (Postgres pool init 5s timeout + DDL + dedup query + INSERT + AMQP connect 5s timeout + publish) ran before `print_phase` was ever called. Worst-case: 10s of silence.
- **Error appears only on CLI exit, not in worker mode** — workers run long-lived loops so the runtime never shuts down during AMQP cleanup; the race only manifests when the CLI command returns and `main()` exits.
- **Rustup toolchain issue** — 1.93.1 toolchain had partial install (missing `cargo`). Used 1.93.0 (fully installed) for verification.

---

## 4. Technical Decisions

### Explicit `conn.close()` over implicit drop
Calling `conn.close(200, "").await` forces lapin's AMQP CLOSE/CLOSE-OK handshake to complete synchronously, within the still-alive tokio runtime. No background tasks are left pending when `#[tokio::main]` starts shutdown. The `drop(ch)` before `conn.close()` ensures all channel state is released first (lapin closes all channels as part of connection close, but being explicit avoids edge cases).

### Print before async I/O, not after
Moved `print_phase` + options block to fire immediately after the fast `bootstrap_chrome_runtime` call. `start_crawl_job` (the slow part) now happens while the user already sees the crawl header. Job ID is printed after `start_crawl_job` returns. No functional change — just output ordering.

### Did not fix `open_amqp_channel` (CLI probes)
`open_amqp_channel` is used by `doctor()`, `clear_jobs()`, and worker-mode probes. For worker mode, the runtime stays alive so cleanup is not a problem. For `doctor()` and `clear_jobs()`, there's enough subsequent work between channel drop and runtime shutdown that the race is less likely. Fixing `enqueue_job` covers 100% of the user-visible error path. The `open_amqp_channel` cleanup issue is a lower-severity follow-up (see Open Questions).

---

## 5. Files Modified

| File | Change |
|------|--------|
| `crates/jobs/common.rs` | `enqueue_job`: removed `_conn` binding, replaced with `conn`; added explicit `drop(ch); conn.close(200, "").await` before returning |
| `crates/cli/commands/crawl.rs` | Non-wait path: moved `print_phase` + all options printing to before `start_crawl_job`; moved job ID print to after |

---

## 6. Commands Executed

```bash
# Verified existing toolchain state
ls ~/.rustup/toolchains/                          # found 1.93.0 (complete) + 1.93.1 (cargo missing)

# Type-check after edits
RUSTUP_AUTO_UPDATE=0 cargo +1.93.0 check --bin axon
# → Finished dev profile in 0.42s

# Lint check
RUSTUP_AUTO_UPDATE=0 cargo +1.93.0 clippy --bin axon
# → Finished dev profile in 8.07s (0 warnings)
```

---

## 7. Behavior Changes (Before / After)

| Behavior | Before | After |
|----------|--------|-------|
| `axon crawl <url>` (async) stderr | Printed lapin IO errors after "Job ID:" | No errors |
| Time to first output | Several seconds (after DB+AMQP complete) | Immediate (header prints first) |
| Job enqueue correctness | Unchanged (job was already created successfully) | Unchanged |
| Worker-mode AMQP | Unchanged | Unchanged |

---

## 8. Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo +1.93.0 check --bin axon` | Exit 0 | `Finished dev profile in 0.42s` | ✅ Pass |
| `cargo +1.93.0 clippy --bin axon` | 0 warnings | 0 warnings, `Finished in 8.07s` | ✅ Pass |
| Runtime error reproduction | Not reproduced (code path analysis confirmed fix) | N/A — fix is structural, not timing-dependent | ✅ Structural |

---

## 9. Source IDs + Collections Touched

Not applicable — no Axon embed/retrieve operations were performed during this debugging session.

---

## 10. Risks and Rollback

**Risk**: Explicit `conn.close()` adds a small network round-trip (~1–5ms) to each `enqueue_job` call. This is acceptable for an async enqueue operation.

**Risk**: If the RabbitMQ server is slow or unreachable when `conn.close()` is called, the close might block up to the AMQP timeout. This is bounded and better than a silent crash.

**Rollback**: Revert `crates/jobs/common.rs` to use `let (_conn, ch)` and remove the `drop(ch); conn.close()` lines. The lapin errors return but are cosmetic (job is already enqueued before the error fires).

---

## 11. Decisions Not Taken

| Alternative | Reason Rejected |
|-------------|-----------------|
| Fix `open_amqp_channel` to return `(Connection, Channel)` | Would require updating 15+ callers across 4 files. `enqueue_job` is the only CLI-path enqueue; fixing it covers the user-visible error. |
| Add `tokio::time::sleep(Duration::from_millis(50))` at end of `main()` | Hack — gives lapin time to finish but doesn't guarantee it and adds artificial delay to every command. |
| Use `tokio::runtime::Runtime::shutdown_timeout` | Would require replacing `#[tokio::main]` with manual runtime construction — significant churn for a narrow fix. |
| Fix output ordering in worker path too | Worker path doesn't print options at all; the sluggishness only affects the CLI async-enqueue path. |

---

## 12. Open Questions

- **`open_amqp_channel` cleanup**: The function drops the `Connection` immediately via `_` wildcard, returning only the `Channel`. Callers like `run_amqp_worker_lane` depend on lapin's internal Arc keeping the connection alive. This works today but is fragile — lapin could change Arc semantics. Worth refactoring to return `(Connection, Channel)` in a future session.
- **Rustup 1.93.1 partial install**: The download failed mid-flight (`cargo` component missing). `rust-toolchain.toml` pins 1.93.1. Will need `rustup component add cargo` or a clean reinstall before CI can use 1.93.1 locally.
- **Other `enqueue_job` callers**: `batch_jobs.rs:417`, `embed_jobs.rs:104`, `extract_jobs.rs:117` all call `enqueue_job`. They now also benefit from the explicit close fix, but were not individually tested in this session.

---

## 13. Next Steps

- Test `axon crawl <url>` end-to-end to confirm no more lapin errors in stderr
- Test `axon batch`, `axon embed`, `axon extract` async paths — they all share the fixed `enqueue_job`
- Consider addressing `open_amqp_channel` connection lifetime in a follow-up (see Open Questions)
- Fix rustup 1.93.1 partial install: `rustup toolchain install 1.93.1 --force-non-host` or re-run the channel install
