# Session Overview

- Date: 2026-03-26
- Repository: `axon`
- Branch: `feat/lite-mode`
- Commit pushed: `9f57350c` `fix(mcp): validate smoke coverage across full and lite modes`
- Scope: normalize mcporter config on `axon`, harden MCP smoke coverage, repair screenshot/export/graph smoke handling, update docs, resolve pre-commit monolith/parser failures, and complete push workflow.

# Timeline Of Major Activities

- Updated mcporter-based MCP smoke coverage to validate the discovered surface and execute both full mode (`AXON_LITE=0`) and lite mode (`AXON_LITE=1`) through [`scripts/test-mcp-tools-mcporter.sh`](/home/jmagar/workspace/axon_rust/scripts/test-mcp-tools-mcporter.sh).
- Tightened smoke assertions from exit-code-only checks to route-specific payload checks, which exposed failures in `export`, `screenshot`, and `graph` routes before subsequent fixes.
- Normalized mcporter local config to `axon` in [`config/mcporter.json`](/home/jmagar/workspace/axon_rust/config/mcporter.json) and documented the new behavior across README and MCP/testing docs.
- Resolved commit-blocking hook failures by fixing the schema doc parser in [`scripts/mcp_schema_models.py`](/home/jmagar/workspace/axon_rust/scripts/mcp_schema_models.py), splitting oversized Rust files, and repairing the interrupted refresh service split.
- Staged, committed, and pushed all current tracked work on `feat/lite-mode`, then saved this session document for Axon embedding.

# Key Findings

- `mcporter` local config had drifted from docs and scripts: the configured server name was `axon-axon`, while examples expected `axon`. This was fixed in [`config/mcporter.json`](/home/jmagar/workspace/axon_rust/config/mcporter.json).
- The MCP smoke harness had been reporting success on exit code alone; stricter JSON assertions exposed real route issues before fixes landed in [`scripts/test-mcp-tools-mcporter.sh`](/home/jmagar/workspace/axon_rust/scripts/test-mcp-tools-mcporter.sh).
- `action:help` omitted advertised routed capabilities until [`crates/mcp/server/handlers_system.rs`](/home/jmagar/workspace/axon_rust/crates/mcp/server/handlers_system.rs) was updated to surface `graph` and `refresh_schedule`.
- The schema doc generator treated compatibility `subaction: Option<String>` fields as missing enums; the parser fix lives in [`scripts/mcp_schema_models.py`](/home/jmagar/workspace/axon_rust/scripts/mcp_schema_models.py).
- Pre-commit monolith enforcement required splitting [`crates/cli/commands/common.rs`](/home/jmagar/workspace/axon_rust/crates/cli/commands/common.rs) and [`crates/cli/commands/serve_supervisor.rs`](/home/jmagar/workspace/axon_rust/crates/cli/commands/serve_supervisor.rs), and repairing the interrupted refresh split into [`crates/services/refresh.rs`](/home/jmagar/workspace/axon_rust/crates/services/refresh.rs) and [`crates/services/refresh_schedule.rs`](/home/jmagar/workspace/axon_rust/crates/services/refresh_schedule.rs).

# Technical Decisions And Rationale

- Used mcporter-discovered schema plus `action:help` as the smoke source of truth, because client-visible discovery is the contract real MCP consumers depend on.
- Tested both lite and full mode in the same harness, because `export` and `graph:*` are intentionally unavailable in lite mode and should be asserted differently there.
- Forced repo-local writable runtime paths in mcporter config to avoid environment-specific runtime drift and readonly path failures.
- Preserved the service-layer contract by keeping refresh schedule behavior routed through `services::refresh` / `services::refresh_schedule`, then updated stale compatibility tests to follow the moved implementation rather than weaken the check.
- Accepted a patch version bump (`0.33.0 -> 0.33.1`) because the finalized commit prefix was `fix(...)`.

# Files Modified/Created And Purpose

