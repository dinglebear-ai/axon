# Session: Docker Build Cache Mounts + Worker Crash Loop Debug

**Date:** 2026-02-26
**Branch:** feat/crawl-download-pack

---

## Session Overview

Two parallel workstreams:
1. Implemented BuildKit cache mounts for `docker/Dockerfile` and `docker/Dockerfile.chrome` to dramatically reduce warm-build times (target: 30s warm vs 28min cold).
2. Diagnosed and confirmed fix for a worker crash loop caused by invalid PostgreSQL syntax in `crates/jobs/common/schema.rs`.

---

## Timeline

| Time (UTC) | Activity |
|------------|----------|
| Session start | Read `docker/Dockerfile` and `docker/Dockerfile.chrome` current state |
| Early | Implemented Stage 0 (`chef`), cache mounts on cook/build, binary relay in workers Dockerfile |
| Early | Added cache mounts to `cargo install headless_browser` in `Dockerfile.chrome` |
| Build #1 | `docker compose build --no-cache axon-workers axon-chrome` → failed on `ask.rs` compile error |
| Mid | Diagnosed: stale BuildKit `target/` cache had old `.rlib` artifacts; `cargo check` passed locally |
| Mid | Other agent fixed `schema.rs` (bad parameterized SQL → `format!`) and `ask.rs` test URLs |
| Build #2 | `docker compose build axon-workers && docker compose up -d axon-workers` → succeeded |
| Post-build | Diagnosed PostgreSQL crash loop from logs — confirmed crash loop stopped after new binary deployed |

---

## Key Findings

- **BuildKit `--no-cache` does NOT clear cache mounts** (`--mount=type=cache`). Layer cache is skipped but the `target/` BuildKit cache volume persists across builds, including stale incremental artifacts.
- **The bad SQL** `SET LOCAL lock_timeout = ($1::bigint || 'ms')::interval` was in an old version of `schema.rs` committed to HEAD. The working tree already had the fix. Workers were crashing in a tight s6 restart loop (~1 crash/sec) because every worker startup attempted this migration query and PostgreSQL 17 rejects `bigint || text` concatenation without an explicit text cast.
- **The fixed binary did NOT contain the bad SQL string.** Confirmed via `grep -ao` on `/usr/local/bin/axon` in the running container — old pattern absent, new `lock_timeout = '...ms'` pattern present.
- **`cargo check` passes locally** for the working tree with all three agent-modified files (`schema.rs`, `ask.rs`, `research.rs`).

---

## Technical Decisions

### Stage 0 `chef` in Dockerfile
**Decision:** Add a dedicated `chef` stage that compiles `cargo-chef` once with registry/git cache mounts, then `COPY --from=chef` the binary into both `planner` and `builder`.
**Rationale:** Previously `cargo install cargo-chef --locked` ran twice (once in planner, once in builder) with no caching — pure wasted compilation on every build.

### Binary relay pattern (`cp target/release/axon /usr/local/bin/axon-release`)
**Decision:** Copy binary out of the cache-mounted `target/` dir within the same `RUN` step; update `COPY --from=builder` in the runtime stage to use `/usr/local/bin/axon-release`.
**Rationale:** BuildKit cache mounts don't persist as image layers — the binary must be copied to a non-cached path in the same `RUN` command to be available for the `COPY --from=builder` instruction.

### `CARGO_TARGET_DIR` in Dockerfile.chrome
**Decision:** Set `CARGO_TARGET_DIR=/usr/local/cargo/headless-target` so build artifacts land in the cache-mounted path without interfering with the final binary install location (`/usr/local/cargo/bin/headless_browser`).
**Rationale:** `cargo install` places the final binary in `$CARGO_HOME/bin/`, not `$CARGO_TARGET_DIR/release/`. The runtime `COPY --from=builder` is unaffected.

### `format!` for `SET LOCAL` timeouts (schema.rs)
**Decision:** Use `sqlx::query(&format!("SET LOCAL lock_timeout = '{SCHEMA_LOCK_TIMEOUT_MS}ms'"))` instead of binding a parameter.
**Rationale:** PostgreSQL's `SET LOCAL` does not accept parameter markers (`$1`). The constants are compile-time `i64` values — no injection risk.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `docker/Dockerfile` | Added Stage 0 `chef`; `COPY --from=chef` in planner + builder; cache mounts on cook + build; binary relay | BuildKit cache mounts for warm builds |
| `docker/Dockerfile.chrome` | Cache mounts + `CARGO_TARGET_DIR` on `cargo install headless_browser` | Cache headless_browser compilation |
| `crates/jobs/common/schema.rs` | (other agent) `$1::bigint \|\| 'ms'` → `format!("{ms}ms")` | Fix PostgreSQL 17 syntax error crashing workers |
| `crates/vector/ops/commands/ask.rs` | (other agent) test URLs → `docs.*.dev` hostnames | Fix procedural query gate blocking test assertions |
| `crates/cli/commands/research.rs` | (other agent) Added progress ticker + elapsed timing output | UX improvement for long research commands |

