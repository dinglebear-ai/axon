# Session: Source Display Fix, Rustls Panic Fix, and Codebase Tightening
**Date:** 2026-02-26
**Branch:** `feat/crawl-download-pack`
**Duration:** ~2 hours

---

## Session Overview

Three bug fixes and two code quality improvements:

1. **`axon ask` showed local file paths instead of URLs in sources** — manifest lookup in `source_display.rs` used the wrong JSON key (`file_path`) while real crawl manifests write `relative_path`. Fixed in both `ops/` and `ops_v2/`.
2. **Ingest worker crashed with rustls crypto provider panic** — `lapin` and `octocrab` compile both `ring` and `aws-lc-rs` into the same rustls 0.23 binary. Fixed with `install_default()` in both binary entry points.
3. **Tightened up in-progress branch changes** — dead `"error"` status arm in watchdog detection, and `is_low_signal_source_url` false-positive on legitimate web URLs containing `/logs/`.

---

## Timeline

| Time | Activity |
|------|----------|
| Start | User observed `axon ask` sources showing `.cache/axon-rust/output/...` paths instead of URLs |
| +15 min | Traced bug to `manifest_url_for_file()` in `ops/source_display.rs:31` using `"file_path"` key; manifests use `"relative_path"` |
| +30 min | Fixed both `ops/source_display.rs` and `ops_v2/source_display.rs`; added test |
| +45 min | User pasted ingest worker panic from Docker logs — rustls `CryptoProvider` panic |
| +60 min | Ran `cargo tree --invert` to confirm both `ring` (via `lapin`) and `aws-lc-rs` (via `octocrab`/`spider`) compiled into same rustls instance |
| +75 min | Added `rustls::crypto::aws_lc_rs::default_provider().install_default()` to `main.rs` and `mcp_main.rs`; added `rustls` as direct Cargo dep |
| +90 min | User asked "anything else to tighten up?" — reviewed all unstaged changes on branch |
| +100 min | Fixed dead `"error"` status arm in `status.rs` watchdog detection |
| +110 min | Fixed `is_low_signal_source_url` false-positive on web URLs with `/logs/` in path |
| End | 400 tests passing, 0 clippy warnings |

---

## Key Findings

### Source Display Bug
- `ops/source_display.rs:31` — `parsed.get("file_path")` never matches because manifests (written by `crates/crawl/engine/collector.rs`) use `"relative_path"` key
- `ops_v2/source_display.rs:176` — same bug, same fix
- `tei_manifest.rs:29-34` already had the correct dual-key logic (handled both `relative_path` AND `file_path`) — the `source_display` functions were written independently and missed this
- When `code.claude.com` content appeared in sources, the manifest didn't exist locally (crawled on another machine), so the lookup always fell to the raw file path regardless of key name

### Rustls Panic
- Error: `rustls-0.23.36/src/crypto/mod.rs:249` — `get_or_install_provider()` panics when both providers compiled in and no default set
- `cargo tree --invert aws-lc-rs` and `cargo tree --invert ring` both showed the same root chains:
  - `ring`: `lapin` → `rustls-connector` → `rustls`
  - `aws-lc-rs`: `octocrab` + `spider`/`reqwest 0.12` → `hyper-rustls` → `rustls`
- **Why only ingest panicked:** ingest worker was the first code path to initiate a real HTTPS handshake (GitHub API via `octocrab`). Other workers use TEI/local services over plain HTTP, or hadn't processed a job yet
- `crates/ingest/github.rs:74` — `build_octocrab()` creates an `Octocrab` instance that internally builds a reqwest client, triggering TLS

### Status Command Dead Arm
- `status.rs:148` — `matches!(status, "failed" | "error")` — `"error"` is not a valid `JobStatus` variant (only `pending/running/completed/failed/canceled` exist). Dead code that could mislead future readers.

### Low-Signal URL False Positive
- `ask/context.rs:68` — `lower.contains("/logs/")` would exclude legitimate indexed web pages like `https://docs.datadoghq.com/logs/explorer/` from `ask` context
- Correct intent: filter LOCAL file paths that are axon session logs or cache files, not web URLs

---

## Technical Decisions

