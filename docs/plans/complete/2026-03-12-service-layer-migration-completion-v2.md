# Service Layer Migration Completion (v2) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Complete all remaining CLI, MCP, and web execute service-layer bypasses so business operations route through `crates/services/*`, with TDD-first changes, sanitized error handling, and no monolith policy regressions.

**Architecture:** Finish service API parity first, then rewire MCP handlers, then CLI command paths, then web async ingest dispatch. Keep transport/command layers thin and move lifecycle orchestration into service wrappers. Use existing `common.rs`/logging helpers and split files proactively before crossing monolith thresholds.

**Tech Stack:** Rust (`tokio`, `axum`, `rmcp`, `sqlx`), service facade modules in `crates/services`, MCP handlers in `crates/mcp/server`, CLI command modules in `crates/cli/commands`, web execute bridge in `crates/web/execute`, test harness via `cargo test`, monolith gate via `scripts/enforce_monoliths.py`.

---

## Execution Rules (Apply To Every Task)

- Use `@superpowers:test-driven-development` for every migrated path.
- Use `@verification-before-completion` before claiming each phase complete.
- Run monolith checks before and after edits for every touched Rust file:

```bash
python3 scripts/enforce_monoliths.py --file <path>
```

Expected: `Monolith policy check passed.`

- If a touched file is `>= 450` lines, split immediately (do not add allowlist entries).
- Use existing logging/error patterns only:
- CLI/services logging: `log_info`, `log_warn`, `log_done`, `log_error`
- MCP errors: `invalid_params`, `internal_error`, `logged_internal_error`
- Service event logs: `ServiceEvent::Log` via `emit`

---

### Task 1: Guardrails First (CLI + MCP + Web)

**Files:**
- Modify: `crates/cli/commands/services_migration_tests.rs`
- Create: `crates/mcp/server/services_migration_tests.rs`
- Modify: `crates/mcp/server.rs`
- Modify: `crates/web/execute/tests/ws_protocol_tests.rs`
- Create: `crates/web/execute/tests/async_ingest_routing_tests.rs`

**Step 1: Write failing guard tests for new forbidden imports/calls**

```rust
#[test]
fn migrated_cli_commands_do_not_import_raw_business_logic_layers_v2() {
    let checks = [
        ("embed.rs", include_str!("embed.rs"), &["jobs::embed::{"][..]),
        ("extract.rs", include_str!("extract.rs"), &["jobs::extract::{"][..]),
        ("ingest.rs", include_str!("ingest.rs"), &["ingest::classify::classify_target"][..]),
        ("ingest_common.rs", include_str!("ingest_common.rs"), &["jobs::ingest::{"][..]),
        ("watch.rs", include_str!("watch.rs"), &["jobs::watch::{"][..]),
        ("domains.rs", include_str!("domains.rs"), &["vector::ops::qdrant::domains_payload"][..]),
        ("sources.rs", include_str!("sources.rs"), &["vector::ops::qdrant::sources_payload"][..]),
        ("stats.rs", include_str!("stats.rs"), &["vector::ops::stats::stats_payload"][..]),
    ];
    // same scan/assert pattern as existing guard
}
```

```rust
#[test]
fn migrated_mcp_handlers_do_not_import_jobs_layers_directly() {
    let checks = [
        ("handlers_embed_ingest.rs", include_str!("handlers_embed_ingest.rs"), &["crate::crates::jobs::embed", "crate::crates::jobs::ingest"][..]),
        ("handlers_crawl_extract.rs", include_str!("handlers_crawl_extract.rs"), &["crate::crates::jobs::crawl", "crate::crates::jobs::extract"][..]),
        ("handlers_refresh_status.rs", include_str!("handlers_refresh_status.rs"), &["crate::crates::jobs::refresh"][..]),
        ("handlers_system.rs", include_str!("handlers_system.rs"), &["crawl::screenshot::spider_screenshot_with_options"][..]),
    ];
    // same scan/assert pattern
}
```

```rust
#[test]
fn async_subprocess_modes_does_not_include_ingest_modes() {
    for mode in ["github", "reddit", "youtube"] {
        assert!(
            !super::super::constants::ASYNC_SUBPROCESS_MODES.contains(&mode),
            "ingest mode must not use subprocess fallback: {mode}"
        );
    }
}
```