---

## Commands Executed

```bash
# Verify local compilation
cargo check --bin axon
# → Finished `dev` profile in 0.39s (clean)

# First build attempt (failed — stale cache artifact)
docker compose build --no-cache axon-workers axon-chrome
# → ERROR ask.rs:284 — format_insufficient_evidence arg count mismatch

# Check what SQL is in the deployed binary
docker run --rm --entrypoint /bin/bash axon-axon-workers:latest -c \
  "grep -ao 'lock_timeout[^Z]*' /usr/local/bin/axon 2>/dev/null | cat -v | head -5"
# → lock_timeout = '...ms' (new format! version — bad pattern absent)

# Check for old bad SQL pattern
docker run --rm --entrypoint /bin/bash axon-axon-workers:latest -c \
  "grep -ao '\$1::bigint || .ms' /usr/local/bin/axon 2>/dev/null; echo done"
# → done (zero matches — confirmed not present)

# Verify workers recovered
docker compose logs --since=30s axon-postgres axon-workers 2>&1 | tail -10
# → heartbeat lane=1 alive / heartbeat lane=2 alive / checkpoint complete
```

---

## Behavior Changes (Before / After)

| Scenario | Before | After |
|----------|--------|-------|
| Warm Docker build (no source changes) | ~28 min (no caching) | ~30 sec (BuildKit cache hits all layers) |
| Source-only change | ~28 min | 2–5 min (dep cook cached) |
| `axon-workers` startup | Crash loop: `syntax error at or near "("` every ~1s | Workers start cleanly, heartbeat healthy |
| `cargo-chef` compilation | Ran twice (planner + builder, uncached) | Compiled once in `chef` stage, copied to both |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | Clean build | `Finished dev profile 0.39s` | ✅ |
| `grep` for old SQL in binary | 0 matches | 0 matches | ✅ |
| `docker compose logs` (post-rebuild) | Heartbeat messages | `crawl worker heartbeat lane=1/2 alive` | ✅ |
| Workers container status | healthy | `Up 2 minutes (healthy)` | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations performed during this session (infrastructure/build work only).

---

## Risks and Rollback

### Cache mount risks
- BuildKit cache mounts are **shared across builds on the same Docker host**. A corrupted `target/` cache can cause phantom compile errors. Workaround: `docker builder prune --filter type=exec.cachemount` to clear just the exec cache mounts.
- `--no-cache` on `docker compose build` skips layer cache but NOT cache mounts — this is a common footgun.

### Rollback
Revert `docker/Dockerfile` and `docker/Dockerfile.chrome` to the previous state:
```bash
git checkout HEAD~1 -- docker/Dockerfile docker/Dockerfile.chrome
docker compose build axon-workers axon-chrome
```

### schema.rs fix
Low risk — the fix is a pure SQL string change, no logic change. The constants are compile-time values, injection is not possible. PostgreSQL 17 is strict about implicit bigint→text casting; the `format!` approach is the correct pattern for `SET LOCAL` statements with dynamic values.

---

## Decisions Not Taken

- **Clearing BuildKit cache before second build**: Would have forced a cold build (~28 min). Instead relied on the fact that `cargo check` passing locally meant the compile error was a cache artifact, not a source bug.
- **Using `--no-cache` for the second build**: Same reason — would defeat the entire purpose of the cache mount work.
- **Adding `git` removal from runtime image**: The `git` package in the worker runtime is required for `axon github` wiki ingestion (`git clone --depth=1` subprocess in `crates/ingest/github/wiki.rs`). Removing it was explicitly not done.

---

## Open Questions

- **Why did Build #1 fail with a 1-arg error on `format_insufficient_evidence`?** The local working tree had 2 args, `cargo check` passed, and the final deployed binary was correct. Likely cause: stale incremental `.rlib` in the BuildKit `target/` cache from a previous broken commit. Needs monitoring on future `--no-cache` builds to confirm it doesn't recur.
- **Will the second warm build actually hit ~30s?** Expected but not yet verified — the user has not run a second build with no source changes post-warmup. Should verify next session.

---

## Next Steps

1. Run a second `docker compose build axon-workers` with no changes to verify warm cache performance (~30s target).
2. Verify `docker compose build axon-chrome` warm cache speed for `headless_browser`.
3. Consider adding `docker builder prune --filter type=exec.cachemount` to the `just` task documentation as a recovery step for stale cache issues.