- **`aws-lc-rs` over `ring`** for rustls default: `aws-lc-rs` is FIPS-eligible, actively maintained, and already the provider used by `octocrab` and `spider`. Choosing `ring` would conflict with the majority of the dep tree.
- **`install_default()` in `main.rs` not `lib.rs`**: Belongs in the binary entry point, not library code. Library code calling `install_default()` would cause failures in tests and embedding contexts where the provider is already set.
- **`let _ = install_default()`**: Intentional — returns `Err` if already installed (normal in tests). Panicking here would be wrong.
- **Fix `source_display` not the embed path**: The right fix is at display time, not to change how URLs are stored in Qdrant. Stored paths remain as-is; display resolves them.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/vector/ops/source_display.rs` | `manifest_url_for_file()` — handle `relative_path` + `file_path` | Fix sources showing file paths instead of URLs |
| `crates/vector/ops_v2/source_display.rs` | `build_manifest_lookup()` — same dual-key fix | Same bug, newer module |
| `main.rs` | `rustls::crypto::aws_lc_rs::default_provider().install_default()` | Fix ingest worker panic |
| `mcp_main.rs` | Same `install_default()` call | Fix same panic in MCP binary |
| `Cargo.toml` | `rustls = { version = "0.23", features = ["aws-lc-rs"], default-features = false }` | Expose `rustls::crypto::aws_lc_rs` module |
| `crates/cli/commands/status.rs` | `"failed" \| "error"` → `== "failed"` in `is_watchdog_reclaimed_failure` | Remove dead `"error"` arm |
| `crates/vector/ops/commands/ask/context.rs` | Guard `/logs/` and `.log` patterns with `!is_web_url` | Prevent filtering legitimate web pages |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo test source_display` | 2 tests pass | `ok. 2 passed` | ✅ |
| `cargo test --lib` (after all fixes) | 400 tests pass | `ok. 400 passed; 0 failed` | ✅ |
| `cargo clippy --lib` | 0 warnings | `0 warnings` | ✅ |
| `cargo check --bin axon --bin axon-mcp` | Compiles | `Finished dev profile` | ✅ |

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|----------|--------|-------|
| `axon ask` sources for crawled content | Shows `.cache/axon-rust/output/domains/code.claude.com/.../0321-support-claude-com-...md` | Shows `https://support.claude.com/en/articles/12599426/...` |
| `axon ingest github <repo>` in workers | Panics: `Could not automatically determine the process-level CryptoProvider` | Processes normally |
| `axon status` watchdog-reclaimed failure detection | Matched `"error"` status (unreachable) | Only matches `"failed"` (correct) |
| `axon ask` with indexed `docs.datadoghq.com/logs/` | Would be filtered as "low signal" | Correctly included in context |

---

## Risks and Rollback

| Risk | Severity | Notes |
|------|----------|-------|
| `install_default(aws-lc-rs)` silently wins race if called after another provider | Low | `let _ =` means second caller gets `Err` and continues — both callers work |
| `relative_path` manifest entries joining with stale `base_dir` on different machine | Low | If manifest doesn't exist locally, `find_manifest_for_markdown()` returns `None` early — no path join attempted |
| `aws-lc-rs` introduces platform-specific build requirement (C compiler, CMake) | Medium | Already present in dep tree via spider/octocrab — no net change to build requirements |

**Rollback:** Revert `main.rs`, `mcp_main.rs`, and the `rustls` line in `Cargo.toml`. The `source_display` fixes are purely additive and safe to keep.

---

## Decisions Not Taken

- **Eliminate one crypto provider via Cargo features** — would require patching transitive deps (`lapin`, `octocrab`) to agree on one provider. `install_default()` is the standard workaround and requires no upstream changes.
- **Fix source display at embed time** (store the URL when embedding files) — correct long-term direction but requires schema migration and changes to embed pipeline. Out of scope for this session.
- **Use `ring` instead of `aws-lc-rs`** as the default provider — would conflict with octocrab/spider which pull in `aws-lc-rs`. `aws-lc-rs` is the modern choice.

---

## Open Questions

- **Container rebuild**: The fix is in code but `axon-workers` Docker image needs `docker compose build && docker compose up -d axon-workers` to actually deploy. The running container still has the panic.
- **`docs/sessions/` filter for `axon ask`**: The `is_low_signal_source_url` function excludes session logs from RAG context unless the query explicitly requests them. This is the right default but depends on the `query_requests_low_signal_sources` heuristic (keyword list) being comprehensive.
- **`ops_v2/source_display.rs` vs `ops/source_display.rs`**: Both modules coexist. The `ask` command uses `ops/`. When `ops_v2` becomes the primary path, the fix there will activate.

---

## Next Steps

1. **Rebuild workers container**: `docker compose build axon-workers && docker compose up -d axon-workers` — deploys the rustls fix
2. **Test `axon ask` source display**: Run a query against content crawled from a known domain, verify sources show HTTP URLs
3. **Re-test `axon ingest github <repo>`**: Confirm the panic is resolved in the rebuilt container
4. **Consider unifying `ops/` and `ops_v2/` source_display**: The duplicate modules carry maintenance burden; pick one and remove the other
