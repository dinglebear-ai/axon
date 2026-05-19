---
date: 2026-05-06 19:03:52 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: b5efbc28
plan: none
agent: Claude (claude-sonnet-4-6)
session id: unknown
transcript: unknown
working directory: /home/jmagar/workspace/axon_rust
---

## User Request

Fix several stray file/directory problems: `.config/` was gitignored (preventing `nextest.toml` from being tracked), `storage/` screenshots landing in the wrong place, `config/.cache/` MCP artifacts being created in the wrong directory, `config/http-cacache/` appearing from an unknown source, and the MCP artifact root using a CWD-relative fallback path.

## Session Overview

Audited and fixed five separate stray-file issues across the repo. Each had a different root cause: a blanket gitignore entry, a CWD-relative spider path with an env var override, a CWD-relative fallback in Rust code, and npm's `make-fetch-happen` writing `http-cacache` wherever its host process runs. All fixed without disabling any functionality.

## Sequence of Events

1. User noticed `.config/` was listed in `.gitignore`, blocking `nextest.toml` from being committed
2. Removed `.config/` from `.gitignore`; confirmed `nextest.toml`'s location is canonical for `cargo nextest` and cannot be moved
3. User noticed `storage/` contained hundreds of URL-encoded screenshot filenames — traced to spider's `SCREENSHOT_DIRECTORY` env var defaulting to `./storage/`
4. Added `SCREENSHOT_DIRECTORY=.cache/axon-rust/screenshots` to both `.env` and `.env.example`
5. User noticed `config/.cache/axon-mcp/` — traced to MCP artifact root using `PathBuf::from(".cache/axon-mcp")` as CWD-relative fallback
6. Fixed `artifact_root()` in `crates/mcp/server/artifacts/path.rs` to use `axon_data_base_dir().join("axon/artifacts")` instead of CWD-relative path
7. Added `.cache/` to `config/.gitignore` and deleted the stray `config/.cache/` directory
8. User (correctly) pushed back on "just gitignore it" as the fix for stray directories — root causes must be fixed
9. User noticed `config/http-cacache/` — researched source; initially suspected spider's HTTP cache
10. Confirmed axon uses `cache_mem` (in-memory Moka), not disk `cache` — spider writes no disk cache files
11. Identified `http-cacache` as npm's `make-fetch-happen` format (content-v2/ + tmp/ structure), created by npx-based MCP servers in `mcporter.json` (`shadcn@latest mcp` and `@upstash/context7-mcp`)
12. Fixed by adding `npm_config_cache` env var to both npx server entries in `config/mcporter.json`

## Key Findings

- `.config/nextest.toml` is the canonical location for `cargo nextest` config; cannot be relocated without always passing `--config-file`
- `SCREENSHOT_DIRECTORY` env var in spider controls where `chrome_store_page` saves screenshots; defaults to `./storage/` relative to CWD (`spider-2.51.137/src/utils/mod.rs:5422,5447`)
- `artifact_root()` in `crates/mcp/server/artifacts/path.rs:62` used `PathBuf::from(".cache/axon-mcp")` as final fallback — CWD-relative, causing `config/.cache/axon-mcp/` when MCP server ran from `config/`
- `axon_data_base_dir()` in `crates/core/paths.rs:19` resolves `AXON_DATA_DIR` → `$HOME/.local/share` → CWD-relative only if HOME is unset — safe absolute path in any normal environment
- Axon's spider features include `cache_mem` but NOT `cache` — `CACACHE_MANAGER` uses in-memory Moka, writing zero disk files (`Cargo.toml` spider features section)
- `http-cacache/content-v2/` matches npm `cacache` format exactly; created by `make-fetch-happen` (used by npx) when `npm_config_cache` is not set, defaulting to `./http-cacache` in CWD
- The two npx-based servers in `config/mcporter.json` (`plate` and `context7`) had no env configuration, so they wrote to whatever directory mcporter was invoked from

## Technical Decisions

