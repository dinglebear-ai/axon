# CLI P1 Remediation Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Address all current P1 findings for the CLI crate and related workspace config, with regression coverage and updated documentation/reporting.

**Architecture:** Execute the work in dependency-aware batches. First unify CLI job output contracts and shared job helpers so `embed` and `refresh` can move onto the same status/list/cancel/error pathways. Then fix service-boundary and security issues (`domains`, MCP bind default, GitHub refresh validation/client), and finally align build/CI policy (MSRV, Postgres test gating, monolith policy) before updating docs and review reports.

**Tech Stack:** Rust, Tokio, Serde, SQLx, reqwest, GitHub Actions, Markdown docs

---

## Chunk 1: CLI contract and handler unification

### Task 1: Make P0 report status visually obvious

**Files:**
- Modify: `crates/cli/.full-review/05-final-report.md`

- [ ] **Step 1: Add checkbox markers to the P0 findings table**
- [ ] **Step 2: Add checked markers to the completed P0 action items**
- [ ] **Step 3: Read the updated section and verify the checkboxes are clearly visible**
- [ ] **Step 4: Commit**

### Task 2: Collapse duplicate job output structs in `job_contracts.rs`

**Files:**
- Modify: `crates/cli/commands/job_contracts.rs`
- Modify: `crates/cli/commands/common.rs`
- Test: `crates/cli/commands/job_contracts.rs`

- [ ] **Step 1: Write failing tests covering shared serialization behavior for status and summary payloads**
- [ ] **Step 2: Run the targeted job contract tests and verify they fail for the duplicated path you are replacing**
- [ ] **Step 3: Introduce one shared record type or shared constructor path for the overlapping fields**
- [ ] **Step 4: Update `common.rs` consumers to use the unified contract without changing JSON field names**
- [ ] **Step 5: Re-run targeted `job_contracts` and `common` tests**
- [ ] **Step 6: Commit**

### Task 3: Remove duplicated `JobStatus` trait boilerplate

**Files:**
- Modify: `crates/cli/commands/common.rs`
- Modify: `crates/cli/commands/job_contracts.rs` (if shared constructors/macros live here)
- Test: `crates/cli/commands/common.rs`

- [ ] **Step 1: Add a test that exercises all current `JobStatus` implementors through the shared helper APIs**
- [ ] **Step 2: Run the targeted test and verify it fails before the refactor**
- [ ] **Step 3: Replace the repeated impl blocks with a shared macro/helper that preserves behavior**
- [ ] **Step 4: Re-run targeted helper tests**
- [ ] **Step 5: Commit**

### Task 4: Move `embed.rs` onto shared job lifecycle helpers

**Files:**
- Modify: `crates/cli/commands/embed.rs`
- Modify: `crates/cli/commands/common.rs`
- Modify: `crates/cli/commands/job_contracts.rs` (if needed for embed payload parity)
- Test: `crates/cli/commands/embed.rs`

- [ ] **Step 1: Add failing tests for `embed status`, `embed errors`, `embed list`, and `embed cancel` JSON/human output parity**
- [ ] **Step 2: Run those targeted tests and verify failure against the current hand-rolled implementation**
- [ ] **Step 3: Implement `JobStatus` for `EmbedJob` and route handlers through `handle_job_*` helpers**
- [ ] **Step 4: Re-run embed command tests**
- [ ] **Step 5: Commit**

### Task 5: Move `refresh.rs` onto shared job lifecycle helpers and typed payloads

**Files:**
- Modify: `crates/cli/commands/refresh.rs`
- Modify: `crates/services/refresh.rs`
- Modify: `crates/cli/commands/common.rs`
- Modify: `crates/cli/commands/job_contracts.rs`
- Test: `crates/cli/commands/refresh.rs`

- [ ] **Step 1: Add failing tests for `refresh status`, `refresh errors`, `refresh list`, and `refresh cancel` output shapes**
- [ ] **Step 2: Run the targeted tests and verify they fail for the untyped `serde_json::Value` access pattern**
- [ ] **Step 3: Introduce/extend a typed refresh job response in services and implement `JobStatus` for refresh jobs**
- [ ] **Step 4: Route refresh lifecycle handlers through `handle_job_*` helpers while preserving user-facing output**
- [ ] **Step 5: Re-run refresh command tests**
- [ ] **Step 6: Commit**

## Chunk 2: Services boundary and security fixes

### Task 6: Move `domains` detailed mode behind services

**Files:**
- Modify: `crates/cli/commands/domains.rs`
- Modify: `crates/services/system.rs`
- Modify: `crates/services/types/` (if a new typed result is needed)
- Test: `crates/cli/commands/domains.rs`
- Test: `tests/services_system_services.rs`

- [ ] **Step 1: Add failing tests for detailed domains mode using only services-layer entry points**
- [ ] **Step 2: Run targeted CLI/services tests and verify current dependency direction is exposed**
- [ ] **Step 3: Move detailed domain aggregation into the services layer and update CLI to consume it**
- [ ] **Step 4: Re-run targeted domains and services tests**
- [ ] **Step 5: Commit**

### Task 7: Default MCP HTTP bind to loopback and document it

**Files:**
- Modify: `crates/cli/commands/mcp.rs`
- Modify: `crates/core/config/types/config.rs`
- Modify: `crates/core/config/types/config_impls.rs`
- Modify: `.env.example` or relevant config docs if bind defaults are surfaced there
- Modify: `crates/cli/CLAUDE.md`
- Test: config or CLI tests that cover default MCP host behavior

