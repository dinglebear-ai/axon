# Old Crate Removal Final Issue Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish Phase 12 by removing old crates/modules only after replacement paths pass, running final cutover verification, updating issue #298, and filing deferrable hardening follow-ups.

**Architecture:** Treat crate removal as the last cleanup gate, not an implementation shortcut. Prove canonical replacements, generated removal checks, reset/preflight, Tier 5 cutover cases, and issue checklist state before marking #298 complete.

**Tech Stack:** Rust workspace, Cargo metadata, `cargo xtask check`, GitHub CLI, issue #298 checklist, generated schemas, Tier 5 fake/local cutover tests.

## Removal Readiness Audit (2026-07-06/07, wave 2 investigation)

Investigation-only pass across all five candidate crates, done by reading the
actual `crates/*/Cargo.toml` dependency graph, `grep`-verifying every
`axon_<crate>::` source reference (excluding test files), tracing the live
clap `CliCommand` enum in `crates/axon-core/src/config/cli.rs`, and reading
this repo's own `xtask/src/checks/crate_contracts_spec*.rs` +
`xtask/src/checks/layering.rs` (both of which independently encode the same
contract this plan describes). Findings below **supersede** the blanket "all
five must be removed together" framing at the top of this doc — one of the
five is genuinely orphaned today; the other four are still load-bearing.

### Corrected per-crate verdicts

