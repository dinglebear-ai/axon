# 2026-03-12 Service Layer Migration v2

## Scope Completed

This session finalized the v2 service-layer migration tasks, including CLI refresh scheduling rewires and web execute async ingest rewires, then ran the full verification/hardening checklist.

## Bypasses Removed

- Rewired refresh scheduler dispatch path to use `services::refresh::refresh_start` in `crates/cli/commands/refresh/schedule/run_due.rs`.
- Rewired GitHub refresh enqueue path to use `services::ingest::ingest_start` in `crates/cli/commands/refresh/github.rs`.
- Removed web execute ingest subprocess fallback behavior by routing `github`, `reddit`, and `youtube` through direct async service dispatch in `crates/web/execute/async_mode.rs`.
- Updated async mode classification in `crates/web/execute/constants.rs` and `crates/web/execute.rs` so ingest async modes are treated as direct async service modes.

## File Splits Performed

- Split `crates/cli/commands/refresh/schedule.rs` into:
  - `crates/cli/commands/refresh/schedule/add.rs`
  - `crates/cli/commands/refresh/schedule/run_due.rs`
  - `crates/cli/commands/refresh/schedule/worker.rs`
- Kept `schedule.rs` as the orchestrator/entrypoint module and shared helpers.

## Guard Suites Run

- `cargo test services_migration_tests --lib`
- `cargo test mcp_contract_parity --test mcp_contract_parity`
- `cargo test cli_full_rewire_smoke --test cli_full_rewire_smoke`
- `cargo test ws_protocol_tests --lib`
- `cargo test async_ingest_routing_tests --lib`

All above passed.

## Compile + Regression Checks Run

- `cargo check --bin axon`
- `cargo test tests::cli_ -- --nocapture`
- `cargo test tests::mcp_ -- --nocapture`

All above passed.

## Monolith Checks Run

- `python3 scripts/enforce_monoliths.py --file crates/cli/commands/refresh/schedule.rs`
- `python3 scripts/enforce_monoliths.py --file crates/mcp/server/handlers_system.rs`
- `python3 scripts/enforce_monoliths.py --file crates/web/execute/async_mode.rs`

All passed (with `async_mode.rs` function-length warning only, under hard limit).

## Notes

- Added/updated routing guard assertions in refresh/web tests to lock in service-layer dispatch expectations.
- No `.monolith-allowlist` changes were required.
- Post-migration hook hygiene follow-up fixed `clippy::collapsible_if` (`crates/crawl/engine.rs`), reduced OAuth helper `Err` payload size in `handlers_protected.rs` via boxed responses, and removed `await_holding_lock` in OAuth tests by dropping `ENV_LOCK` before async checks.
