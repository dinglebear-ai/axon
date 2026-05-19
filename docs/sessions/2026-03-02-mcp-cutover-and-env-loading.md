# Session Documentation — MCP Cutover + Deterministic .env Loading

## 1) Session Overview
- Converted MCP startup from standalone `axon-mcp` binary to first-class `axon mcp` subcommand.
- Updated CLI wiring, dispatch, docs, and mcporter/CI invocation paths to `axon` + `mcp` args.
- Investigated and fixed mcporter smoke-test failures (path resolution and env resolution behavior).
- Implemented deterministic `.env` loading in `main.rs` so launcher cwd no longer controls env discovery.
- Verified with cargo checks and mcporter smoke tests (pass).

## 2) Timeline
- Confirmed architecture: MCP runtime already shared at `crates/mcp/server.rs:132-137` (`run_stdio_server`).
- Added `mcp` command wiring across parser/config/dispatch (`crates/core/config/*`, `lib.rs`, `crates/cli/commands/mcp.rs`).
- Removed standalone MCP entrypoint (`mcp_main.rs`) and `axon-mcp` bin stanza in `Cargo.toml:13-17`.
- Cut over mcporter/CI config and docs; initial smoke test failed due `ENOENT` and env-loading context mismatch.
- Fixed `config/mcporter.json:4-5` command path and implemented deterministic env loading in `main.rs:1-70`.

## 3) Key Findings
- MCP runtime was already centralized and reusable (`crates/mcp/mod.rs:5`, `crates/mcp/server.rs:132-137`).
- `mcporter` resolves stdio `command` relative to config location; `./target/debug/axon` from `config/mcporter.json` pointed to non-existent path, causing `spawn ... ENOENT`.
- Cwd-based dotenv loading created launcher-dependent behavior; explicit launch-context path search fixed this (`main.rs:3-24`, `main.rs:26-57`).
- `axon --help` now advertises MCP directly (`crates/core/config/help.rs`, observed output: `mcp  Start MCP stdio server`).
- Full mcporter smoke coverage now passes without manual env sourcing.

## 4) Technical Decisions
- Use one shared MCP runtime implementation and two thin layers (CLI command wiring + server handlers) rather than duplicate MCP startup logic.
- Remove standalone MCP binary to enforce single invocation path (`axon mcp`) and reduce drift.
- Keep artifact directory semantics unchanged (`.cache/axon-mcp`) to avoid downstream contract churn.
- Add deterministic dotenv behavior in binary instead of requiring wrappers.

## 5) Files Modified (created/updated/deleted) and Purpose
- `crates/core/config/cli/mod.rs` — added `CliCommand::Mcp`.
- `crates/core/config/types/enums.rs` — added `CommandKind::Mcp` + `as_str` mapping.
- `crates/core/config/parse/build_config.rs` — mapped `CliCommand::Mcp` to `CommandKind::Mcp`.
- `crates/cli/commands/mcp.rs` (new) — added `run_mcp` command runner.
- `crates/cli/commands.rs`, `lib.rs` — wired MCP command export/import/dispatch.
- `Cargo.toml` — removed `axon-mcp` bin stanza.
- `mcp_main.rs` (deleted) — removed standalone MCP entrypoint.
- `scripts/axon-mcp` (deleted) — removed standalone MCP launcher wrapper.
- `config/mcporter.json` — switched command to `../target/debug/axon` + args `mcp`.
- `.github/workflows/ci.yml` — switched MCP smoke job to `axon` binary + `mcp` args.
- `README.md`, `CLAUDE.md`, `docs/MCP.md`, `docs/MCP-TOOL-SCHEMA.md`, `crates/mcp/README.md`, `crates/mcp/CLAUDE.md`, `crates/README.md`, `scripts/mcp_doc_renderer.py` — invocation/docs alignment.
- `main.rs` — deterministic dotenv loading (`AXON_ENV_FILE`, current_exe/cwd ancestor search, fallback dotenv).

## 6) Commands Executed (critical)
- `cargo check --locked --bin axon` → passed after MCP wiring and server adjustments.
- `cargo check --locked --all-targets` → passed.
- `cargo run --locked --bin axon -- --help | rg -n "mcp"` → reported `mcp  Start MCP stdio server`.
- `./scripts/test-mcp-tools-mcporter.sh` (initial) → `PASS=2 FAIL=21`.
- `./scripts/test-mcp-tools-mcporter.sh` (after config/env fixes) → `PASS=23 FAIL=0`.
- `(cd /tmp && mcporter --config <tmp> call axon.axon action:status --output json)` → succeeded with JSON status output.
- `mcporter list` → only `axon` and `plate` servers available; no Neo4j memory server present.

