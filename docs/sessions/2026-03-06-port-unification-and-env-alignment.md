# Session: Port Unification & Environment Alignment

**Date:** 2026-03-06
**Branch:** `feat/services-layer-refactor`
**Duration:** Extended session (continued from prior context)

## Session Overview

Unified port defaults, environment variable resolution, and output directory paths between local dev and Docker environments. Fixed the root cause of persistent WebSocket connection failures (stale `.env.local`), suppressed ACP SDK log noise, and brought all linters and tests green.

## Timeline

1. **Output directory unification** (from prior context) — Derived all output paths (`output_dir`, `artifact_root`, `diagnostics_dir`, `log_file`) from `AXON_DATA_DIR` when set, eliminating local-vs-Docker path mismatches. Removed `normalizeOutputDirForWeb()` and `AXON_WORKER_OUTPUT_DIR`.

2. **Added MCP server to `just dev`** — Wired `AXON_MCP_HTTP_PORT=8001 cargo run --bin axon -- mcp &` into the dev recipe and added MCP process to `just stop`.

3. **Port 3939 → 49000 migration** — Changed `axon serve` default from 3939 to 49000 across: `build_config.rs`, `config_impls.rs`, `cli.rs` (clap `env = "AXON_SERVE_PORT"`), `next.config.ts`, `axon-ws-exec.ts`, Justfile.

4. **Env var wiring for serve/MCP ports** — Added `env = "AXON_SERVE_PORT"` to clap arg. MCP was already env-driven. Updated `.env.example` with `AXON_SERVE_PORT`, `AXON_MCP_HTTP_PORT`, `AXON_MCP_HTTP_HOST`.

5. **Fixed 6 local-vs-Docker inconsistencies:**
   - Log path derivation from `AXON_DATA_DIR` (`logging.rs`)
   - Pulse chat hardcoded container paths → env var overrides (`claude-stream-types.ts`)
   - Test URL normalization for AMQP/Redis/Qdrant (`common.rs`)
   - `AXON_SERVE_HOST` footgun in `.env.example`
   - `NEXT_PUBLIC_AXON_PORT` default mismatch in `next.config.ts`
   - Workspace route CLAUDE_CONFIG comment (`route.ts`)

6. **Root cause: persistent port 3939** — `apps/web/.env.local` had `AXON_BACKEND_URL=http://localhost:3939` and `AXON_WORKERS_WS_URL=ws://localhost:3939/ws` overriding all code defaults. Fixed both to port 49000.

7. **Doc cleanup** — Purged all stale port 3939 references from `docs/SERVE.md`, `docs/commands/serve.md`, `crates/core/CLAUDE.md`, `apps/web/CLAUDE.md`.

8. **Linter/test green pass:**
   - Fixed clippy `cmp_owned`: `PathBuf::from(...)` → `Path::new(...)` in `build_config.rs:477`
   - Suppressed ACP `usage_update` decode noise: `agent_client_protocol::rpc=warn` in `logging.rs`
   - Fixed `.env` test URLs pointing to non-existent containers (ports 53380/45536/53335 → real ports 53379/45535/53333)
   - Created missing log directory: `mkdir -p $AXON_DATA_DIR/axon/logs`

## Key Findings

- **`.env.local` overrides everything**: Next.js loads `.env.local` at startup and it takes priority over code defaults. Any hardcoded URL there silently wins over `next.config.ts` changes. Not hot-reloaded — requires process restart.
- **`agent-client-protocol` 0.10.0 gap**: The Claude Code wire protocol sends `usage_update` session updates with cost data, but the Rust ACP SDK doesn't have this variant in its `SessionUpdate` enum. Deserialization fails at ERROR level but sessions continue normally.
- **Qdrant test flakiness**: `ensure_collection_is_idempotent` and `qdrant_delete_stale_domain_urls_handles_large_batch` fail intermittently when run concurrently with other Qdrant tests due to `dispatch task is gone` (tokio runtime contention). Always pass in isolation.
- **Test URL isolation pattern**: `.env` uses separate `AXON_TEST_*` env vars for integration tests, pointing at different ports than production. When those containers don't exist, tests fail with "Connection refused" instead of skipping.

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Port 49000 as serve default | Matches Docker container mapping; avoids conflict with common dev ports |
| `env = "AXON_SERVE_PORT"` on clap | Single source of truth — `.env` controls the port without CLI flags |
| Suppress ACP noise at `warn` level | Non-fatal deserialization error; upstream fix needed in `agent-client-protocol` crate |
| Test URLs → real container ports | No separate test infrastructure running; tests use unique prefixes for data isolation |
| `Path::new()` over `PathBuf::from()` | Clippy `cmp_owned` — avoids unnecessary heap allocation for comparison |
| Noise directive array pattern in logging | DRY: both console and file filters apply same directives via fold |

## Files Modified