- [`config/mcporter.json`](/home/jmagar/workspace/axon_rust/config/mcporter.json): normalized local mcporter server registration to `axon` and stabilized stdio runtime paths.
- [`scripts/test-mcp-tools-mcporter.sh`](/home/jmagar/workspace/axon_rust/scripts/test-mcp-tools-mcporter.sh): dual-mode MCP smoke harness with discovery parity checks and payload invariants.
- [`crates/mcp/server/handlers_system.rs`](/home/jmagar/workspace/axon_rust/crates/mcp/server/handlers_system.rs): exposed `graph` and `refresh_schedule` in `action:help`.
- [`crates/crawl/screenshot.rs`](/home/jmagar/workspace/axon_rust/crates/crawl/screenshot.rs), [`crates/services/screenshot.rs`](/home/jmagar/workspace/axon_rust/crates/services/screenshot.rs), [`crates/mcp/server/handlers_system/screenshot.rs`](/home/jmagar/workspace/axon_rust/crates/mcp/server/handlers_system/screenshot.rs): repaired screenshot path behavior used by MCP smoke coverage.
- [`crates/services/refresh.rs`](/home/jmagar/workspace/axon_rust/crates/services/refresh.rs) and [`crates/services/refresh_schedule.rs`](/home/jmagar/workspace/axon_rust/crates/services/refresh_schedule.rs): completed the refresh service split required by monolith policy and hook compliance.
- [`crates/cli/commands/common.rs`](/home/jmagar/workspace/axon_rust/crates/cli/commands/common.rs), [`crates/cli/commands/common_jobs.rs`](/home/jmagar/workspace/axon_rust/crates/cli/commands/common_jobs.rs), [`crates/cli/commands/common_urls.rs`](/home/jmagar/workspace/axon_rust/crates/cli/commands/common_urls.rs): split oversized shared CLI helpers.
- [`crates/cli/commands/serve_supervisor.rs`](/home/jmagar/workspace/axon_rust/crates/cli/commands/serve_supervisor.rs), [`crates/cli/commands/serve_supervisor_model.rs`](/home/jmagar/workspace/axon_rust/crates/cli/commands/serve_supervisor_model.rs), [`crates/cli/commands/serve_supervisor_preflight.rs`](/home/jmagar/workspace/axon_rust/crates/cli/commands/serve_supervisor_preflight.rs), [`crates/cli/commands/serve_supervisor_runtime.rs`](/home/jmagar/workspace/axon_rust/crates/cli/commands/serve_supervisor_runtime.rs), [`crates/cli/commands/serve_supervisor_tests.rs`](/home/jmagar/workspace/axon_rust/crates/cli/commands/serve_supervisor_tests.rs): split oversized supervisor implementation for monolith compliance.
- [`README.md`](/home/jmagar/workspace/axon_rust/README.md), [`docs/TESTING.md`](/home/jmagar/workspace/axon_rust/docs/TESTING.md), [`docs/MCP.md`](/home/jmagar/workspace/axon_rust/docs/MCP.md), [`docs/auth/MCP-AUTH.md`](/home/jmagar/workspace/axon_rust/docs/auth/MCP-AUTH.md), [`crates/mcp/README.md`](/home/jmagar/workspace/axon_rust/crates/mcp/README.md), [`crates/mcp/CLAUDE.md`](/home/jmagar/workspace/axon_rust/crates/mcp/CLAUDE.md): updated docs for dual-mode mcporter smoke testing and normalized local server naming.

# Critical Commands Executed And Outcomes

- `bash scripts/test-mcp-tools-mcporter.sh`
  Outcome: final smoke result reported `PASS=152 FAIL=0`.
- `python3 scripts/generate_mcp_schema_doc.py`
  Outcome: succeeded and rewrote [`docs/MCP-TOOL-SCHEMA.md`](/home/jmagar/workspace/axon_rust/docs/MCP-TOOL-SCHEMA.md).
- `python3 scripts/enforce_monoliths.py --staged`
  Outcome: final result passed with warnings only.
- `cargo check --all-targets --locked`
  Outcome: passed after refresh/common/supervisor split repairs.
- `cargo clippy --all-targets --locked -- -D warnings`
  Outcome: passed after module wiring and type-complexity cleanup.
- `cargo test --all --locked`
  Outcome: passed with `1553 passed; 0 failed; 12 ignored` plus doc tests `0 passed; 0 failed; 6 ignored`.
- `git commit -m "fix(mcp): validate smoke coverage across full and lite modes" -m "Co-authored-by: Claude <noreply@anthropic.com>"`
  Outcome: passed all pre-commit hooks and created commit `9f57350c`.
- `git push`
  Outcome: pushed `9f57350c` to `origin/feat/lite-mode`.

# Behavior Changes