- [ ] **Step 1: Add a failing test for the default MCP bind address resolving to loopback**
- [ ] **Step 2: Run the targeted test and verify failure under the current `0.0.0.0` default**
- [ ] **Step 3: Change the default bind host to `127.0.0.1` and update affected docs**
- [ ] **Step 4: Re-run targeted config/CLI tests**
- [ ] **Step 5: Commit**

### Task 8: Validate GitHub repo input and use a safe HTTP client path

**Files:**
- Modify: `crates/cli/commands/refresh/github.rs`
- Modify: shared HTTP helper file if a repo-safe client belongs there (`crates/core/http.rs` or equivalent)
- Test: `crates/cli/commands/refresh/github.rs`

- [ ] **Step 1: Add failing tests for invalid repo strings and accepted `owner/repo` input**
- [ ] **Step 2: Add a failing test that verifies the GitHub request path uses a bounded client configuration**
- [ ] **Step 3: Run targeted tests and verify failure**
- [ ] **Step 4: Implement strict repo validation and move the request onto a shared timeout-configured client path**
- [ ] **Step 5: Re-run targeted GitHub refresh tests**
- [ ] **Step 6: Commit**

## Chunk 3: CI, test policy, and monolith fixes

### Task 9: Align MSRV declarations across workspace config

**Files:**
- Modify: `Cargo.toml`
- Modify: `rust-toolchain.toml`
- Modify: `.github/workflows/ci.yml`
- Modify: Docker/build config that pins Rust version if present
- Test: version consistency by reading files and, if cheap, running the relevant CI lint/consistency target

- [ ] **Step 1: Record the current Rust version pins in each source of truth**
- [ ] **Step 2: Choose one workspace-wide MSRV/runtime version and update all declarations to match**
- [ ] **Step 3: Run the cheapest available consistency check (for example targeted grep/readback plus a build or lint command if needed)**
- [ ] **Step 4: Commit**

### Task 10: Stop silently passing Postgres-dependent tests

**Files:**
- Modify: `crates/cli/commands/refresh.rs`
- Modify: `crates/cli/commands/watch.rs`
- Modify: other affected test helpers if needed
- Test: the relevant refresh/watch tests

- [ ] **Step 1: Add or update tests so missing `AXON_TEST_PG_URL` results in `#[ignore]` or explicit skip signaling instead of `Ok(())` success**
- [ ] **Step 2: Run targeted tests and verify they no longer register as false passes**
- [ ] **Step 3: Implement the chosen gating mechanism consistently across affected tests**
- [ ] **Step 4: Re-run targeted refresh/watch tests**
- [ ] **Step 5: Commit**

### Task 11: Resolve the `common.rs` monolith finding

**Files:**
- Modify: `crates/cli/commands/common.rs`
- Create: focused extracted helper module(s), likely under `crates/cli/commands/`
- Modify: `crates/cli/commands.rs` if new module wiring is required
- Modify: `.monolith-allowlist` only if extraction is clearly worse than allowlisting
- Test: `crates/cli/commands/common.rs` and any extracted helper tests

- [ ] **Step 1: Identify the cleanest extraction boundary, preferring job helper rendering code over URL parsing logic**
- [ ] **Step 2: Add or preserve focused tests around the extracted logic**
- [ ] **Step 3: Move the helper code into the new module with no behavior change**
- [ ] **Step 4: Re-run targeted command helper tests**
- [ ] **Step 5: Verify the file now satisfies monolith policy or add the minimal justified allowlist entry**
- [ ] **Step 6: Commit**

## Chunk 4: Coverage, docs, and report closure

### Task 12: Add command-handler regression coverage for high-risk CLI paths

**Files:**
- Modify: `crates/cli/commands/embed.rs`
- Modify: `crates/cli/commands/refresh.rs`
- Modify: `crates/cli/commands/domains.rs`
- Modify: `crates/cli/commands/mcp.rs`
- Modify: supporting test files/helpers as needed

- [ ] **Step 1: Add targeted tests for error messages, `--json` formatting, and edge cases in the handlers touched by the P1 work**
- [ ] **Step 2: Run the targeted command tests and make sure they fail before implementation where appropriate**
- [ ] **Step 3: Complete minimal implementation adjustments needed to satisfy the tests**
- [ ] **Step 4: Re-run the targeted test set**
- [ ] **Step 5: Commit**

### Task 13: Update docs and review report for P1 completion

**Files:**
- Modify: `crates/cli/CLAUDE.md`
- Modify: `crates/cli/.full-review/05-final-report.md`
- Modify: any other user-facing docs changed by bind-default or test-policy behavior

- [ ] **Step 1: Update docs for MCP bind default, shared lifecycle helpers, and any changed command/testing expectations**
- [ ] **Step 2: Mark completed P1 issues in the final report with visible checkbox status**
- [ ] **Step 3: Read back the updated report sections for clarity**
- [ ] **Step 4: Commit**

## Verification checkpoint

- [ ] Run the targeted command and service tests for all touched P1 areas
- [ ] Run any cheap workspace-level verification needed for config/version changes
- [ ] Confirm docs/report reflect the final implementation state
- [ ] Prepare a concise summary of completed P1 fixes and any deferred items

Plan complete and saved to `docs/superpowers/plans/2026-03-15-cli-p1-remediation-plan.md`. Ready to execute?