| Crate | Verdict | Why |
|---|---|---|
| `axon-code-index` | **Safe to delete now** | Zero dependents. No other crate's `Cargo.toml` lists it as a path dependency, no `.rs` file outside its own crate imports `axon_code_index::`, and the `code-search`/`code-search-watch` CLI commands it existed to serve are already removed from the live `CliCommand` enum (confirmed absent from `crates/axon-core/src/config/cli.rs`; also present in `xtask`'s own `removed_commands()`/`schema_registry.rs` list). This crate is dead weight left over from the Phase 6 code-search-generation cutover (commit `966496e90`), which moved local-code-index responsibilities into `axon-ledger` + `axon-parse` + `axon-jobs` + `axon-vectors` and never deleted the old crate shell. |
| `axon-vector` | **Blocked — still required** | Depended on (path dep in `[dependencies]`, not just dev-deps) by `axon-cli`, `axon-jobs`, `axon-ingest`, `axon-mcp`, `axon-web`, `axon-services`. Real, non-test source usage (42 files) inside `axon-services` for `embed`, `scrape` (vertical dispatch fallback), `refresh`, `reset`, `migrate`, `system::{dedupe,domains,purge,sources,stats}`, and — critically — `query/ask_retrieval.rs`. That file's own doc comment says the cutover to `axon-retrieval` only replaced the SEARCH+CONTEXT half of `ask`; **LLM synthesis is "deliberately left on the legacy path"** via `axon_vector::ops::commands::ask::ask_result_from_context`, and `ask --explain` (used by `train`) stays on the full legacy `build_ask_context` reranker. `axon-cli`'s `serve`/`mcp` startup path also calls `axon_vector::cache::enforce_core_dump_disabled_for_ask_cache` directly (a real security hardening call, not dead code). |
| `axon-crawl` | **Blocked — still required** | Depended on by `axon-cli`, `axon-jobs`, `axon-extract`, `axon-services`. This is the actual HTTP/Chrome crawl engine. The new `axon source` dispatch path's `dispatch_web` (in `crates/axon-services/src/source/dispatch.rs`) does NOT reimplement fetching — it calls `crawl_for_source` in `crates/axon-services/src/crawl_sync.rs`, which directly calls `axon_crawl::engine::*` and `axon_crawl::chrome_bootstrap::*`. `axon-adapters/src/web.rs` (the new web "adapter") is metadata/scope/manifest-only, matching the family pattern of every other adapter — it has no HTTP client dependency at all. `dependency-order-map.md`'s own "Do Not Start X Until Y" list still says "Do not port web crawl until URL normalization, authority mapping, and map/source scope behavior are implemented" — this has not fully landed as a crawl-engine replacement, only as a routing/scoping layer in front of the same engine. `axon-cli`'s `screenshot` command also calls `axon_crawl::screenshot::url_to_screenshot_filename` directly. |
| `axon-ingest` | **Blocked — still required, but narrower than it looks** | Depended on by `axon-cli`, `axon-jobs`, `axon-extract`, `axon-services`, `axon-web`. Good news: the new `axon source` dispatch path (`dispatch_git`/`dispatch_feed`/`dispatch_reddit`/`dispatch_youtube`/`dispatch_registry`/`dispatch_session` in `source/dispatch.rs`) does **not** touch `axon_ingest::` at all — it uses brand-new `axon-services`-owned acquisition modules (`git_acquire.rs`, `feed_acquire.rs`, `reddit_acquire.rs`, `youtube_acquire.rs`, `registry_acquire.rs`) plus `index_*_source_with_job` bridges. So the *acquisition* half of the old ingest pipeline is already superseded for those source kinds. What's still live: (1) `axon_services::ingest.rs` re-exports `axon_ingest::orchestrate::*` and is called by `axon-cli`'s `sessions` command via `commands/ingest_common.rs` (job status/cancel/list/cleanup/clear/worker/recover subcommands + the sync ingest path used by `sessions`); (2) `axon_ingest::sessions::watch::{SessionWatchEventSink, SessionWatchProcessEvent}` types are used directly by `axon-cli/src/commands/sessions.rs`; (3) `axon_ingest::github::github_repo_exists` is used inside `ingest.rs`. The legacy top-level `Ingest` clap subcommand itself is confirmed gone from `CliCommand`, but its job-runner/watch/session machinery is still reachable through the still-live `Sessions` command. |
| `axon-extract` | **Blocked — still required, and the axon-jobs dependency is real, current, and *intentionally* undocumented as permanent (see below)** | Depended on by `axon-cli`, `axon-mcp`, `axon-jobs`, `axon-services`. `axon_services::extract::extract_sync` is a direct re-export of `axon_extract::sync::extract_sync`, and it backs the still-live `Extract` clap command (present in `CliCommand`, mutating/async job). `axon_services::scrape.rs` re-exports `axon_extract::scrape::*` and is the vertical-extractor auto-routing (`dispatch_by_url()`) used by `scrape`/`brand`/`summarize`/`diff`/`screenshot`/the web source dispatch path whenever `cfg.enable_verticals = true` (default on). |

### `axon-jobs` → `axon-extract` dependency — resolved

The task's open question was whether `crates/axon-jobs/src/workers/runners/extract.rs`'s
`run_extract_job()` calling `axon_extract::sync::extract_sync()` directly makes
`axon-extract` a **permanent** required dependency of `axon-jobs` (meaning the
plan needs to be revised to say this crate survives Phase 12), or whether the
plan intends that logic to relocate before `axon-extract` can go away.

**Answer, straight from this repo's own layering-contract source:**
`xtask/src/checks/crate_contracts_spec_cont.rs`'s `axon-jobs` contract entry
says, verbatim, in a code comment:

> README: axon-jobs must not depend on axon-services (contradiction-review.md:
> "axon-jobs may not depend on axon-services" — the composition layer injects
> worker functions instead). **Legacy crates are intentionally NOT forbidden
> here: axon-jobs' worker runners currently wrap
> axon-crawl/axon-vector/axon-ingest/axon-extract behavior pre-cutover.**

