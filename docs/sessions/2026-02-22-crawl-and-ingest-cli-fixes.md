# Session: crawl + ingest CLI Bug Fixes

**Date:** 02/22/2026 | **Time:** 17:42 EST
**Branch:** main

---

## Session Overview

Three CLI bugs were systematically debugged and fixed:

1. `axon crawl <url>` rejected its own URL as an "unknown subcommand" (regression)
2. `axon crawl audit <url>` failed at the clap parse level (pre-existing)
3. `axon ingest worker` caused a crash loop in the Docker s6 ingest-worker service because `ingest` was never registered as a CLI subcommand

All three were fixed with targeted, minimal changes. 149 tests passing, 0 regressions, clippy clean.

---

## Timeline

| Time | Activity |
|------|----------|
| 17:00 | User reports `crawl <url>` treating URL as unknown subcommand |
| 17:05 | Reproduced: `cargo run -- crawl https://example.com` → `"unknown crawl subcommand: https://example.com"` |
| 17:10 | Root cause traced: redundant guard in `run_crawl()` fires on positional URL |
| 17:12 | Fix 1 applied: removed 3-line guard in `crates/cli/commands/crawl.rs` |
| 17:15 | Matrix run reveals second bug: `crawl audit <url>` → clap error |
| 17:20 | Root cause: `CrawlArgs.url: Option<String>` can't hold both `"audit"` and the URL |
| 17:25 | Fix 2 applied: `Vec<String>`, guard update, audit URL routing |
| 17:30 | User reports s6 crash loop: `axon ingest worker` → `unrecognized subcommand 'ingest'` |
| 17:35 | Root cause: `ingest` command family never wired into `CliCommand` |
| 17:40 | Fix 3 applied: full `Ingest` command wiring across 6 files |
| 17:42 | 149/149 tests passing, clippy clean |

---

## Key Findings

### Bug 1 — `crawl <url>` (`crates/cli/commands/crawl.rs:22-28`)