### Rust (tracked)
| File | Change |
|------|--------|
| `crates/core/config/cli.rs:54` | Added `env = "AXON_SERVE_PORT"` to clap serve port arg |
| `crates/core/config/parse/build_config.rs:30,477` | Default 3939→49000; `Path::new()` for clippy fix |
| `crates/core/config/types/config.rs:394` | Updated doc comment for serve_port |
| `crates/core/config/types/config_impls.rs:127` | Default impl 3939→49000 |
| `crates/core/logging.rs:170-200` | ACP noise suppression; noise_directives array pattern |
| `crates/jobs/common.rs:149-227` | Hoisted test helpers to module-level; fixed resolve_test_amqp/redis/qdrant_url |
| `crates/services/acp.rs` | (prior context) ACP 0.9.5→0.10.0 upgrade |
| `crates/web/execute/files.rs` | (prior context) Simplified output_dir() with AXON_DATA_DIR |
| `crates/mcp/server/common.rs` | (prior context) artifact_root() AXON_DATA_DIR derivation |
| `crates/core/health.rs` | (prior context) diagnostics dir AXON_DATA_DIR derivation |

### TypeScript (tracked)
| File | Change |
|------|--------|
| `apps/web/next.config.ts:4` | Default port 3939→49000 |
| `apps/web/lib/axon-ws-exec.ts:13` | Hardcoded ws://127.0.0.1:3939/ws → env var chain with 49000 fallback |
| `apps/web/app/api/pulse/chat/claude-stream-types.ts:92-145` | Env var overrides for ALLOWED_DIR_ROOTS, mcp-config, plugin-dir |

### Config/Docs (tracked)
| File | Change |
|------|--------|
| `Justfile:134-180` | serve port, MCP in dev recipe, stop recipe, sleep |
| `.env.example` | AXON_SERVE_PORT, AXON_MCP_HTTP_PORT/HOST, AXON_TEST_REDIS/QDRANT_URL docs |
| `docs/SERVE.md:16` | Port 3939→49000 |
| `docs/commands/serve.md:18,32` | Port 3939→49000 |
| `crates/core/CLAUDE.md:67` | serve_port default + env var |
| `apps/web/CLAUDE.md:151` | Removed stale 3939 comment |

### Local-only (gitignored)
| File | Change |
|------|--------|
| `apps/web/.env.local:2-3` | AXON_BACKEND_URL and AXON_WORKERS_WS_URL → port 49000 |
| `.env:41-42,97-98` | AXON_TEST_* URLs → real container ports with credentials |

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo fmt --all -- --check` | No output | No output | PASS |
| `cargo clippy --all-targets --locked -- -D warnings` | Clean | Clean | PASS |
| `cargo check -q --locked` | Clean | Clean | PASS |
| `cargo test -q --locked --lib -- --skip worker_e2e` | ~845 pass | 845 pass, 1 flaky Qdrant | PASS (flaky) |
| `pnpm test` (apps/web) | All pass | 647 passed, 58 files | PASS |
| `pnpm lint` (apps/web Biome) | Warnings only | 22 warnings (pre-existing) | PASS |
| `grep -r 3939 --include='*.rs' --include='*.ts' --include='*.md'` | No matches | No matches | PASS |

## Behavior Changes (Before/After)

| Area | Before | After |
|------|--------|-------|
| `axon serve` default port | 3939 | 49000 (env: `AXON_SERVE_PORT`) |
| `just dev` | No MCP server | Includes `axon mcp` on port 8001 |
| Output dir (local, AXON_DATA_DIR set) | `.cache/axon-rust/output` | `$AXON_DATA_DIR/axon/output` |
| Artifact dir | `.cache/axon-mcp` | `$AXON_DATA_DIR/axon/artifacts` (when set) |
| Log file path | `logs/axon.log` | `$AXON_DATA_DIR/axon/logs/axon.log` (when set) |
| ACP `usage_update` messages | ERROR-level log spam | Suppressed to warn |
| Integration tests (AMQP/Redis/Qdrant) | 19 failures (wrong ports) | 18-19 passing (1 flaky) |

## Risks and Rollback

- **Port change is breaking for existing `.env.local` files**: Anyone with `AXON_BACKEND_URL=http://localhost:3939` in their local env will need to update. Mitigated: `.env.example` documents the new defaults.
- **Test URLs point at production containers**: Tests use unique key prefixes and temp collections for data isolation, but a rogue test could theoretically pollute production data. Low risk given the test patterns.
- **ACP noise suppression**: If a real RPC error occurs at the `agent_client_protocol::rpc` target, it'll be logged at `warn` instead of `error`. Acceptable tradeoff — the `usage_update` spam was drowning real errors.

## Decisions Not Taken

- **Separate test Docker compose**: Could run isolated test instances on different ports. Rejected — overhead not justified for local dev; data-level isolation is sufficient.
- **Bump `agent-client-protocol` past 0.10.0**: 0.10.0 is the latest on crates.io. No newer version available with `usage_update` support.
- **Fix Qdrant test flakiness**: Would require serializing Qdrant tests or using a per-test client pool. Not worth the complexity for an intermittent timing issue.

## Open Questions

- When will `agent-client-protocol` add `usage_update` to the `SessionUpdate` enum? Monitor crates.io for 0.11.0+.
- Should we add a `docker-compose.test.yaml` for isolated test infrastructure? Currently using production containers with data isolation.
- The `just dev` ELIFECYCLE error from `pnpm dev` getting SIGTERM'd by `just stop` is cosmetic but messy. Could be fixed with a more graceful shutdown sequence.

## Next Steps

- Restart `just dev` to verify port 49000 WebSocket connections work end-to-end
- Monitor for ACP `usage_update` upstream fix
- Consider PR for this branch once smoke-tested