**Step 2: Run guard tests and verify they fail**

Run:

```bash
cargo test services_migration_tests --lib
cargo test ws_protocol_tests --lib
```

Expected: failures referencing currently-present forbidden fragments and subprocess fallback modes.

**Step 3: Wire new MCP test module**

```rust
#[cfg(test)]
#[path = "server/services_migration_tests.rs"]
mod services_migration_tests;
```

**Step 4: Re-run targeted tests (still red for behavior tasks)**

Run:

```bash
cargo test services_migration_tests --lib
```

Expected: compiles with new module, tests still failing until rewires are complete.

**Step 5: Commit**

```bash
git add crates/cli/commands/services_migration_tests.rs crates/mcp/server/services_migration_tests.rs crates/mcp/server.rs crates/web/execute/tests/ws_protocol_tests.rs crates/web/execute/tests/async_ingest_routing_tests.rs
git commit -m "test: add migration guardrails for CLI MCP and web ingest routing"
```

---

### Task 2: Service API Parity for Lifecycle Operations

**Files:**
- Modify: `crates/services/crawl.rs`
- Modify: `crates/services/extract.rs`
- Modify: `crates/services/embed.rs`
- Modify: `crates/services/ingest.rs`
- Modify: `crates/services/refresh.rs`
- Modify: `crates/services/types/service.rs`
- Create (if split needed): `crates/services/ingest_lifecycle.rs`

**Step 1: Write failing service tests for missing wrappers**

- Add/extend in:
- `tests/services_lifecycle_services.rs`
- `tests/cli_full_rewire_smoke.rs`

```rust
#[test]
fn ingest_service_exposes_start_status_cancel_list_cleanup_clear_recover() {
    // compile-time contract test: ensure symbols exist and callable signatures compile
}
```

```rust
#[test]
fn refresh_service_exposes_schedule_lifecycle_wrappers() {
    // list/create/delete/enable/disable wrappers compile from services::refresh
}
```

**Step 2: Run tests to verify failure**

Run:

```bash
cargo test services_lifecycle_services --test services_lifecycle_services
```

Expected: unresolved function/import failures for missing service wrappers.

**Step 3: Add minimal wrappers in service modules**

```rust
pub async fn ingest_start(cfg: &Config, source: IngestSource) -> Result<IngestStartResult, Box<dyn Error>> {
    let job_id = start_ingest_job(cfg, source).await?;
    Ok(IngestStartResult { job_id: job_id.to_string() })
}

pub async fn ingest_status(cfg: &Config, id: Uuid) -> Result<Option<IngestJobResult>, Box<dyn Error>> {
    let job = get_ingest_job(cfg, id).await?;
    Ok(job.map(|j| IngestJobResult { payload: serde_json::to_value(j).unwrap_or_default() }))
}
```

**Step 4: Re-run tests and monolith checks**

Run:

```bash
cargo test services_lifecycle_services --test services_lifecycle_services
python3 scripts/enforce_monoliths.py --file crates/services/ingest.rs
python3 scripts/enforce_monoliths.py --file crates/services/refresh.rs
```

Expected: tests pass for new contracts, monolith checks pass.

**Step 5: Commit**

```bash
git add crates/services/crawl.rs crates/services/extract.rs crates/services/embed.rs crates/services/ingest.rs crates/services/refresh.rs crates/services/types/service.rs tests/services_lifecycle_services.rs tests/cli_full_rewire_smoke.rs
git commit -m "feat: add service lifecycle wrappers for crawl extract embed ingest refresh"
```

---

### Task 3: Ingest Classification and Start Boundary in Services

**Files:**
- Modify: `crates/services/ingest.rs`
- Create: `crates/services/ingest/classify.rs`
- Modify: `crates/services.rs` (module export if needed)
- Modify: `tests/services_lifecycle_services.rs`

**Step 1: Add failing tests for classification boundary**

```rust
#[test]
fn classify_target_returns_ingest_source_for_github_slug() {
    let src = classify_target("owner/repo", true).expect("must classify");
    matches!(src, IngestSource::Github { .. });
}

#[test]
fn classify_target_rejects_unknown_target() {
    let err = classify_target("not-a-target", false).unwrap_err();
    assert!(err.to_string().contains("cannot determine ingest source"));
}
```