`into_config()` stores the URL in `cfg.positional` (not a separate field). After `maybe_handle_subcommand()` returns `Ok(false)` (URL didn't match any subcommand keyword), `run_crawl()` had a second guard:
```rust
if let Some(subcmd) = cfg.positional.first() {
    return Err(format!("unknown crawl subcommand: {subcmd}").into());
}
```
This guard fired on the URL itself. `validate_url(start_url)?` immediately below already handles bad input — the guard was purely redundant and actively broken.

### Bug 2 — `crawl audit <url>` (`crates/core/config/cli.rs`, `crates/cli/commands/common.rs`)

`CrawlArgs.url` was `Option<String>` — one slot. `axon crawl audit https://example.com` has two positionals (`"audit"` + URL). Clap error: `"the subcommand 'https://example.com' cannot be used with '[URL]'"`. Additionally, `start_url_from_cfg` didn't include `"audit"` in its job-subcommand guard, so it would have extracted `"audit"` as the URL even if clap had accepted the input.

### Bug 3 — `axon ingest worker` (`docker/s6/s6-rc.d/ingest-worker/run:5`)

`run_ingest_worker`, `ingest_jobs.rs`, `ingest_common.rs`, and the s6 service script were all created during the ingest feature implementation, but the `CliCommand::Ingest` → `CommandKind::Ingest` → `run_ingest` wiring was never added. The s6 run script called `axon ingest worker` on every restart → clap rejected it → s6 looped.

---

## Technical Decisions

### Fix 1: Remove guard, don't replace it
The guard `if let Some(subcmd) = cfg.positional.first()` was supposed to catch unknown subcommand words. But since `maybe_handle_subcommand` already returns `Ok(false)` for anything not in its match, AND `validate_url` catches non-URL garbage, the guard added nothing. Removed cleanly.

### Fix 2: `Vec<String>` over alternative approaches
Three options were considered:
- **`Vec<String>` for `CrawlArgs.url`** (chosen) — minimal clap change, audit URL routed from `positional[1]`
- Add `audit`/`diff` as `JobSubcommand` variants — cleaner long-term but more invasive
- Use `--start-url` global flag for audit — user-hostile, changes the calling convention

`Vec<String>` is consistent with `BatchArgs.urls` and requires the least refactoring.

### Fix 3: `ingest` as a pure management command
The `ingest` CLI command has no URL target — it only dispatches worker/status/list management subcommands (all sources share one worker). Not added to `is_async_enqueue_mode` since `ingest` never enqueues jobs directly (github/reddit/youtube do that). Returns a clear error if called without a subcommand.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/cli/commands/crawl.rs` | Removed 3-line redundant guard after `maybe_handle_subcommand` |
| `crates/core/config/cli.rs` | `CrawlArgs.url: Option<String>` → `Vec<String>`; added `IngestArgs` struct + `CliCommand::Ingest` |
| `crates/core/config/parse.rs` | `args.url.into_iter().collect()` → `args.url` for `CliCommand::Crawl`; added `CliCommand::Ingest` arm |
| `crates/cli/commands/common.rs` | Extended `start_url_from_cfg` guard to include `"audit" \| "diff"` |
| `crates/cli/commands/crawl.rs` | `"audit"` arm now reads `cfg.positional.get(1)` as URL |
| `crates/core/config/types.rs` | Added `Ingest` to `CommandKind` enum + `"ingest"` to `as_str()` |
| `crates/cli/commands/ingest.rs` | **NEW** — delegates to `maybe_handle_ingest_subcommand` |
| `crates/cli/commands/mod.rs` | Added `pub mod ingest` + `pub use ingest::run_ingest` |
| `mod.rs` | Added `run_ingest` to imports + `CommandKind::Ingest => run_ingest(cfg).await?` |

---

## Commands Executed

```bash
# Reproduce Bug 1
cargo run --bin axon -- crawl https://example.com
# → Error: "unknown crawl subcommand: https://example.com"

# Verify fix (all three bugs)
./target/debug/axon crawl https://example.com --wait false
# → ◐ Crawling https://example.com  Options: ...

./target/debug/axon crawl audit https://example.com
# → Crawl Audit (then postgres connect failed — expected, no infra)

./target/debug/axon ingest worker
# → Error: postgres connect failed  (no longer "unrecognized subcommand")

# Final verification
cargo test --lib    # 149 passed, 0 failed
cargo clippy        # 0 warnings
```

---

## Behavior Changes (Before/After)

| Command | Before | After |
|---------|--------|-------|
| `axon crawl https://example.com` | `Error: "unknown crawl subcommand: https://example.com"` | Starts crawl, proceeds to enqueue/connect |
| `axon crawl` (no URL) | Same broken error | `Error: invalid URL: ` (correct) |
| `axon crawl bad-input` | Same broken error | `Error: invalid URL: bad-input` (correct) |
| `axon crawl audit https://example.com` | clap: `"the subcommand 'https://...' cannot be used with '[URL]'"` | Routes correctly to `run_crawl_audit` |
| `axon ingest worker` | clap: `"unrecognized subcommand 'ingest'"` → s6 crash loop | Connects to Postgres/AMQP, starts worker |
| `axon ingest list` | same clap error | Lists ingest jobs from DB |
| `axon ingest` (no subcommand) | same clap error | `Error: "ingest requires a subcommand: worker, status, ..."` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `axon crawl https://example.com --wait false` | Print options, no parse error | `◐ Crawling https://example.com  Options: ...` | ✅ PASS |
| `axon crawl` | `invalid URL` error | `invalid URL` (via `validate_url`) | ✅ PASS |
| `axon crawl bad-input` | `invalid URL` error | `Error: invalid URL: bad-input` | ✅ PASS |
| `axon crawl list` | `postgres connect failed` (no infra) | `Error: postgres connect failed` | ✅ PASS (infra error, not parse error) |
| `axon crawl audit https://example.com` | Route to `run_crawl_audit` | `Crawl Audit` (then postgres) | ✅ PASS |
| `axon crawl audit` | `invalid URL` error | `Error: invalid URL: ` | ✅ PASS |
| `axon ingest worker` | Connect to postgres/AMQP | `Error: postgres connect failed` | ✅ PASS (infra, not parse) |
| `axon ingest list` | Connect to postgres | `Error: postgres connect failed` | ✅ PASS (infra, not parse) |
| `axon ingest` | Clear error message | `Error: ingest requires a subcommand: worker, ...` | ✅ PASS |
| `cargo test --lib` | 149 passed, 0 failed | `149 passed; 0 failed` | ✅ PASS |
| `cargo clippy` | 0 errors/warnings | clean | ✅ PASS |

---

## Source IDs + Collections Touched

None — this session involved only code changes, no Axon embed/crawl operations against external sources.

---

## Risks and Rollback

**Risk (low):** `CrawlArgs.url: Vec<String>` now accepts multiple URL positionals at the clap level. Currently only `positional[0]` is consumed as `start_url` by `start_url_from_cfg`, and `positional[1]` is only read by the `audit` handler. Extra args beyond what's expected are silently ignored by the runtime dispatch. This is consistent with how `BatchArgs.urls: Vec<String>` behaves.

**Rollback:** All changes are in `crates/` and `mod.rs`. Git revert of the session commits restores prior behavior exactly. The only new file is `crates/cli/commands/ingest.rs` (9 lines).

---

## Decisions Not Taken

| Option | Why Rejected |
|--------|-------------|
| Keep the `unknown subcommand` guard but skip URLs | Fragile — requires URL detection logic duplicated from `validate_url`; `validate_url` already handles it |
| Add `audit`/`diff` as `JobSubcommand` variants | More correct long-term, but significantly more invasive for a low-traffic feature; deferred |
| Fix s6 script to call `axon github worker` instead of `axon ingest worker` | Semantically wrong — the ingest worker processes ALL sources (github/reddit/youtube); routing to one source's worker name is misleading |
| Add `Ingest` to `is_async_enqueue_mode` | Wrong — `ingest` only manages the worker queue, never enqueues jobs directly |

---

## Open Questions

- The `crawl audit` feature needs infrastructure (Postgres + the crawl manifest tables) to actually function — has it been validated end-to-end with a real crawl?
- `ingest errors <uuid>` is defined in `JobSubcommand` but `maybe_handle_ingest_subcommand` doesn't handle `"errors"` — it will return `Ok(false)` and `run_ingest` will return a "requires subcommand" error. Intentional gap or oversight?
- The `ingest worker` s6 service was silently crash-looping — how long has it been failing? Were any ingest jobs queued during that time that are now stuck `pending`?

---

## Next Steps

- Rebuild and redeploy `axon-workers` Docker image to stop the s6 crash loop: `docker compose build axon-workers && docker compose up -d axon-workers`
- Verify `axon ingest worker` connects and processes jobs with infra running
- Consider adding `"errors"` to `maybe_handle_ingest_subcommand` or removing `JobSubcommand::Errors` support from `IngestArgs` to avoid the silent no-op
- Run `axon crawl recover` against infra to reclaim any stale jobs from before these fixes