This is decisive: the codebase's own machine-checked contract already
anticipates and names this exact dependency, and explicitly frames it as
**temporary pre-cutover debt**, not a sanctioned permanent architecture. Every
*other* new-generation crate's `forbidden_axon_deps` list (in the same file)
explicitly forbids all five legacy crate names (`axon-vector`, `axon-crawl`,
`axon-ingest`, `axon-extract`, `axon-code-index`) — `axon-jobs` is the single,
deliberate exception, and the comment names it as such. Separately,
`foundation/crate-structure.md`'s target-dependency table lists `axon-jobs`'s
allowed axon dependencies as "provider boundary crates, domain service
traits" only — `axon-extract` is not among them in the target state.

**Conclusion:** the plan should **not** be revised to say `axon-extract`
survives Phase 12 as a permanent `axon-jobs` dependency. The correct fix is
what `foundation/crate-structure.md` already prescribes: `axon-extract`'s
surviving pieces ("Structured LLM extraction remains a top-level action, but
vertical scraping/adapters move under source routing") land in `axon-llm` +
`axon-parse` + a top-level `axon-services` extraction entry point, and
`axon-jobs`' `run_extract_job` should call that new entry point instead of
`axon_extract::sync::extract_sync` directly. Note `foundation/crate-structure.md`
already hedges `axon-extract`'s disposition as "remove **or shrink**" (softer
than the unconditional "remove after ..." wording used for the other four
crates), which matches this: it may not fully disappear, but it must stop
being a distinct workspace member that `axon-jobs` depends on directly. Until
that relocation happens, Task 3 ("Delete Old Crates From Workspace") cannot
proceed for `axon-extract` — deleting it today would break `axon-jobs`'
extract job runner and the still-live `Extract` CLI command.

### Corrected Global Constraint