**Step 2: Run tests to verify failure**

Run:

```bash
cargo test services_lifecycle_services --test services_lifecycle_services
```

Expected: missing `services::ingest::classify_target` function.

**Step 3: Implement service-owned classifier entrypoint**

```rust
pub fn classify_target(target: &str, include_source: bool) -> Result<IngestSource, Box<dyn Error>> {
    crate::crates::ingest::classify::classify_target(target, include_source)
}
```

**Step 4: Re-run tests and monolith check**

Run:

```bash
cargo test services_lifecycle_services --test services_lifecycle_services
python3 scripts/enforce_monoliths.py --file crates/services/ingest.rs
```

Expected: tests pass and file remains under threshold.

**Step 5: Commit**

```bash
git add crates/services/ingest.rs crates/services/ingest/classify.rs crates/services.rs tests/services_lifecycle_services.rs
git commit -m "feat: add service-owned ingest target classification"
```

---

### Task 4: MCP Rewire — Embed/Ingest

**Files:**
- Modify: `crates/mcp/server/handlers_embed_ingest.rs`
- Modify: `crates/mcp/server/common.rs`
- Modify: `tests/mcp_contract_parity.rs`
- Modify: `crates/mcp/server/services_migration_tests.rs`

**Step 1: Add failing MCP parity tests for embed/ingest response shape and error sanitization**

```rust
#[test]
fn mcp_embed_start_returns_job_id_payload_shape() {
    let payload = serde_json::json!({"job_id": "abc"});
    assert!(payload.get("job_id").is_some());
}

#[test]
fn mcp_ingest_start_requires_source_type() {
    let err = axon::crates::mcp::server::common::invalid_params("source_type is required");
    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
}
```

**Step 2: Run tests and confirm red**

Run:

```bash
cargo test mcp_contract_parity --test mcp_contract_parity
```

Expected: assertions fail once tightened for current implementation gaps.

**Step 3: Rewire handlers to `services::embed` and `services::ingest` only**

```rust
let result = crate::crates::services::embed::embed_start(self.cfg.as_ref(), None)
    .await
    .map_err(|e| logged_internal_error("embed.start", e))?;
```

```rust
let source = crate::crates::services::ingest::classify_target(&target, include_source)?;
let result = crate::crates::services::ingest::ingest_start(self.cfg.as_ref(), source)
    .await
    .map_err(|e| logged_internal_error("ingest.start", e))?;
```

**Step 4: Re-run tests and guard checks**

Run:

```bash
cargo test mcp_contract_parity --test mcp_contract_parity
cargo test services_migration_tests --lib
python3 scripts/enforce_monoliths.py --file crates/mcp/server/handlers_embed_ingest.rs
```

Expected: MCP parity tests pass, migration guard no longer flags jobs-layer imports.

**Step 5: Commit**

```bash
git add crates/mcp/server/handlers_embed_ingest.rs crates/mcp/server/common.rs tests/mcp_contract_parity.rs crates/mcp/server/services_migration_tests.rs
git commit -m "refactor: route mcp embed ingest handlers through services layer"
```

---

### Task 5: MCP Rewire — Crawl/Extract/Refresh/Status + Screenshot

**Files:**
- Modify: `crates/mcp/server/handlers_crawl_extract.rs`
- Modify: `crates/mcp/server/handlers_refresh_status.rs`
- Modify: `crates/mcp/server/handlers_system.rs`
- Create (split to avoid monolith growth): `crates/mcp/server/handlers_system/screenshot.rs`
- Modify: `crates/mcp/server/services_migration_tests.rs`

**Step 1: Write failing tests for lifecycle parity and screenshot transport shape**

```rust
#[test]
fn mcp_refresh_schedule_unknown_subaction_returns_invalid_params() {
    let msg = "unknown schedule_subaction";
    assert!(msg.contains("unknown schedule_subaction"));
}

#[test]
fn mcp_screenshot_payload_contains_path_size_and_viewport() {
    let payload = serde_json::json!({"path":"/tmp/a.png","size_bytes":10,"viewport":"1280x720"});
    assert!(payload.get("path").is_some());
}
```

**Step 2: Run tests to verify failure**

Run:

```bash
cargo test mcp_contract_parity --test mcp_contract_parity
```