- Before: mcporter smoke testing assumed one mode and could report success on exit code alone.
  After: the harness validates both full and lite mode contracts with route-specific payload assertions.
- Before: local mcporter config and repo docs disagreed on the server name.
  After: local stdio examples and config use `axon`.
- Before: `action:help` did not fully reflect all advertised routed MCP capabilities.
  After: `graph` and `refresh_schedule` are surfaced in help output.
- Before: commit hooks failed on the schema parser, monolith policy, and an interrupted refresh split.
  After: schema generation, monolith enforcement, clippy, tests, and commit hooks all pass.

# Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `bash scripts/test-mcp-tools-mcporter.sh` | both lite/full suites pass | `PASS=152 FAIL=0` | PASS |
| `python3 scripts/enforce_monoliths.py --staged` | no hard file/function violations | warnings only; `Monolith policy check passed.` | PASS |
| `cargo check --all-targets --locked` | build succeeds | `Finished 'dev' profile` | PASS |
| `cargo clippy --all-targets --locked -- -D warnings` | no warnings/errors | `Finished 'dev' profile` | PASS |
| `cargo test --all --locked` | no failing tests | `1553 passed; 0 failed; 12 ignored`; doc tests `0 failed` | PASS |
| `git push` | branch updated remotely without force | `3fc64858..9f57350c  feat/lite-mode -> feat/lite-mode` | PASS |
| `./scripts/axon embed docs/sessions/2026-03-26-mcporter-dual-mode-and-hook-fixes.md --json` | embed succeeds and returns a job id | `{\"chunks_embedded\":13,\"collection\":\"axon\"}` then `{\"job_id\":\"0a7b70a6-deec-4e5c-90e9-59e3795da052\",\"status\":\"completed\"}` | PASS |
| `./scripts/axon embed status 0a7b70a6-deec-4e5c-90e9-59e3795da052 --json` | completed job exposes stored target/collection metadata | `status=completed`, `target=docs/sessions/2026-03-26-mcporter-dual-mode-and-hook-fixes.md`, `metrics.collection=axon` | PASS |
| `./scripts/axon retrieve "docs/sessions/2026-03-26-mcporter-dual-mode-and-hook-fixes.md" --collection axon` | retrieved indexed session document | `Chunks: 13` and session markdown content returned | PASS |

# Source IDs + Collections Touched

- MCP smoke harness exercised the local mcporter stdio server `axon`; no Axon collection/source IDs were captured during smoke runs in this session log.
- Session document embed job id: `0a7b70a6-deec-4e5c-90e9-59e3795da052`.
- Session document collection from `embed status`: `axon`.
- Session document source identifier actually used for retrieval: `docs/sessions/2026-03-26-mcporter-dual-mode-and-hook-fixes.md`.
- Observed CLI mismatch: local `embed status --json` did not emit a `data.url` field; retrieval was verified successfully with the embedded document path in collection `axon`.

# Risks And Rollback

- This commit includes broad repo changes already present in the worktree at commit time, including prompt deletions and image deletions shown by `git diff --stat HEAD`.
- `unwrap-warn` emitted warnings for new `.unwrap()` / `.expect()` usage in non-test Rust; the hook is warning-only, but those call sites remain a maintenance risk.
- Rollback path is standard git revert of `9f57350c`; no history rewrite or force push was used.

# Decisions Not Taken

- Did not keep the MCP smoke harness in single-mode operation; dual-mode testing was chosen to match the real full/lite contract split.
- Did not bypass hooks with `--no-verify`; hook failures were fixed directly.
- Did not use alternate writable directories to dodge file locks; cargo lock waits were respected and duplicate self-started cargo jobs were killed instead.
- Did not attempt Neo4j memory persistence through MCP because no Neo4j memory MCP tool was available in this session environment.

# Open Questions

- Whether the broad prompt deletions and image deletions included in `9f57350c` were intended product changes or accumulated worktree state; they were committed because they were present in the staged worktree at commit time.
- Whether the warning-only `.unwrap()` / `.expect()` additions should be cleaned up in a follow-up hardening pass.

# Next Steps

- Investigate why local `embed --json` / `embed status --json` returns a completed-job shape without the documented `data.job_id` / `data.url` envelope.
- If Neo4j memory tooling becomes available, capture commit/repository/session-doc entities and relations for this push.