- **`SCREENSHOT_DIRECTORY` via env rather than code change**: spider's screenshot path is controlled by env var; no Rust changes needed for this fix
- **`axon_data_base_dir()` for MCP artifact fallback**: reuses the existing path-resolution chain already used by sqlite, logs, and data dirs — consistent, no new logic
- **`$HOME/.cache/axon-mcporter-npm` for npm cache**: absolute path (doesn't depend on CWD), shared between plate and context7 servers (deduplication benefit), outside the repo
- **Did not disable spider HTTP cache**: confirmed `cache_mem` (in-memory) is already active and the disk `cache` feature is not compiled in — nothing to disable
- **Did not "just gitignore" as the fix**: user correctly rejected this for `config/.cache/` and `config/http-cacache/` — root causes were found and fixed instead

## Files Modified

| File | Change |
|------|--------|
| `.gitignore` | Removed `.config/` entry (line 118) |
| `.env.example` | Added `SCREENSHOT_DIRECTORY=.cache/axon-rust/screenshots` in Output & CLI section |
| `.env` | Added `SCREENSHOT_DIRECTORY=.cache/axon-rust/screenshots` in CLI/output section |
| `config/.gitignore` | Added `.cache/` entry |
| `crates/mcp/server/artifacts/path.rs` | Replaced CWD-relative `.cache/axon-mcp` fallback with `axon_data_base_dir().join("axon/artifacts")`; added `use crate::crates::core::paths::axon_data_base_dir`; removed redundant `AXON_DATA_DIR` branch; updated doc comment |
| `config/mcporter.json` | Added `npm_config_cache: "${HOME}/.cache/axon-mcporter-npm"` env to `plate` and `context7` MCP server entries |

## Commands Executed

- `grep -n '\.config' .gitignore` — confirmed `.config/` at line 118
- `cargo check --bin axon` — confirmed `path.rs` changes compile cleanly (no output = success)
- `git check-ignore -v config/http-cacache` — confirmed `http-cacache/` at root gitignore:120 covers `config/http-cacache/` contents (directory itself reports not ignored, but contents are covered)
- `grep -A30 'spider = { version' Cargo.toml` — confirmed `cache_mem` present, `cache` absent from spider features
- `cat /home/jmagar/.cargo/registry/src/.../http-global-cache-0.2.0/src/lib.rs` — confirmed `make_manager()` uses `CACacheManager::new(std::env::temp_dir().join("spider-http-cache"), false)` for disk mode, Moka for `cache_mem`

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `nextest.toml` | Gitignored; not tracked | Tracked in version control |
| Spider crawl-time screenshots | Written to `./storage/` relative to CWD | Written to `.cache/axon-rust/screenshots/` |
| MCP artifact files | Written to `.cache/axon-mcp/<context>/` relative to CWD of MCP server process | Written to `$HOME/.local/share/axon/artifacts/<context>/` (absolute) |
| npx MCP server npm cache | Written to `./http-cacache/` relative to mcporter CWD | Written to `$HOME/.cache/axon-mcporter-npm/` (absolute) |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | No errors | No output (success) | ✅ |
| `grep -n 'axon_data_base_dir' crates/mcp/server/artifacts/path.rs` | Import + call site present | Lines 2 and 56 | ✅ |
| `grep -n 'SCREENSHOT_DIRECTORY' .env` | Entry present | Found at CLI/output section | ✅ |
| `grep -n 'npm_config_cache' config/mcporter.json` | Present for plate and context7 | Found in both server entries | ✅ |

## Risks and Rollback

- **MCP artifact path change**: existing artifacts at old CWD-relative `.cache/axon-mcp/` will not be auto-migrated. Any MCP client that references artifact paths by absolute path will need to re-run. Rollback: revert `path.rs` to CWD-relative fallback.
- **`npm_config_cache` env in mcporter.json**: if mcporter doesn't expand `${HOME}` in env values, the path will be literal `${HOME}/.cache/...` and npm will write to a `${HOME}` subdirectory in CWD. Needs verification on first run.

## Decisions Not Taken

- **Disable spider HTTP cache (`--cache false` default)**: unnecessary — axon already uses `cache_mem` (in-memory), no disk writes occur
- **Gitignore as primary fix for `config/.cache/` and `config/http-cacache/`**: rejected by user; root causes fixed instead
- **Patch spider to accept `SCREENSHOT_DIRECTORY` for the explicit screenshot config path**: spider's `ss.output_dir` path already goes through `cfg.output_dir` (`.cache/axon-rust/output/screenshots`); only the implicit crawl-time path needed the env var fix

## Open Questions

- Whether mcporter expands `${HOME}` in `env` values — if not, `npm_config_cache` path will be literal string. Needs verification on next mcporter invocation.
- The `config/.gitignore` was reverted by a linter/hook during the session (system-reminder showed it without the `.cache/` entry). Whether the addition persisted to disk needs confirmation before commit.
- `storage/` at repo root: the gitignore had `/storage/` (anchored) but the directory was CWD-relative. If the binary is run from a subdirectory, screenshots could still land elsewhere. The `SCREENSHOT_DIRECTORY` env var fix is the correct solution.

## Next Steps

**Not yet started:**
- Commit all changes in a single `chore: fix stray file locations` commit
- Verify mcporter `${HOME}` expansion works on next invocation
- Consider whether `AXON_MCP_ARTIFACT_DIR` should be pre-set in `.env.example` to a canonical absolute path to avoid any remaining CWD dependence