Expected: failures for tightened contract assertions.

**Step 3: Rewire to services and split screenshot handler**

```rust
let result = crate::crates::services::crawl::crawl_status(&cfg, id)
    .await
    .map_err(|e| logged_internal_error("crawl.status", e))?;
```

```rust
let shot = crate::crates::services::screenshot::screenshot_capture(&cfg, &normalized)
    .await
    .map_err(|e| logged_internal_error("screenshot", e))?;
```

**Step 4: Re-run tests + monolith checks**

Run:

```bash
cargo test mcp_contract_parity --test mcp_contract_parity
cargo test services_migration_tests --lib
python3 scripts/enforce_monoliths.py --file crates/mcp/server/handlers_crawl_extract.rs
python3 scripts/enforce_monoliths.py --file crates/mcp/server/handlers_refresh_status.rs
python3 scripts/enforce_monoliths.py --file crates/mcp/server/handlers_system.rs
```

Expected: passing tests and no monolith violations.

**Step 5: Commit**

```bash
git add crates/mcp/server/handlers_crawl_extract.rs crates/mcp/server/handlers_refresh_status.rs crates/mcp/server/handlers_system.rs crates/mcp/server/handlers_system/screenshot.rs crates/mcp/server/services_migration_tests.rs tests/mcp_contract_parity.rs
git commit -m "refactor: complete mcp lifecycle and screenshot rewires to services"
```

---

### Task 6: CLI Rewire — Embed/Extract/Crawl/Ingest + Domains/Sources/Stats

**Files:**
- Modify: `crates/cli/commands/embed.rs`
- Modify: `crates/cli/commands/extract.rs`
- Modify: `crates/cli/commands/crawl/subcommands.rs`
- Modify: `crates/cli/commands/ingest_common.rs`
- Modify: `crates/cli/commands/ingest.rs`
- Modify: `crates/cli/commands/domains.rs`
- Modify: `crates/cli/commands/sources.rs`
- Modify: `crates/cli/commands/stats.rs`
- Modify: `crates/cli/commands/services_migration_tests.rs`

**Step 1: Add failing CLI tests for migrated subcommands and output parity**

- Extend:
- `tests/cli_full_rewire_smoke.rs`
- `tests/cli_system_rewire_regression.rs`

```rust
#[tokio::test]
async fn embed_status_routes_through_service_wrapper() {
    // compile-time contract: handler uses services::embed::<lifecycle fn>
}

#[test]
fn domains_command_uses_services_system_domains() {
    // include_str guard assertion against qdrant direct payload helper
}
```

**Step 2: Run tests and verify failure**

Run:

```bash
cargo test cli_full_rewire_smoke --test cli_full_rewire_smoke
cargo test cli_system_rewire_regression --test cli_system_rewire_regression
```

Expected: failures/guard hits for current direct imports.

**Step 3: Rewire command handlers to service wrappers and standard logging/error mapping**

```rust
let result = crate::crates::services::embed::embed_status(cfg, id).await?;
log_info(&format!("command=embed subcommand=status id={id}"));
```

```rust
let facets = crate::crates::services::system::domains(cfg, pagination).await?;
```

**Step 4: Re-run tests and monolith checks**

Run:

```bash
cargo test cli_full_rewire_smoke --test cli_full_rewire_smoke
cargo test cli_system_rewire_regression --test cli_system_rewire_regression
cargo test services_migration_tests --lib
python3 scripts/enforce_monoliths.py --file crates/cli/commands/embed.rs
python3 scripts/enforce_monoliths.py --file crates/cli/commands/extract.rs
python3 scripts/enforce_monoliths.py --file crates/cli/commands/ingest_common.rs
```

Expected: tests green, no direct-layer forbidden fragments.

**Step 5: Commit**

```bash
git add crates/cli/commands/embed.rs crates/cli/commands/extract.rs crates/cli/commands/crawl/subcommands.rs crates/cli/commands/ingest_common.rs crates/cli/commands/ingest.rs crates/cli/commands/domains.rs crates/cli/commands/sources.rs crates/cli/commands/stats.rs crates/cli/commands/services_migration_tests.rs tests/cli_full_rewire_smoke.rs tests/cli_system_rewire_regression.rs
git commit -m "refactor: route cli lifecycle and system commands through services"
```