Line 18 below ("Remove `axon-vector`, `axon-code-index`, `axon-crawl`,
`axon-ingest`, and `axon-extract` only after replacement paths and tests
pass") is accurate in spirit but should not be read as "these five are
equally close to done." As of this audit: **only `axon-code-index` currently
has zero remaining dependents and can be deleted immediately** (Task 3 for
that one crate only, independent of the other four). The other four each have
real, current, non-test call sites and require the acquisition/dispatch/
synthesis migrations described above before Task 3 applies to them.

## Engineering Review Corrections

The Lavra engineering review found that old-crate deletion must be blocked by replacement-path evidence, not by import absence alone. This revision requires Tier 5 replacement/cutover cases before deletion, dispatch and call-path tests for removed operations, smaller implementation PRs, and explicit issue-scope honesty: full all-source fixture completeness cannot be deferred if issue #298 is marked complete.

## Global Constraints

- Source of truth: live issue #298 Phase 12 and every contract under `docs/pipeline-unification`, especially `delivery/cutover-contract.md`, `delivery/testing-contract.md`, `delivery/documentation-contract.md`, `delivery/docs-generator-contract.md`, `delivery/surface-removal-contract.md`, `delivery/dependency-order-map.md`, `foundation/crate-structure.md`, `foundation/repo-structure.md`, `runtime/pruning-contract.md`, `schemas/schema-generator-contract.md`, `surfaces/*-contract.md`, and `crates/README.md`.
- Remove `axon-vector`, `axon-code-index`, `axon-crawl`, `axon-ingest`, and `axon-extract` only after replacement paths and tests pass.
- Prove no compatibility facades, no transport imports of domain internals, no old code paths reachable from canonical surfaces, and root `axon` is bootstrap only.
- Generated docs, schemas, clients, and presentation token outputs must be current in check mode before final issue sync.
- Tier 0-5 test model must exist; default CI must run tiers 0-3; live smoke must be opt-in/skippable; Tier 5 cutover cases must pass before completion.
- Transport parity matrix must cover CLI/MCP/REST for source, watch, map, extract, query, retrieve, ask, memory, prune, reset, jobs, and events.
- Adapter fixture families must exist for web, local, git, registries, feeds, social, video, sessions, CLI tools, and MCP tools. Full all-source fixture completeness cannot be deferred if #298 is marked complete.
- Task-level verification is narrower than the final gate; full workspace tests and live smoke belong at final cutover.
- Before marking #298 complete, run Tier 5 cutover cases from `delivery/testing-contract.md`.
- Run Tier 5 replacement-path/cutover cases before the final old-crate deletion commit, or at minimum before merging that deletion. Crate deletion depends on replacement path proof.
- Import absence is not enough. Add dispatch/call-path tests proving removed CLI/MCP/REST operations cannot reach old modules or compatibility facades.
- Do not combine crate deletion, Tier 5 harness creation, issue audit, and follow-up issue filing in one implementation PR unless explicitly requested.
- Full all-source fixture completeness cannot be deferred if #298 is being marked complete. If it remains incomplete, #298 must remain open or the issue scope must be explicitly narrowed.
- Filing follow-up GitHub issues should not block code cutover unless Jacob explicitly asks for issue hygiene in the same PR.
- Selected live smoke tests for local, web, git, ask/query, and reset must run before release readiness unless explicitly skipped with an issue-linked reason and live-smoke opt-in evidence.

---

## File Structure

- Modify: `Cargo.toml`
- Modify: crate dependency manifests that still import old crates.
- Delete only after proof: `crates/axon-vector`, `crates/axon-code-index`, `crates/axon-crawl`, `crates/axon-ingest`, `crates/axon-extract`
- Modify: `src/main.rs`, `src/lib.rs` only if bootstrap-only proof exposes old behavior.
- Modify: `xtask/src/check_repo_structure.rs`
- Modify: generated artifacts under `docs/reference/**`
- Update: GitHub issue `#298`
- Create follow-up GitHub issues for deferrable hardening.

---

### Task 1: Replacement Path Gate Before Old Crate Removal

**Files:**
- Test: `xtask/src/check_repo_structure_tests.rs`
- Modify: `xtask/src/check_repo_structure.rs`

**Interfaces:**
- Produces: old-crate removal readiness report.

- [ ] **Step 1: Add failing readiness test**

```rust
#[test]
fn old_crate_removal_is_blocked_until_replacements_pass() {
    let report = old_crate_removal_readiness(workspace_with_old_imports()).unwrap();
    assert!(!report.ready);
    assert!(report.blockers.iter().any(|b| b.contains("transport imports old domain crate")));
}
```

- [ ] **Step 2: Run readiness test**

Run: `cargo test -p xtask old_crate_removal --no-fail-fast`

Expected: blockers are reported for any remaining old crate usage.

- [ ] **Step 3: Implement readiness checks**

Check:

```text
canonical source paths pass targeted tests
generated removal checks pass
reset/preflight blockers pass
docs/schema/presentation generators pass in --check mode
Tier 0-5 test model exists and Tier 5 cutover cases pass
transport parity matrix passes for CLI/MCP/REST
adapter fixture family inventory is complete
no public surface imports old crate handlers
no compatibility facade crates remain
no transport imports domain internals
root axon contains bootstrap only
Cargo metadata has no old crate package after deletion
```

- [ ] **Step 4: Run readiness checks**

Run: `cargo test -p xtask old_crate_removal --no-fail-fast`

Expected: readiness fails until all blockers are resolved.

---

### Task 2: Remove Old Crate Imports And Facades

**Files:**
- Modify: all `Cargo.toml` files with old crate dependencies.
- Modify: service/CLI/MCP/web modules importing old crates.
- Test: `xtask/src/check_repo_structure_tests.rs`

**Interfaces:**
- Produces: workspace with no reachable old crate code paths.

- [ ] **Step 1: Identify old imports**

Run:

```bash
cargo metadata --no-deps --format-version 1 > target/old-crate-metadata.json
```

Expected: metadata file lists current workspace crates before removal.

- [ ] **Step 2: Add import absence test**

```rust
#[test]
fn transports_do_not_import_old_domain_crates() {
    let report = scan_workspace_imports().unwrap();
    for old in ["axon_code_index", "axon_crawl", "axon_ingest", "axon_extract", "axon_vector"] {
        assert!(!report.transport_imports.iter().any(|import| import.crate_name == old));
    }
}
```

- [ ] **Step 3: Remove old imports**

Replace old imports with target crates from the pipeline-unification contracts: source adapters, ledger, vectors, retrieval, graph, jobs, memory, prune, and services. Delete compatibility facade modules instead of forwarding to canonical paths.

- [ ] **Step 4: Run import checks**

Run: `cargo test -p xtask transports_do_not_import_old_domain_crates --no-fail-fast`

Expected: no transport import reaches old domain internals.

---

### Task 3: Delete Old Crates From Workspace

> See "Removal Readiness Audit" above for current per-crate status. As of the
> 2026-07-06/07 audit only `crates/axon-code-index` is ready for this task
> today (zero dependents); the other four are blocked on the migrations
> described there. Do not batch `axon-code-index`'s removal with the other
> four — it has no shared replacement-path prerequisite with them.

**Files:**
- Modify: `Cargo.toml`
- Delete: `crates/axon-vector` (blocked — see audit)
- Delete: `crates/axon-code-index` (ready now — zero dependents)
- Delete: `crates/axon-crawl` (blocked — see audit)
- Delete: `crates/axon-ingest` (blocked — see audit)
- Delete: `crates/axon-extract` (blocked — see audit; also required by `axon-jobs` pre-cutover, see dependency resolution above)
- Modify generated references through schema/docs generator.

**Interfaces:**
- Produces: workspace without old crates.

- [ ] **Step 1: Run pre-delete targeted checks**

Run:

```bash
cargo test -p axon-services source --no-fail-fast
cargo test -p axon-vectors generation prune --no-fail-fast
cargo xtask schemas generate --check
cargo xtask docs generate --check
cargo xtask presentation generate --check
cargo test --workspace tier5 --no-fail-fast
```

Expected: replacement source/vector/schema paths pass before deletion.

- [ ] **Step 2: Remove crates from workspace**

Delete old crate directories and remove package entries/dependencies from workspace manifests. Do not keep empty crates or compatibility packages with old names.

- [ ] **Step 3: Run metadata absence check**

Run:

```bash
cargo metadata --no-deps --format-version 1 > target/post-removal-metadata.json
```

Expected: metadata contains none of `axon-vector`, `axon-code-index`, `axon-crawl`, `axon-ingest`, or `axon-extract`.

- [ ] **Step 4: Run repo-structure check**

Run: `cargo xtask check-repo-structure`

Expected: removed-crate absence passes.

---

### Task 4: Tier 0-5 Test Model, Transport Parity, And Fixture Inventory

**Files:**
- Modify/create: test tier metadata and CI workflow/config files.
- Modify/create: transport parity matrix tests.
- Modify/create: adapter fixture inventory tests.

**Interfaces:**
- Produces: executable release-readiness evidence for issue #298 Phase 12.

- [ ] **Step 1: Add test tier registry**

Define Tier 0 static, Tier 1 unit, Tier 2 boundary fake, Tier 3 integration local, Tier 4 live smoke, and Tier 5 cutover commands in code/config/docs. Ensure default CI runs tiers 0-3 and live smoke is opt-in/skippable.

- [ ] **Step 2: Add transport parity matrix**

Cover CLI/MCP/REST parity for source create/refresh, map, watch create/exec, extract, query, retrieve, ask, memory remember/search/context, prune plan/exec, reset plan/exec, jobs get/events/cancel/retry, and shared success/error envelopes.

- [ ] **Step 3: Add adapter fixture family inventory**

Assert fixture families exist for web, local, git/github, registries, feeds, social, video, sessions, CLI tools, and MCP tools with input, resolved source, capability, manifest, fetched item, `SourceDocument`, metadata, graph candidates where supported, degraded modes, and watch/refresh behavior.

- [ ] **Step 4: Run release-readiness tests**

Run:

```bash
cargo test --workspace tier0 tier1 tier2 tier3 --no-fail-fast
cargo test --workspace transport_parity adapter_fixture_inventory --no-fail-fast
cargo test --workspace tier5 --no-fail-fast
```

Expected: default tiers, transport parity, fixture inventory, and Tier 5 cutover cases pass.

---

### Task 5: Generated Docs, Schemas, Clients, And Presentation Final Gate

**Files:**
- Generated: `docs/reference/**`
- Generated: web/Palette/Android/Chrome clients
- Generated: presentation token outputs
- Test: xtask docs/schema/presentation tests.

**Interfaces:**
- Produces: final generated-surface freshness evidence.

- [ ] **Step 1: Run schema/docs/presentation generation checks**

Run:

```bash
cargo xtask schemas generate --check
cargo xtask docs generate --check
cargo xtask presentation generate --check
cargo xtask check-doc-links
cargo xtask check-doc-contracts
```

Expected: generated markdown headers, source input manifests, validated examples, schema fixtures, token snapshots, and stale-doc CI failure behavior exist and pass.

- [ ] **Step 2: Verify removed surfaces cannot dispatch**

Run the CLI/MCP/REST removed-surface tests plus generated-client removed-operation tests.

Expected: removed CLI/MCP/REST/config/DTO/client surfaces are absent and cannot dispatch or validate.

---

### Task 6: Phase 6 Through Phase 12 Issue Checklist Audit

**Files:**
- No code files.
- External: GitHub issue `#298`.

**Interfaces:**
- Produces: exact checked/unchecked status update on issue #298.

- [ ] **Step 1: Fetch issue body and comments**

Run:

```bash
gh issue view 298 --json number,title,body,comments,state,url > target/issue-298-final-audit.json
```

Expected: local JSON contains the live checklist and comments.

- [ ] **Step 2: Audit checklist items**

For every Phase 6, 7, 8, 9, 10, 11, and 12 unchecked item, plus crate relocation, dependency/order, and planned PR breakdown items, record:

```text
issue checklist text
implemented evidence path/test
status: checked or remains unchecked
reason if unchecked
follow-up issue if deferrable
```

- [ ] **Step 3: Verify completion rules before editing**

Do not mark #298 complete unless generated docs/schemas, fake-boundary tests, transport parity matrix, selected live smoke tests, Tier 5 cutover cases, mandatory reviews, all required adapter fixture families, and no-known-contract-gaps signoff all have current evidence. If any required item remains incomplete, keep #298 open and list blockers.

- [ ] **Step 4: Update issue body**

Use `gh issue edit 298 --body-file target/issue-298-updated-body.md` after updating checklist boxes. Only check items backed by code/docs/tests in the current branch.

- [ ] **Step 5: Post audit summary comment**

Run:

```bash
gh issue comment 298 --body-file target/issue-298-final-audit-comment.md
```

Expected: issue comment lists exact checked/unchecked changes and final blockers if any.

---

### Task 7: File Deferrable Hardening Follow-Ups

**Files:**
- External: GitHub issues.

**Interfaces:**
- Produces: follow-up issues for non-blocking hardening.

- [ ] **Step 1: Create follow-up issue drafts**

Draft issues only for genuinely non-blocking hardening. Do not defer any item required by issue #298 Phase 12 completion. Candidate non-blocking issues:

```text
additional live-smoke breadth beyond selected local/web/git/ask/query/reset
extra presentation polish after required token parity and accessibility checks pass
additional Android/Chrome UX depth after generated client parity passes
full logs/traces status polish beyond required observability/status parity
memory graph/vector enhancements not required for current memory contract
```

- [ ] **Step 2: File follow-ups**

Run one `gh issue create` command per follow-up, each with title, body, labels, and link back to #298.

Expected: every deferrable hardening item has a durable issue URL.

- [ ] **Step 3: Comment follow-up list on #298**

Append the follow-up issue URLs to the #298 final audit comment or a separate comment if the final audit is already posted.

---

### Task 8: Tier 5 Cutover Cases

**Files:**
- Test: `crates/axon-services/src/tier5_cutover_tests.rs`
- Test: `crates/axon-cli/src/tier5_cutover_tests.rs`
- Test: `crates/axon-web/src/tier5_cutover_tests.rs`
- Test: `crates/axon-mcp/src/tier5_cutover_tests.rs`

**Interfaces:**
- Consumes: completed Phases 6-11.
- Produces: final cutover case coverage from `delivery/testing-contract.md`.

- [ ] **Step 1: Add Tier 5 test harness**

```rust
#[tokio::test]
async fn tier5_incompatible_store_block_and_reset_flow() {
    let harness = tier5_harness_with_incompatible_store().await;
    assert_eq!(harness.start_workers().await.unwrap_err().code.to_string(), "startup.incompatible_store");
    let plan = harness.reset_dry_run().await.unwrap();
    assert!(plan.estimates.sqlite_rows > 0);
    let receipt = harness.reset_yes(plan.reset_plan_id).await.unwrap();
    assert!(receipt.created.qdrant_collections.contains(&"axon".to_string()));
}
```

- [ ] **Step 2: Add remaining Tier 5 cases**

Cover removed config validation, removed CLI/MCP/REST/generated-client absence, old job/code-index/Qdrant payload absence, fresh SQLite schema, fresh Qdrant collection/index shape, canonical local source reindex, canonical web source reindex, ask/query retrieval from target payloads, provider backpressure during fresh reindex, auth/token cache invalidation or re-auth guidance, and interrupted partial generation not searchable after restart.

- [ ] **Step 3: Run Tier 5 tests**

Run: `cargo test --workspace tier5 --no-fail-fast`

Expected: all Tier 5 fake/local cutover tests pass.

---

### Task 9: Selected Live Smoke And Mandatory Reviews

- [ ] **Step 1: Run selected live smoke tests**

Run the opt-in live smoke suite for local, web, git, ask/query, and reset with secrets redacted in output.

Expected: live smoke is opt-in/skippable and records service URLs/model/collection/job ids/degradation status without secrets.

- [ ] **Step 2: Run mandatory PR reviews**

Run the required engineering/security/product or project-defined review process and record findings/resolutions in the final audit.

Expected: mandatory review findings are resolved or tracked as blockers before issue completion.

---

### Task 10: Final Verification Gate

- [ ] **Step 1: Run final format check**

Run: `cargo fmt --all -- --check`

Expected: no formatting changes needed.

- [ ] **Step 2: Run repository check**

Run: `cargo xtask check`

Expected: schemas, generated artifacts, repo structure, removal checks, and contract checks pass.

- [ ] **Step 3: Run full workspace tests**

Run: `cargo test --workspace --no-fail-fast`

Expected: all workspace tests pass.

- [ ] **Step 4: Run docs/schema/presentation final checks**

Run:

```bash
cargo xtask schemas generate --check
cargo xtask docs generate --check
cargo xtask presentation generate --check
cargo xtask check-doc-links
cargo xtask check-doc-contracts
```

Expected: docs match generated artifacts, examples validate, links/contracts pass, and no stale generated output remains.

- [ ] **Step 5: Verify old crate absence**

Run:

```bash
cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name' | sort > target/workspace-packages.txt
```

Expected: output does not include `axon-vector`, `axon-code-index`, `axon-crawl`, `axon-ingest`, or `axon-extract`.

- [ ] **Step 6: Mark #298 complete only after evidence exists**

Update #298 only if Tasks 1-10 pass and the checklist audit has no required unchecked items or known contract gaps. If required items remain, leave #298 open and list blockers with exact files/tests.