## 7) Behavior Changes (Before/After)
- **Before:** MCP startup required separate `axon-mcp` binary path.
  **After:** MCP startup is `axon mcp` via primary CLI path.
- **Before:** mcporter config path `./target/debug/axon` under `config/` resolved incorrectly (`ENOENT`).
  **After:** `../target/debug/axon` resolves correctly.
- **Before:** dotenv behavior depended heavily on launcher cwd.
  **After:** binary attempts deterministic `.env` resolution from explicit env path and launch context ancestors.

## 8) Verification Evidence
| command | expected | actual | status |
|---|---|---|---|
| `cargo check --locked --bin axon` | Build succeeds | Succeeded | ✅ |
| `cargo check --locked --all-targets` | Project checks succeed | Succeeded | ✅ |
| `cargo run --locked --bin axon -- --help | rg -n "mcp"` | Help lists `mcp` | `50:  mcp  Start MCP stdio server` | ✅ |
| `./scripts/test-mcp-tools-mcporter.sh` (final) | MCP smoke passes | `PASS=23 FAIL=0` | ✅ |
| `mcporter ... action:status` from `/tmp` | Works without cwd `.env` dependency | JSON response returned | ✅ |
| `mcporter list` | Neo4j-memory MCP available for graph capture | Only `axon` and `plate` listed | ⚠️ unavailable |
| `axon embed "docs/sessions/2026-03-02-mcp-cutover-and-env-loading.md" --json` | Embed enqueued/completed | `{"job_id":"de72c9df-2299-42b2-b08b-972f6c7e17a0","status":"pending",...}` then `embed status` reported `status=completed` | ✅ |
| `axon retrieve "rust" --collection "cortex"` | Retrieve confirms indexed content from status-derived source id | `No content found for URL: rust` | ⚠️ partial |
| `axon retrieve "docs/sessions/2026-03-02-mcp-cutover-and-env-loading.md" --collection "cortex"` | Fallback verification on file-path source id | `Retrieve Result ... Chunks: 1` | ✅ |

## 9) Source IDs + Collections Touched
- Embed job `464c00e1-9654-4e1e-91ce-bb459c435789` completed in collection `cortex` for the same markdown path (first post-save embed attempt).
- Embed job `de72c9df-2299-42b2-b08b-972f6c7e17a0` completed with `status=completed`; status payload exposed `result_json.source=rust` and `result_json.collection=cortex`.
- Retrieve attempt with status-derived pair (`rust`, `cortex`) returned no content.
- Fallback retrieve using source id `docs/sessions/2026-03-02-mcp-cutover-and-env-loading.md` + collection `cortex` succeeded.

## 10) Risks and Rollback
- Risk: tools/scripts hardcoded to removed `axon-mcp` entrypoint may break.
- Risk: deterministic dotenv search may load unintended ancestor `.env` if repository layout is unusual.
- Rollback: restore `[[bin]] axon-mcp`, restore `mcp_main.rs`/`scripts/axon-mcp`, revert `main.rs` dotenv loader to prior behavior.

## 11) Decisions Not Taken
- Keep dual entrypoints (`axon mcp` + `axon-mcp`) — rejected to enforce single canonical startup path.
- Require wrapper scripts for env determinism — rejected in favor of binary-level deterministic loading.
- Rename artifact directory away from `.cache/axon-mcp` — rejected to avoid external contract changes.

## 12) Open Questions
- Should dotenv ancestor search be constrained to dirs containing `Cargo.toml` with package `axon` to avoid accidental parent `.env` picks?
- Should we update `skills/axon/SKILL.md` wording from "axon-mcp" to "axon mcp" for consistency?
- Do we want a deprecation alias command/script for transitional users still invoking `axon-mcp`?
- Neo4j memory capture was requested by skill instructions, but no `neo4j-memory` MCP server/tools were available in this runtime (`mcporter list` showed only `axon` and `plate`).

## 13) Next Steps
- Optionally tighten dotenv search root heuristic.
- Optionally update any remaining out-of-repo docs/examples mentioning legacy `axon-mcp` startup.
- Keep MCP smoke in CI as gate for invocation regressions.