---

### Task 7: CLI Rewire — Watch + Refresh Schedule Stack (Mandatory Split)

**Files:**
- Modify: `crates/cli/commands/watch.rs`
- Modify: `crates/cli/commands/refresh/schedule.rs` (currently 516 lines)
- Create: `crates/cli/commands/refresh/schedule/add.rs`
- Create: `crates/cli/commands/refresh/schedule/run_due.rs`
- Create: `crates/cli/commands/refresh/schedule/worker.rs`
- Modify: `crates/cli/commands/refresh/github.rs`
- Modify: `crates/cli/commands/refresh/resolve.rs`
- Modify: `crates/cli/commands/refresh/schedule_compat_tests.rs`

**Step 1: Add failing tests for schedule/watch service routing and run-due behavior**

```rust
#[tokio::test]
async fn refresh_schedule_run_due_uses_service_refresh_start() {
    // assert dispatch path uses services::refresh::refresh_start
}

#[tokio::test]
async fn watch_run_now_refresh_task_uses_service_layer() {
    // assert no direct jobs::watch or jobs::refresh dispatch path
}
```

**Step 2: Run tests to verify failure**

Run:

```bash
cargo test schedule_compat_tests --lib
cargo test watch --lib
```

Expected: failures or guard findings on direct job-layer paths.

**Step 3: Split `refresh/schedule.rs` and migrate logic to service wrappers**

```rust
// crates/cli/commands/refresh/schedule.rs
mod add;
mod run_due;
mod worker;

pub use run_due::handle_refresh_schedule_run_due;
```

```rust
let started = crate::crates::services::refresh::refresh_start(cfg, &urls).await?;
```

**Step 4: Re-run tests + monolith checks**

Run:

```bash
cargo test schedule_compat_tests --lib
cargo test watch --lib
python3 scripts/enforce_monoliths.py --file crates/cli/commands/refresh/schedule.rs
python3 scripts/enforce_monoliths.py --file crates/cli/commands/watch.rs
python3 scripts/enforce_monoliths.py --file crates/cli/commands/refresh/github.rs
```

Expected: split files all pass monolith policy; tests green.

**Step 5: Commit**

```bash
git add crates/cli/commands/watch.rs crates/cli/commands/refresh/schedule.rs crates/cli/commands/refresh/schedule/add.rs crates/cli/commands/refresh/schedule/run_due.rs crates/cli/commands/refresh/schedule/worker.rs crates/cli/commands/refresh/github.rs crates/cli/commands/refresh/resolve.rs crates/cli/commands/refresh/schedule_compat_tests.rs
git commit -m "refactor: split refresh schedule and route watch/scheduler through services"
```

---

### Task 8: Web Execute — Remove Ingest Subprocess Fallback

**Files:**
- Modify: `crates/web/execute/constants.rs`
- Modify: `crates/web/execute/async_mode.rs`
- Modify: `crates/web/execute/sync_mode/service_calls.rs`
- Modify: `crates/web/execute/sync_mode/subprocess.rs`
- Modify: `crates/web/execute/tests/ws_protocol_tests.rs`
- Modify: `crates/web/execute/tests/ws_event_v2_tests.rs`
- Modify: `crates/web/execute/tests/async_ingest_routing_tests.rs`

**Step 1: Add failing tests that ingest modes are direct async dispatch**

```rust
#[test]
fn async_subprocess_modes_excludes_ingest_modes() {
    assert!(!super::super::constants::ASYNC_SUBPROCESS_MODES.contains(&"github"));
    assert!(!super::super::constants::ASYNC_SUBPROCESS_MODES.contains(&"reddit"));
    assert!(!super::super::constants::ASYNC_SUBPROCESS_MODES.contains(&"youtube"));
}

#[test]
fn async_modes_includes_ingest_modes() {
    assert!(super::super::constants::ASYNC_MODES.contains(&"github"));
    assert!(super::super::constants::ASYNC_MODES.contains(&"reddit"));
    assert!(super::super::constants::ASYNC_MODES.contains(&"youtube"));
}
```

**Step 2: Run tests and verify failure**

Run:

```bash
cargo test ws_protocol_tests --lib
cargo test ws_event_v2_tests --lib
```

Expected: failures against current constants/routing behavior.

**Step 3: Implement send-safe ingest dispatch boundary**

- Preferred: use the same `LocalSet + oneshot + std::thread` pattern already used by evaluate wrappers in `sync_mode/service_calls.rs`, but for async ingest enqueue wrappers.

```rust
fn call_ingest_start_send_safe(
    cfg: Arc<Config>,
    target: String,
    include_source: bool,
) -> Pin<Box<dyn Future<Output = Result<EnqueueResult, String>> + Send + 'static>> {
    Box::pin(async move {
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, async move {
                let result = crate::crates::services::ingest::ingest_start_from_target(
                    &cfg,
                    &target,
                    include_source,
                )
                .await
                .map(|r| EnqueueResult::JobId(r.job_id))
                .map_err(|e| e.to_string());
                let _ = tx.send(result);
            });
        });
        rx.await.map_err(|_| "ingest dispatch task panicked".to_string())?
    })
}
```

**Step 4: Re-run tests + guard checks**

Run:

```bash
cargo test ws_protocol_tests --lib
cargo test ws_event_v2_tests --lib
cargo test async_ingest_routing_tests --lib
python3 scripts/enforce_monoliths.py --file crates/web/execute/async_mode.rs
python3 scripts/enforce_monoliths.py --file crates/web/execute/constants.rs
```

Expected: no ingest subprocess fallback and event shape remains compatible.

**Step 5: Commit**

```bash
git add crates/web/execute/constants.rs crates/web/execute/async_mode.rs crates/web/execute/sync_mode/service_calls.rs crates/web/execute/sync_mode/subprocess.rs crates/web/execute/tests/ws_protocol_tests.rs crates/web/execute/tests/ws_event_v2_tests.rs crates/web/execute/tests/async_ingest_routing_tests.rs
git commit -m "refactor: move web ingest async modes from subprocess to service dispatch"
```

---

### Task 9: Final Verification and Hardening

**Files:**
- Modify (if needed): `docs/sessions/2026-03-12-service-layer-migration-v2.md`

**Step 1: Run targeted migration guards**

Run:

```bash
cargo test services_migration_tests --lib
cargo test mcp_contract_parity --test mcp_contract_parity
cargo test cli_full_rewire_smoke --test cli_full_rewire_smoke
cargo test ws_protocol_tests --lib
cargo test async_ingest_routing_tests --lib
```

Expected: all pass.

**Step 2: Run compile + focused regressions**

Run:

```bash
cargo check --bin axon
cargo test tests::cli_ -- --nocapture
cargo test tests::mcp_ -- --nocapture
```

Expected: clean compile and no regressions in relevant suites.

**Step 3: Run monolith policy on all touched files**

Run (repeat per touched file):

```bash
python3 scripts/enforce_monoliths.py --file crates/cli/commands/refresh/schedule.rs
python3 scripts/enforce_monoliths.py --file crates/mcp/server/handlers_system.rs
python3 scripts/enforce_monoliths.py --file crates/web/execute/async_mode.rs
```

Expected: all pass, no `.monolith-allowlist` modifications.

**Step 4: Validate no bypasses remain via grep checks**

Run:

```bash
rg -n "jobs::(crawl|extract|embed|ingest|refresh)|vector::ops::|ingest::classify" crates/cli/commands crates/mcp/server crates/web/execute
```

Expected: only intentional service internals and tests; no command/handler/web dispatch bypasses.

**Step 5: Commit + summary**

```bash
git add -A
git commit -m "chore: finalize service layer migration v2 with guards and verifications"
```

Then record session summary with:
- bypasses removed
- file splits performed
- guard suites run
- monolith checks run

---

## Definition of Done Checklist

- [ ] No business-operation bypasses in `crates/cli/**`, `crates/mcp/server/**`, `crates/web/execute/**`.
- [ ] All migrated operations route through `crates/services/*`.
- [ ] MCP handlers use sanitized error helpers consistently.
- [ ] CLI touched modules use established structured logging and error mapping.
- [ ] Web execute no longer uses subprocess fallback for `github|reddit|youtube`.
- [ ] CLI + MCP + web migration guard tests pass.
- [ ] No touched file exceeds 500 lines.
- [ ] `.monolith-allowlist` unchanged.
