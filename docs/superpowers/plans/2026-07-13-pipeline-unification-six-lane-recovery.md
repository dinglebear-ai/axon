# Pipeline Unification Recovery Triage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Recover PR #418 into a truthful, reviewable pipeline-unification slice, then drive the remaining #298 work through dependency-gated follow-up slices aligned with `docs/pipeline-unification/`.

**Architecture:** Use six review/coordination lanes with strict ownership boundaries, but serialize implementation through the critical path. Parallelism is for read-only audits, fixture inventory, and disjoint preparatory patches; integration into the recovery branch happens only at gates because schema, API, payload, job, security, and surface artifacts are shared.

**Tech Stack:** Rust workspace, Cargo, xtask schema generator, SQLite stores, Qdrant vector store, TEI embedding provider, Axum REST server, rmcp MCP surface, generated docs/schemas.

## Global Constraints

- `docs/pipeline-unification/` is the source of truth.
- Clean-break cutover: no compatibility aliases, no old payload backfill, no legacy job-row migration.
- One public indexing model: `SourceRequest -> SourceResolver -> SourceRouter -> SourceAcquisition -> SourceManifestDiff -> SourceGeneration -> SourceEnrichment -> SourceDocument -> SourceParseFacts / GraphCandidate -> SourceGraph -> DocumentPreparer -> PreparedDocument -> EmbeddingBatch -> EmbeddingProvider -> VectorPointBatch -> VectorStore -> DocumentStatus -> GenerationPublisher -> CleanupDebt`.
- Adapters own acquisition and emit `SourceDocument`, never `PreparedDocument` or vector points.
- CLI, MCP, REST, jobs, watches, and app surfaces project the same transport-neutral DTOs.
- Every detached operation returns one pollable `job_id`, except reset if Gate 6 explicitly documents and tests it as the synchronous destructive exception.
- Every mutable or refreshable source is ledger-owned before it is searchable.
- Every vector payload is produced from the shared payload builder and approved metadata registry.
- Removed CLI commands, MCP actions, REST routes, DTO fields, config keys, and docs are absent from generated surfaces.
- Every operation has an explicit scope decision before side effects: `read`, `write`, `admin`, `execute`, and `local` are not interchangeable.
- Every detached operation stores and enforces an auth/config snapshot at worker execution time.
- Provider reservations/backpressure must protect embedding, vector read/write, fetch, render, LLM, graph write, and artifact write capacity.
- Reset/prune are admin/destructive operations with dry-run inventory, confirmation, receipt artifacts, configured collection selection, and artifact-root containment.
- Default CI must cover tiers 0-3 from `delivery/testing-contract.md`; live smoke is opt-in.
- Do not use `/home/jmagar/workspace/axon` root checkout for #418 execution until its unrelated dirty state is preserved. Use a clean worktree.

---

## Engineering Review Applied

This plan was reviewed as an epic by four engineering agents:

- `/tmp/axon-plan-eng-review/architecture.md`
- `/tmp/axon-plan-eng-review/simplicity.md`
- `/tmp/axon-plan-eng-review/security.md`
- `/tmp/axon-plan-eng-review/performance.md`

Recommendations applied:

- [x] Rename/reframe the plan from "finish everything in one six-lane branch" to recovery triage plus dependency-gated epic execution.
- [x] Treat six lanes as coordination lanes, not six independent code lanes.
- [x] Replace the first implementation wave with a serial critical-path spine.
- [x] Start source-family implementation with local file/directory, not Git.
- [x] Add a blocker/deferral table so optional vertical/tool/search/client work does not silently become a merge blocker.
- [x] Add a cross-lane security contract gate.
- [x] Require auth/config snapshots for every detached job, watch, refresh, prune, reset, and tool-capable path.
- [x] Keep CLI/MCP tool execution fail-closed until sandbox policy is separately proved.
- [x] Add SSRF, local-path, redaction, artifact, reset/prune, and removed-route side-effect-negative requirements.
- [x] Add provider reservation/backpressure and SQLite/Qdrant operational safety gates.
- [x] Make `JobKind::Crawl`, legacy refresh, legacy watch, and `sessions_legacy` explicit disposition decisions before claiming "one operational source pipeline."
- [x] Split surface work into inventory tests first, actual removal only after replacements pass.
- [x] Mark vertical waves, broad app client work, searchwire, tool execution, commerce verticals, memory cleanup, and most performance cleanups as follow-ups unless promoted by a blocker decision.

## Gate Drilldown Applied

This plan has also been updated from the gate-drilldown reports and the external PR/worktree report:

- `/tmp/axon-gate-drilldown/gate0-recovery.md`
- `/tmp/axon-gate-drilldown/gate1-data-safety.md`
- `/tmp/axon-gate-drilldown/gate2-local-source.md`
- `/tmp/axon-gate-drilldown/gate3-runtime.md`
- `/tmp/axon-gate-drilldown/gate4-source-families.md`
- `/tmp/axon-gate-drilldown/gate5-surfaces.md`
- `/tmp/axon-gate-drilldown/gate6-reset-release.md`

Current PR facts to treat as live gates:

- PR #418 is open at `claude/finish-298-wiring`, head `9eae94a9293fc197946df8c0105210731cc70ac7`.
- `.worktrees/finish-298` is the relevant PR worktree and is dirty only at `scripts/cargo-rustc-wrapper`; the root checkout is unrelated-dirty and must not be used for integration.
- Current `origin/main` is `bc13e57dd95c016a8dc4e212aea88cd1d9e71425`; PR #418 is currently `CONFLICTING` / `DIRTY`.
- Local schema reproduction shows `cargo xtask schemas generate --check` fails until `cargo xtask schemas generate --update-fixtures` updates 16 generated files; after that, `generate --check` passes in the disposable reproduction.
- Merge conflict evidence is currently `.monolith-allowlist`, with `Cargo.toml` auto-merging but requiring review.
- CodeRabbit skipped review due to PR size and the previous final-fix loop hit weekly-limit failures, so late dirty-worktree fixes require a final engineering review after integration.

Dirty final-fix worktrees are salvage inputs, not automatically trusted patches:

| Worktree | Required Disposition |
|---|---|
| `.worktrees/wt-fcperf` | Integrate shared TEI/Qdrant `reqwest::Client` only after provider/backpressure tests cover pool reuse. |
| `.worktrees/wt-fcacq` | Integrate concurrent web acquisition, per-item isolation, and visible Chrome/sitemap warnings into Gate 4 web completion. |
| `.worktrees/wt-fcprune` | Reapply prune fail-closed and configured collection ideas compile-wide in Gate 1. |
| `.worktrees/wt-fcsec` | Reapply local auth bypass, lexical local-path classification, execution-time auth recheck, and secret denylist guard in Gates 2/3. |
| `.worktrees/wt-searchwire` | Defer unless Gate 5 removal or search contract proof promotes search/research provider wiring. |
| `.worktrees/wt-toolupload` | Treat CLI/MCP tool and upload adapters as metadata-only/fail-closed; live execute wiring is a later sandbox design item. |
| `.worktrees/wt-enrichprereq` | Integrate as enrichment prerequisite plumbing; it does not restore the lost vertical extractors. |
| `.worktrees/wt-fcsmall` | Integrate migration idempotency, heartbeat logging, user-agent config, and `source_url_audit` contract entry where they match lane ownership. |
| `.worktrees/wt-fcguard` | Fix TEI env migration hints to point to `[embed]`, not `[tei]`. |

## Blocker And Deferral Table

| Work Item | Classification | Why |
|---|---|---|
| Branch recovery, wrapper drift, `.monolith-allowlist`, `Cargo.toml`, schema baseline | blocker | PR #418 cannot be evaluated or merged while conflicting/dirty/schema-red. |
| Dirty final-fix worktree salvage | blocker | Several review fixes exist only as uncommitted worktree diffs and must be consciously integrated or rejected before the PR can be called complete. |
| Payload/retrieval/prune field drift | blocker | Query, retrieve, domains, delete, and prune can silently match zero or delete wrong data. |
| Payload index profile and Qdrant operational shape | blocker | New filter fields must not recreate the prior Qdrant RAM/scan problem. |
| Route/resolver/adapter registry and local source spike | blocker | Source-family ports must prove the mechanical pipeline with the simplest source first. |
| Adapter-owned acquisition for source families claimed by #418 | blocker | The PR cannot claim "adapters own acquisition" while services fetch/clone/dump. |
| Security scope matrix and auth/config snapshots | blocker | Rewired detached jobs and public surfaces must not widen local/admin/execute access. |
| Provider reservations/backpressure fakes | blocker | Background indexing/watch must not starve interactive ask/query or overload TEI/Qdrant/Chrome/LLM. |
| Legacy refresh/watch/session/crawl disposition | blocker | Normal product paths cannot keep hidden second indexing pipelines. |
| Reset/preflight dry-run, config preservation, incompatible-store blocking | blocker | Clean-break cutover depends on safe empty-store reset and clear stale-config detection. |
| Removed-surface inventory tests | blocker | Old surfaces must be known before deletion. |
| Removed-surface actual deletion | release-readiness | Delete only after canonical replacements and generated schemas exist. |
| Web/Palette/Android/Chrome client migrations | release-readiness | Required for shipped clients, but split by client and generated DTO dependency. |
| Full vertical waves | follow-up | Restore as enrichment features after the seam, parser hints, graph candidates, and payload naming are stable. |
| Lost `axon-extract` vertical extractor restore | explicit decision | Cortex evidence says 17 vertical extractors were identified as lost/not relocated. Either restore them in a dedicated vertical epic after `wt-enrichprereq`, or explicitly retire them from #298. |
| Commerce verticals | follow-up | Requires explicit opt-in browser/antibot/SSRF/artifact policy. |
| Upload, CLI tool, and MCP tool live execution | follow-up | Metadata-only or unsupported/fail-closed is acceptable until sandbox proof exists. |
| Search/research provider delegation (`wt-searchwire`) | follow-up | Valuable cleanup unless public contract removal directly depends on it. |
| Broad app polish and memory cleanup | follow-up | Not required for source-pipeline recovery unless touched by the slice. |
| Bead refiling and PR body hygiene | release-readiness | Must happen before final handoff, but should not expand data-path implementation. |

## Critical-Path Spine

Implementation must move through these gates in order. A lane may prepare notes or fixtures early, but code integration follows this spine.

### Gate 0: Recovery Baseline

- [ ] Preserve unrelated root checkout dirt.
- [ ] Create or select a clean integration worktree from `origin/claude/finish-298-wiring` or a successor branch.
- [ ] Prefer a merge from current `origin/main` over rebase unless the coordinator chooses a successor branch with a documented reason.
- [ ] Resolve wrapper drift once: either clean `scripts/cargo-rustc-wrapper` before merge or commit the intentional change separately.
- [ ] Resolve `.monolith-allowlist` toward the PR side only if the monolith helper passes after merge.
- [ ] Review `Cargo.toml` after merge for current-main package metadata plus PR crate/dependency removals.
- [ ] Run `cargo xtask schemas generate --update-fixtures`, inspect the 16 expected generated-file updates, then prove `cargo xtask schemas generate --check`.
- [ ] Push the branch and treat GitHub CI as authoritative because many PR jobs are currently skipped or stale.

### Gate 1: Data Safety

- [ ] Freeze the canonical vector payload contract before touching retrieval: remove or document `source_family`, remove target `source_type`, decide whether `chunk_text` is optional or required, and make `source_generation`/`committed_generation` integer indexes.
- [ ] Preserve target web metadata through vectorization: `web_seed_url`, `web_origin`, `web_path`, `web_normalized_url`, `web_fetch_method`, `normalization_version`, and related approved web fields.
- [ ] Move target canonical URI filters to `item_canonical_uri`, `source_canonical_uri`, `source_item_key`, and `chunk_locator.canonical_uri`; target paths must not depend on `url`, `seed_url`, or `domain`.
- [ ] Move domains, purge/delete, and normal retrieve off retired fields or quarantine the legacy `axon-vector` path as migration-only.
- [ ] Define the target Qdrant payload index profile and indexed field cardinality, including canonical source/document/generation/status/redaction fields and excluding legacy URL/domain/schema fields.
- [ ] Prove prune fails closed on generation-fence lookup errors, generation mismatch, and configured collection-selection errors.
- [ ] Prove prune uses a non-default configured collection end to end and never hardcodes `axon` in normal selectors.
- [ ] Add redaction/classification fixtures for payload, graph evidence, job events, and artifacts, including proof fields and clean/redacted count stamping.

### Gate 2: Route And Local Source Spike

- [ ] Prove `SourceRequest -> resolver -> router -> adapter -> ledger draft -> SourceDocument -> prepare` on local file/directory before any non-local family.
- [ ] Enforce local source policy from the `RoutePlan` before runtime dispatch or data-plane construction; `write` and `admin` do not imply `local`.
- [ ] Thread `RoutePlan` into local execution so local identity, scope, route options, hints, and safety class are not recomputed in the service bridge.
- [ ] Prove `map` and `embed=false` discover/ledger without prepare/embed/vector/publish requirements.
- [ ] Align fixtures around `local://lp_*` identities, `axon:local` missing-scope failures, and one chosen local/code family naming rule.
- [ ] Prove REST, MCP, and worker snapshots deny local paths before service dispatch unless an explicit local/trusted-local context exists.
- [ ] Prove local-source denylist, symlink containment, raw path redaction, and fake embedding/vector boundaries without live TEI/Qdrant.

### Gate 3: Runtime Lifecycle

- [ ] Gate 3A foundations: implement or explicitly scope unified job descriptors, immutable auth/config snapshots, worker-side auth enforcement, provider reservations, waiting/backpressure states, heartbeats, events, and deterministic fakes.
- [ ] Gate 3A auth rule: user-originated detached jobs must carry a real snapshot and cannot fall back to `trusted_system` at worker execution.
- [ ] Gate 3A provider rule: model provider classes for `embedding`, `vector_read`, `vector_write`, `llm`, `fetch`, `render`, `parse`, `graph_write`, and `artifact_write`.
- [ ] Build the legacy disposition table before Gate 3B: normal refresh, `fresh`, legacy URL watch, production `/v1/watch`, `sessions_legacy`, `JobKind::Crawl`, old job/watch tables, and migration/backfill exceptions.
- [ ] Gate 3B cutover: implement source-ledger refresh/watch/session/crawl changes only after the decision table is accepted.
- [ ] Decide whether reset is the one synchronous destructive exception or must return a pollable `job_id`; update contracts and tests before release.

### Gate 4: Source-Family Ports

- [ ] Treat web as completion/verification when working from `.worktrees/finish-298`; it already has provider-shaped acquisition but needs option parity and warnings.
- [ ] Port or verify source families in this order: local parity, web completion, git, feed, registry/package, YouTube, Reddit, sessions, then upload/CLI/MCP fail-closed follow-up.
- [ ] For every claimed family, enforce `resolve -> route -> authorize -> policy check -> credential/config snapshot -> redaction/artifact setup -> provider acquisition`.
- [ ] `source/dispatch.rs` may route and invoke family pipelines, but must not clone, fetch, render, run `yt-dlp`, produce dump paths, or pass prepared acquisition paths.
- [ ] Each source family must pass route/scope, policy-before-side-effects, missing/degraded credential/provider, partial failure, not-modified, payload, redaction, and deterministic-order fixtures.
- [ ] Do not promote upload/CLI/MCP live execution unless the sandbox milestone is explicitly added; metadata-only adapters must advertise execution unsupported.

### Gate 5: Surface Cutover

- [ ] Add removed-surface inventory tests first; do not merge intentionally failing tests.
- [ ] Promote `purge`, `dedupe`, `refresh`, `fresh`, singular `/v1/watch`, and `/v1/watch/{id}/run` to explicit blocker inventory items; acquisition verbs are mostly already gone from public CLI/REST.
- [ ] Prove replacements before deletion: source page/site/local/git/feed/session, query code filters, canonical watches, prune, jobs, and reset.
- [ ] Clean MCP at all three layers: advertised schema, request DTO parsing, and handler dispatch. Published-schema filtering alone is insufficient.
- [ ] Make `surface-removal-contract.md`, `xtask/src/schemas/removed.rs`, `axon_web::schema_registry::removed_routes()`, and `xtask/src/schemas/registry.rs` agree.
- [ ] Add canonical `/v1/watches` routes before deleting legacy `/v1/watch` routes.
- [ ] Delete old CLI/MCP/REST/app surfaces only after canonical replacements pass.
- [ ] Verify old routes/actions fail before side effects if any temporary parser/router stubs remain.
- [ ] Split generated-client refresh from app semantic migrations, then add static route-token gates for Web, Palette, Android, and Chrome extension shipping code.

### Gate 6: Reset And Release Proof

- [ ] Resolve reset job semantics: either make reset job-backed everywhere or update contracts/tests to declare reset the destructive synchronous exception.
- [ ] Implement `axon preflight --config`, one removed-config registry, startup stale-key validation before workers, config rewrite dry-run/execute, reset dry-run, reset execute, and receipts.
- [ ] Preserve `.env` and `config.toml`; never treat config as indexed data or reset candidates.
- [ ] Make reset dry-run inventory exact: per-store SQLite counts, WAL/SHM, legacy blockers, Qdrant target collection shape, artifact root inventory, and redacted would-delete/create summary.
- [ ] Make reset execute receipt-grade: reviewed plan checksum or same-process immutable plan, configured collection only, target named dense + BM42 sparse shape, artifact containment, config preservation, and redacted receipt.
- [ ] Make removed-surface leaks fail CI, not merely warn.
- [ ] Run targeted tier 0-3 tests, selected live smoke against isolated `AXON_DATA_DIR` and unique smoke collection, generated artifacts, full PR CI, release dry run, issue/bead updates, and final review.

## Execution Model

### Coordinator Rules

- [ ] Create or select a clean integration worktree for `claude/finish-298-wiring`.
- [ ] Preserve the unrelated root checkout state before any branch operation.
- [ ] Record progress in `.superpowers/sdd/progress.md` in the integration worktree.
- [ ] Assign exactly one lane owner per file family below.
- [ ] Let lanes run read-only audits or disjoint worktree patches in parallel.
- [ ] Merge lane patches into the integration branch only at gates.
- [ ] After each lane merge, run that lane's targeted verification before another merge.
- [ ] Regenerate schemas/docs only from the integration branch, not from lane branches.
- [ ] Treat interrupted worktrees as salvage material only: inspect, extract minimal diffs, and re-run targeted tests from the integration branch.
- [ ] Require full command logs or `set -o pipefail` for any piped verification command.
- [ ] Run live provider smoke serially against dedicated test data/collections; do not let parallel lanes share production Qdrant/TEI/Chrome/SQLite state.

### Cross-Lane Security Contract Gate

Before any lane can perform side effects, the plan owner must maintain these matrices in the lane report or PR checklist:

- [ ] Operation-to-scope matrix for CLI, MCP, REST, jobs, watches, app clients, and workers.
- [ ] Source-kind-to-policy matrix for local, web, feed, git, registry, YouTube, Reddit, sessions, upload, CLI tool, MCP tool, verticals, search, reset, and prune.
- [ ] Negative tests for `read`/`write`/`admin`/`execute`/`local` separation.
- [ ] Audit events for denied auth, SSRF denied, local path denied, tool execution denied, redaction failure, secret dropped, artifact traversal, destructive dry-run, destructive execute, and credential missing/degraded.
- [ ] `policy_id` or security contract version recorded on security-relevant events.

Minimum side-effect rule:

```text
resolve -> route -> authorize -> policy check -> credential lookup -> redaction/artifact setup -> side effect
```

Side effects include clone, fetch, render, local read, upload unpack, tool/MCP call, reset, prune, Qdrant delete, artifact write, and worker execution.

### Cross-Lane Performance And Backpressure Gate

Before background source/watch/refresh work is enabled:

- [ ] Provider reservation kinds exist or are explicitly scoped: `embedding`, `vector_read`, `vector_write`, `fetch`, `render`, `llm`, `parse`, `graph_write`, and `artifact_write`.
- [ ] Interactive ask/query has priority over background indexing/watch/reindex.
- [ ] Queued reservations expire or cancel when jobs are canceled.
- [ ] Provider cooldown cancels or delays background work without blocking safe cleanup/finalization.
- [ ] SQLite uses WAL/busy-timeout settings appropriate for the unified job/event/ledger/watch workload.
- [ ] Worker channels are bounded.
- [ ] Watch due leasing, job listing, source refresh, cleanup debt, and graph writes have indexes and `EXPLAIN QUERY PLAN` proof where practical.
- [ ] Observability includes provider queue depth/wait time, SQLite busy/retry counts, Qdrant read/write latency, TEI batch/in-flight counts, Chrome active sessions, LLM in-flight count, job heartbeat lag, and watch due backlog.

### Initial Recovery Gate

Run from `/home/jmagar/workspace/axon/.worktrees/finish-298` or a fresh replacement:

```bash
git status --short
git fetch origin
git rev-parse HEAD
git rev-parse origin/main
git merge-tree --name-only HEAD origin/main
RUSTC_WRAPPER= cargo run --manifest-path xtask/Cargo.toml --no-default-features -- schemas generate --check
```

Expected before implementation:

- Branch head starts at PR #418 head `9eae94a9293fc197946df8c0105210731cc70ac7` or a documented successor branch.
- `scripts/cargo-rustc-wrapper` dirt is either intentionally committed once or removed before merge.
- `origin/main` `bc13e57dd95c016a8dc4e212aea88cd1d9e71425` is merged or the successor branch documents why rebase was used.
- `.monolith-allowlist` conflict is resolved with actual monolith output.
- `Cargo.toml` keeps both current main package metadata and PR crate/dependency removals.
- Schema drift is fixed by `cargo xtask schemas generate --update-fixtures`, inspected, committed, then rechecked.

## Lane 1: Branch, PR, Schema, And Integration Control

**Owner:** integration coordinator agent.

**Primary files:**

- `.monolith-allowlist`
- `Cargo.toml`
- `scripts/cargo-rustc-wrapper`
- `.github/workflows/ci.yml`
- `xtask/src/schemas.rs`
- `xtask/tests/fixtures/schemas/**`
- `.superpowers/sdd/progress.md`

**Mission:** make #418 mergeable, keep the integration branch buildable after every lane merge, and own generated schema snapshots.

**Inputs:**

- `/tmp/axon-recovery-audit/pr-ci.md`
- `/tmp/axon-recovery-audit/fix-worktrees.md`
- `docs/pipeline-unification/delivery/implementation-plan.md`
- `docs/pipeline-unification/delivery/dependency-order-map.md`

**Tasks:**

- [ ] Preserve root checkout state outside the integration worktree.
- [ ] Clean or intentionally integrate the wrapper script drift once.
- [ ] Merge `origin/main` into the PR branch unless a successor-branch strategy is explicitly chosen.
- [ ] Resolve `.monolith-allowlist` based on actual monolith output.
- [ ] Review `Cargo.toml` after merge for package metadata plus removed legacy crates.
- [ ] Reproduce schema drift, run `cargo xtask schemas generate --update-fixtures`, inspect the expected 16 generated-file updates, and re-run `generate --check`.
- [ ] Record every dirty final-fix worktree as integrated, rejected, or deferred with a reason.
- [ ] Integrate salvage work in dependency order: `wt-fcsmall`/`wt-fcguard` when low risk, then Gate 1/2/4/5-specific patches only after that gate's tests are ready.
- [ ] Run schema checks after every lane merge; commit generated artifacts only at explicit integration gates.
- [ ] Track expected schema/doc deltas by family: API DTO, MCP, OpenAPI, CLI, vector payload, database, event, graph, provider, config, and generated docs.
- [ ] Keep a queue of lane patches and reject patches that edit another lane's owned files without coordination.

**Verification commands:**

```bash
RUSTC_WRAPPER= cargo run --manifest-path xtask/Cargo.toml --no-default-features -- schemas generate --check
RUSTC_WRAPPER= cargo run --manifest-path xtask/Cargo.toml --no-default-features -- schemas generate --update-fixtures
git diff --check
git status --short
gh pr checks 418 --repo jmagar/axon --watch=false
gh run view <run-id> --repo jmagar/axon --json jobs
```

**Exit criteria:**

- PR branch is no longer `CONFLICTING`.
- `schema-contract-sync` passes locally.
- Integration branch has no incidental wrapper dirt.
- Every final-fix worktree has an explicit disposition in the progress ledger.
- Each merged lane has a progress ledger entry and test evidence.

## Lane 2: Shared Contracts, Payloads, Retrieval, And Prune Safety

**Owner:** contract and data-integrity agent.

**Primary files:**

- `crates/axon-api/src/**`
- `crates/axon-vectors/src/**`
- `crates/axon-retrieval/src/**`
- `crates/axon-prune/src/**`
- `crates/axon-services/src/prune.rs`
- `crates/axon-services/src/source/prune.rs`
- `docs/pipeline-unification/sources/metadata-payload.md`
- `docs/pipeline-unification/schemas/vector-payload-schema.md`

**Mission:** fix the P0/P1 payload and vector field-name breakage, finish prune fail-closed behavior, and make query/retrieve/delete filters match the target payload schema.

**Inputs:**

- `/tmp/axon-recovery-audit/review-findings.md`
- `/tmp/axon-recovery-audit/fix-worktrees.md`
- `docs/pipeline-unification/sources/metadata-payload.md`
- `docs/pipeline-unification/runtime/pruning-contract.md`
- `docs/pipeline-unification/schemas/vector-payload-schema.md`

**Tasks:**

- [ ] Rebuild the absent `wt-fcpayload` fix from first principles.
- [ ] Decide and document `source_family`, `source_type`, `chunk_text`, and generation-field type semantics before coding.
- [ ] Fix schema generation first: target generation fields are integer/null, target required/shared fields do not include retired payload names.
- [ ] Align payload allowlists with canonical source fields: `item_canonical_uri`, `source_canonical_uri`, `source_item_key`, `web_domain`, `source_id`, `job_id`, generation identifiers, document/chunk identifiers, and approved family prefixes.
- [ ] Preserve target web metadata through `web_source/vectorize.rs` instead of stripping it to satisfy stale validation.
- [ ] Replace canonical URI filters with target fields and add target-only fixtures for retrieve/delete.
- [ ] Define the Qdrant payload index profile: indexed fields, expected cardinality, reset-created index shape, and memory impact.
- [ ] Update retrieval, retrieve-by-url, domains, purge/delete, and source filters to stop depending on retired `url`, `seed_url`, `domain`, `source_type`, and `payload_schema_version` payload names.
- [ ] Add payload fixtures that fail when old field names are required for normal query/retrieve/delete behavior.
- [ ] Finish prune from `wt-fcprune` ideas only: update every `PruneTarget` implementation to the new result shape, add `FenceCheckFailed`, thread configured collection through service prune planning and cleanup debt, and fail closed when generation fences cannot be verified.
- [ ] For normal prune/delete, never recreate a collection; destructive collection recreation belongs only in reset with a receipt.
- [ ] Add field classification: unknown adapter metadata defaults non-public, redaction status/version/counts are recorded, and redaction failure blocks vector writes.
- [ ] Add graph evidence and job event visibility tests for payload-derived metadata.
- [ ] Add Qdrant operational tests: bounded batch upserts, no normal-path full collection facets/scrolls, generation-fenced delete selectors, and collection capability/spec caching.
- [ ] Quarantine old `axon-vector` URL/domain behavior as migration-only if it is not removed from normal paths.

**Verification commands:**

```bash
RUSTC_WRAPPER= cargo test -p axon-vectors --no-fail-fast payload_target_required_fields payload_rejects_retired_shared_fields payload_generation_fields_are_integer_or_null web_payload_accepts_normalized_metadata canonical_uri_filter_uses_target_fields required_retrieval_payload_indexes_match_target_profile target_index_profile_excludes_legacy_fields payload_redaction_stamps_proof_fields
RUSTC_WRAPPER= cargo test -p axon-prune --no-fail-fast
RUSTC_WRAPPER= cargo test -p axon-services --no-fail-fast retrieve_works_without_legacy_url_payload domains_uses_web_domain_without_legacy_domain purge_uses_item_canonical_uri_without_legacy_url prune_uses_non_default_collection source_prune_cleanup_debt_uses_configured_collection source_prune_ledger_error_fails_closed graph_evidence_redacts_secret_metadata artifacts_redact_payload_metadata
RUSTC_WRAPPER= cargo test -p axon-jobs --no-fail-fast job_events_redact_payload_metadata
RUSTC_WRAPPER= cargo xtask schemas vector-payload --check
rg -n '"(url|seed_url|domain|source_type|payload_schema_version)"' crates/axon-vectors/src crates/axon-services/src/query crates/axon-services/src/system crates/axon-services/src/prune.rs crates/axon-services/src/source/prune.rs crates/axon-prune/src xtask/src/schemas
```

**Exit criteria:**

- Web and source embeddings are accepted by the vector payload schema.
- Query/retrieve/domains/prune use current payload fields.
- Target fixtures contain no `url`, `seed_url`, `domain`, `source_type`, or `payload_schema_version`.
- Prune cannot delete across an unverified generation fence.
- Prune uses a configured non-default collection end to end.
- Qdrant indexed fields are intentionally bounded and documented.
- Payload, graph evidence, artifacts, and job events do not expose secrets or raw local paths.
- No P0/P1 payload findings remain open.

## Lane 3: Adapter-Owned Acquisition And Source-Family Ports

**Owner:** source adapter agent.

**Primary files:**

- `crates/axon-adapters/src/**`
- `crates/axon-services/src/source/dispatch.rs`
- `crates/axon-services/src/source/{git,feed,reddit,youtube,registry,sessions,web}*.rs`
- `crates/axon-route/src/**`
- `docs/pipeline-unification/sources/adapter-scopes.md`
- `docs/pipeline-unification/sources/new-source-contract.md`

**Mission:** make "adapters own acquisition" actually true for all active source families, not only web.

**Inputs:**

- `/tmp/axon-recovery-audit/legacy-contract.md`
- `/tmp/axon-recovery-audit/followup-worktrees.md`
- `docs/pipeline-unification/sources/adapter-scopes.md`
- `docs/pipeline-unification/foundation/source-pipeline.md`

**Tasks:**

- [ ] Prove the local file/directory source path first: route, adapter capability, ledger draft, `SourceDocument`, document preparation, fake embedding/vector write, and committed generation metadata.
- [ ] Enforce local policy from `RoutePlan.safety_class` before runtime dispatch; `axon:write`, `axon:admin`, and `trusted_tool_execution()` do not imply local filesystem access.
- [ ] Thread the `RoutePlan` into local execution so identity, scope, route options, hints, and safety class are not recomputed from raw paths in the service bridge.
- [ ] Add local path policy tests before any live local smoke: `axon:local` required for REST/MCP, trusted CLI explicitness, `.env` excluded, private keys excluded, `.ssh`, `.codex`, `.gemini`, browser/token/profile paths denied, symlink escape denied, ignored/binary policy honored, and raw absolute paths redacted.
- [ ] Prove local `map` and `embed=false` discover/ledger without vector work.
- [ ] Align local fixtures from `file:///workspace` to the selected `local://lp_*` or documented local identity format, and align missing-scope fixtures to `axon:local`.
- [ ] Treat `wt-fcsec` as a security reference patch for local auth bypass, lexical local-path classification, execution-time auth recheck, and local secret denylist dispatch guard.
- [ ] Treat web as completion from `.worktrees/finish-298`: verify provider-owned acquisition, then thread ETag conditional, WARC, automation script, warning propagation, and deterministic order.
- [ ] Move Git acquisition behind adapter/provider boundaries; `source/dispatch.rs` must not call `clone_git_repo`.
- [ ] Move feed acquisition behind adapter/provider boundaries; `source/dispatch.rs` must not call `fetch_feed_to_file`.
- [ ] Move registry/package acquisition behind adapter/provider boundaries; `source/dispatch.rs` must not call `fetch_registry_dump`.
- [ ] Move YouTube acquisition behind a dedicated constrained `YoutubeProvider`; `source/dispatch.rs` must not call `fetch_youtube_dump`.
- [ ] Move Reddit acquisition behind an authenticated API provider that uses stored credential snapshots; `source/dispatch.rs` must not call `fetch_reddit_dump`.
- [ ] Defer sessions until Lane 4 decides `sessions_legacy`; then move selector/export discovery into the sessions adapter/source pipeline.
- [ ] Keep upload, CLI tool, and MCP tool source dispatch fail-closed unless the sandbox milestone is explicitly promoted into this plan.
- [ ] Metadata-only upload/tool adapters may land only if generated schemas and docs say execution is unsupported.
- [ ] Route every network fetch/render through SSRF policy: private IP denied, redirect-to-private denied, DNS rebinding denied, `file://` denied, localhost/link-local/metadata denied, and Chrome render policy parity proved.
- [ ] Add per-provider batching/caching: per-host concurrency limits, retry-after/backoff, item-level failure isolation, deterministic output order, and not-modified fixtures.
- [ ] Add missing/degraded credential tests and redacted-header/event tests for each source family.
- [ ] Keep `map` as a first-class source scope and transport action.
- [ ] Add typed option schema work for non-web adapters; do not rely only on allowed-key checks and ad hoc bridge inputs.
- [ ] Add provider-boundary inventory before each port: web `FetchProvider`/`RenderProvider`; feed `FetchProvider`; registry typed registry provider; git constrained clone provider; YouTube constrained subprocess provider; Reddit authenticated provider; sessions trusted local export provider.
- [ ] Add partial-failure publication policy: required-source failure blocks publish; optional/item-level failures may publish `completed_degraded` only when the family declares degraded mode.

**Verification commands:**

```bash
rg -n "clone_git_repo|fetch_feed_to_file|fetch_reddit_dump|fetch_youtube_dump|fetch_registry_dump" crates/axon-services/src/source/dispatch.rs
rg -n "repo_root|feed_path|reddit_dump_path|youtube_dump_path|registry_dump_path" crates/axon-adapters/src/{git.rs,feed.rs,reddit.rs,youtube.rs,registry_sources.rs}
RUSTC_WRAPPER= cargo test -p axon-adapters --no-fail-fast
RUSTC_WRAPPER= cargo test -p axon-services --no-fail-fast source dispatch route scope
RUSTC_WRAPPER= cargo test -p axon-route --no-fail-fast
```

Expected `rg` result: no matches in `source/dispatch.rs`.

**Exit criteria:**

- Local source spike passes before non-local ports are integrated.
- Every current source family enters acquisition through an adapter/provider boundary.
- Unsupported scopes fail before acquisition.
- Service dispatch builds route plans and invokes source pipelines, but does not fetch/clone/dump directly.
- Policy checks, redaction setup, and ArtifactStore setup happen before side effects.
- Web, Git, feed, registry, YouTube, Reddit, and sessions preserve `embed=false` and `map` semantics where the source kind supports them.

## Lane 4: Ledger, Jobs, Refresh, Watches, Sessions, Reset, And Observability

**Owner:** runtime lifecycle agent.

**Primary files:**

- `crates/axon-ledger/src/**`
- `crates/axon-jobs/src/**`
- `crates/axon-services/src/source/**`
- `crates/axon-services/src/refresh.rs`
- `crates/axon-services/src/sessions.rs`
- `crates/axon-cli/src/commands/{refresh,sessions,watch,reset,doctor,setup}.rs`
- `crates/axon-web/src/server/handlers/{source_watch,admin,rest}.rs`
- `docs/pipeline-unification/runtime/{job-contract,ledger-contract,observability-contract}.md`
- `docs/pipeline-unification/delivery/cutover-contract.md`

**Mission:** make all detached and recurring work source/job/ledger-owned, then support the clean-slate reset contract.

**Inputs:**

- `/tmp/axon-recovery-audit/legacy-contract.md`
- `docs/pipeline-unification/runtime/job-contract.md`
- `docs/pipeline-unification/runtime/ledger-contract.md`
- `docs/pipeline-unification/delivery/cutover-contract.md`
- `docs/pipeline-unification/delivery/testing-contract.md`

**Tasks:**

- [ ] Create the legacy disposition table before coding this lane:
  - normal refresh Qdrant facet/job-table replay
  - `fresh` and freshness scheduler
  - legacy URL watch execution over `axon_watch_defs`
  - production `/v1/watch`
  - `sessions_legacy`
  - `JobKind::Crawl`
  - legacy job/watch tables
- [ ] Gate 3A: persist real immutable job descriptors with auth snapshot, config snapshot body/id, policy version, normalized request, stage plan, provider requirements, and visibility/redaction ceiling.
- [ ] Gate 3A: reject user-originated worker execution without the stored snapshot; do not silently fallback to `trusted_system`.
- [ ] Gate 3A: add scheduler/stage-bound provider reservations for `embedding`, `vector_read`, `vector_write`, `llm`, `fetch`, `render`, `parse`, `graph_write`, and `artifact_write`.
- [ ] Gate 3A: represent capacity exhaustion as waiting/backpressure state, not generic failure, and preserve interactive ask/query priority.
- [ ] Gate 3A: add deterministic fake stores/providers for job descriptors, auth snapshots, provider reservation, heartbeat, cancellation, and stale recovery tests.
- [ ] Gate 3B: make normal refresh source-ledger driven; Qdrant facet/job-table replay can only survive as explicit admin migration/backfill tooling if kept at all.
- [ ] Replace legacy URL watch execution with source-request-backed scheduling over `axon_source_watches`.
- [ ] Remove production `/v1/watch` legacy routes after `/v1/watches` parity passes.
- [ ] Cut `axon sessions`, session watch, and prepared session ingest over to source pipeline utilities; delete or quarantine `sessions_legacy` entrypoints.
- [ ] Decide internal `JobKind::Crawl` fate: either convert callers to `JobKind::Source` web jobs or explicitly mark crawl as non-indexing maintenance.
- [ ] Resolve reset job semantics: either every detached operation including reset returns a pollable `job_id`, or reset is documented and tested as the synchronous destructive exception.
- [ ] Implement or complete `axon reset --dry-run` and `axon reset --yes` with reset receipts, exact store inventory, config preservation, and fresh schema/Qdrant recreation.
- [ ] Persist and enforce auth/config snapshots for every detached source, watch exec, refresh, sessions, prune, reset, and tool-capable job.
- [ ] Add tests that a job enqueued with only `write` cannot later reset, prune, execute tools, or index local paths.
- [ ] Emit heartbeat/progress events for source stages without log scraping.
- [ ] Add watch parity tests: first-run seed, unchanged skip, changed-page child source job, in-flight coalescing, manual run-now lease, heartbeat-protected long runs, and artifact/history inspection.
- [ ] Add SQLite operational safety: WAL/busy timeout, bounded worker channels, due-watch/job/source/cleanup indexes, batched event/heartbeat writes, retention cleanup, and queue-depth/wait-time observability.
- [ ] Add reset/preflight/config hardening: `axon preflight --config`, startup stale-key failures, config rewrite dry-run, explicit rewrite confirmation, mode/ownership checks for `~/.axon`, SQLite/WAL/SHM, artifacts, logs, screenshots, reset receipts, `.env`, and `config.toml`.
- [ ] Reset/prune tests must cover admin-only access across CLI/MCP/REST/job, confirmation behavior, exact dry-run inventory, configured collection selection, artifact-root canonicalization, symlink escape denial, and assertions that `.env`/`config.toml` are never deleted.
- [ ] `wt-fcperf` can land only after shared TEI/Qdrant client reuse is covered by provider reservation and latency/pool-reuse tests.

**Verification commands:**

```bash
rg -n "/v1/watch|axon_watch_defs|run_watch_now" crates/axon-web/src crates/axon-services/src crates/axon-jobs/src
rg -n "sessions_legacy" crates/axon-cli/src crates/axon-services/src
RUSTC_WRAPPER= cargo test -p axon-jobs --no-fail-fast source watch refresh reset heartbeat
RUSTC_WRAPPER= cargo test -p axon-services --no-fail-fast refresh sessions source_watch reset
RUSTC_WRAPPER= cargo test -p axon-cli --no-fail-fast reset watch sessions
RUSTC_WRAPPER= cargo test -p axon-core --no-fail-fast cutover doctor config
```

Expected `rg` result at final clean-break gate: no production-path matches for legacy watch execution or `sessions_legacy`.

**Exit criteria:**

- Normal refresh, watch exec, and sessions indexing use `SourceRequest`.
- Old job-family tables do not power normal product behavior.
- Reset/preflight can block incompatible non-empty stores and recreate clean stores.
- Job lifecycle tests cover queued/running/completed/failed/degraded/canceled/stale recovery.
- Detached worker execution enforces the original caller auth/config snapshot.
- Background work cannot starve interactive ask/query in fake-boundary tests.
- If reset remains jobless, the exception is reflected in `docs/pipeline-unification/delivery/cutover-contract.md`, `testing-contract.md`, CLI/MCP/REST docs, and tests.

## Lane 5: Enrichment, Verticals, Parse, Graph, And Document Pipeline

**Owner:** enrichment and document pipeline agent.

**Primary files:**

- `crates/axon-adapters/src/enrichment.rs`
- `crates/axon-parse/src/**`
- `crates/axon-document/src/**`
- `crates/axon-graph/src/**`
- `crates/axon-services/src/source/graph.rs`
- `crates/axon-services/src/scrape.rs`
- `docs/pipeline-unification/sources/{parsing-contract,chunking-contract,source-graph,metadata-payload}.md`

**Mission:** establish the enrichment seam, parser-hint bridge, graph-candidate propagation, and metadata safety needed for later vertical waves. Full vertical restoration is a follow-up epic unless explicitly promoted.

**Inputs:**

- `/tmp/axon-recovery-audit/vertical-restore.md`
- `/tmp/axon-recovery-audit/followup-worktrees.md`
- `docs/pipeline-unification/foundation/source-pipeline.md`
- `docs/pipeline-unification/sources/parsing-contract.md`
- `docs/pipeline-unification/sources/source-graph.md`

**Tasks:**

- [ ] Promote the `wt-enrichprereq` `SourceEnricher`, `NoopSourceEnricher`, and fake enricher into the current pipeline.
- [ ] Record that `wt-enrichprereq` is prerequisite plumbing only; it does not restore the lost `axon-extract` vertical extractors.
- [ ] Run enrichment after acquisition/diff and before normalize/prepare for the first proven source family only; broaden after the seam passes fixtures.
- [ ] Merge `SourceEnrichment` into `SourceDocument` metadata, parser hints, chunk hints, graph candidates, warnings, and artifact refs.
- [ ] Thread parser hints into `axon-document` and `axon-parse`; stop always passing `requested_parser: None`.
- [ ] Forward enrichment and parser graph candidates into `SourceGraph`.
- [ ] Resolve canonical payload naming for old `pkg_*` fields versus current `package_*` fields before broad vertical tests.
- [ ] Defer vertical waves unless the PR explicitly promotes them: GitHub repo/issues/releases/PRs; npm/PyPI/crates/docs.rs/Docker/HuggingFace; Hacker News/dev.to/arXiv/Stack Overflow; explicit-only commerce later.
- [ ] Add enrichment safety tests: optional enrichment failures produce `completed_degraded` when allowed, required enrichment failures block publish, graph writes are batched, parser hints do not cause per-chunk parser N+1 behavior, vertical API calls use fetch/LLM/render reservations, and unknown metadata is internal by default.
- [ ] Use ArtifactStore for enrichment artifacts; test traversal rejection, symlink escape rejection, content type/disposition, visibility, hash/byte counts, retention, and no raw local path leakage.
- [ ] Quarantine stale `vertical_scrape` config/docs/schema surfaces until they are truly folded into `source`.
- [ ] Open an explicit decision record for the 17 lost vertical extractors: restore in a vertical epic after the enrichment seam, or retire them from #298.

**Verification commands:**

```bash
RUSTC_WRAPPER= cargo test -p axon-adapters --no-fail-fast enrichment
RUSTC_WRAPPER= cargo test -p axon-parse --no-fail-fast parser graph
RUSTC_WRAPPER= cargo test -p axon-document --no-fail-fast chunk parser_hint
RUSTC_WRAPPER= cargo test -p axon-services --no-fail-fast enrichment graph vertical
```

**Exit criteria:**

- Enrichment is capability-driven, optional unless required by the source scope, and never writes vector points directly.
- Parser hints and graph candidates survive through source documents into parse/graph stores.
- Vertical metadata passes the payload registry tests owned by Lane 2.
- The first enrichment seam is live without requiring full vertical restoration.

## Lane 6: Public Surfaces, Contract Tests, Docs, Beads, And Release Readiness

**Owner:** surface and validation agent.

**Primary files:**

- `crates/axon-cli/src/**`
- `crates/axon-mcp/src/**`
- `crates/axon-web/src/**`
- `apps/**`
- `docs/reference/**`
- `docs/pipeline-unification/**`
- `.github/workflows/**`
- `xtask/src/**`

**Mission:** make users and machines see only the clean-break contract, then prove it through generated artifacts and tests.

**Inputs:**

- `/tmp/axon-recovery-audit/review-findings.md`
- `/tmp/axon-recovery-audit/legacy-contract.md`
- `docs/pipeline-unification/surfaces/command-contract.md`
- `docs/pipeline-unification/surfaces/rest-contract.md`
- `docs/pipeline-unification/surfaces/tool-contract.md`
- `docs/pipeline-unification/delivery/surface-removal-contract.md`
- `docs/pipeline-unification/delivery/testing-contract.md`

**Tasks:**

- [ ] Add removed-surface inventory tests first; do not merge them while intentionally failing.
- [ ] Prove canonical replacements exist before deletion: source create/refresh/map/watch exec/query/prune/reset over CLI `--json`, MCP, REST/OpenAPI, and relevant app clients.
- [ ] Remove or explicitly reclassify old CLI commands from normal help/parser only after replacement proof: `embed`, `ingest`, `scrape`, `crawl`, `code-search`, `code-search-watch`, `purge`, `dedupe`, legacy `refresh`, and legacy `fresh`.
- [ ] Remove old MCP actions from schema, request DTO parsing, and dispatcher only after replacement proof: `embed`, `ingest`, `scrape`, `crawl`, `code_search`, `code_search_watch`, `vertical_scrape`, `purge`, and `dedupe`.
- [ ] Remove old REST routes from router/OpenAPI only after replacement proof: `/v1/embed`, `/v1/ingest`, `/v1/scrape`, `/v1/crawl`, `/v1/purge`, `/v1/dedupe`, `/v1/watch`, and `/v1/watch/{id}/run`.
- [ ] Add canonical plural watch routes, including `/v1/watches/{watch_id}/exec`, before deleting singular watch routes.
- [ ] Add runtime route inventory and side-effect-negative tests for stale APIs, including `/v1/actions`, retained test routers, first-run/admin routes, Android, Palette, Chrome extension, MCP schema, REST OpenAPI, generated docs, and old CLI parser paths.
- [ ] Decide whether `/v1/actions` and `/v1/migrate` are removed entirely or retained as 404-only no-side-effect stubs with no OpenAPI exposure.
- [ ] Make `xtask/src/schemas/removed.rs`, `axon_web::schema_registry::removed_routes()`, and `xtask/src/schemas/registry.rs` agree with `surface-removal-contract.md`.
- [ ] Flip legacy MCP parser tests negative instead of only hiding removed actions from the public schema.
- [ ] Make removed-surface leaks fail CI, not just emit warnings.
- [ ] Update web, Palette, Android, and Chrome-extension clients only where they are part of the release artifact or still call removed routes; split client patches by generated DTO dependency.
- [ ] Regenerate generated clients after server/OpenAPI cutover, then migrate app behavior in separate patches.
- [ ] Add static route-token gates for Web, Palette, Android, and Chrome extension shipping code.
- [ ] Defer `wt-searchwire` unless a removed-surface/schema blocker promotes it; if promoted, include missing-key, pagination/window, provider cooldown, SSRF/search-result policy, and no silent broader-provider fallback tests.
- [ ] Re-file review beads that failed to create during the previous session, using safe quoting and verifying bead IDs.
- [ ] Generate docs and schemas from the final model.
- [ ] Run the tiered testing matrix, selected live smoke tests, full PR CI, and a release workflow dry run before final handoff.
- [ ] Run a final review pass after dirty worktrees, schema artifacts, and generated clients are integrated.

**Verification commands:**

```bash
RUSTC_WRAPPER= cargo run --manifest-path xtask/Cargo.toml --no-default-features -- schemas generate --check
RUSTC_WRAPPER= cargo test --manifest-path xtask/Cargo.toml --no-default-features removed_surface -- --nocapture
RUSTC_WRAPPER= cargo test -p axon-cli --no-fail-fast help removed_commands source
RUSTC_WRAPPER= cargo test -p axon-mcp --no-fail-fast schema removed_actions source
RUSTC_WRAPPER= cargo test -p axon-web --no-fail-fast openapi removed_routes sources
RUSTC_WRAPPER= cargo test --workspace --exclude axon-android --no-fail-fast
RUSTC_WRAPPER= cargo xtask check-api-parity
RUSTC_WRAPPER= cargo xtask check-doc-contracts
RUSTC_WRAPPER= cargo xtask check-openapi-drift
npm --prefix apps/web run openapi:generate
pnpm --dir apps/palette-tauri generate:api
```

Live smoke, only after tiers 0-3 pass:

```bash
SMOKE_ROOT=/tmp/axon-unification-smoke
SMOKE_COLLECTION="axon_unification_smoke_$(date +%Y%m%d%H%M%S)"
AXON_DATA_DIR="$SMOKE_ROOT/data" ./scripts/axon preflight --config --json
AXON_DATA_DIR="$SMOKE_ROOT/data" ./scripts/axon doctor --json --collection "$SMOKE_COLLECTION"
AXON_DATA_DIR="$SMOKE_ROOT/data" timeout 300 ./scripts/axon source /home/jmagar/workspace/axon --scope directory --max-pages 50 --max-depth 2 --exclude-path .git --exclude-path target --exclude-path node_modules --exclude-path .env --wait true --collection "$SMOKE_COLLECTION" --json
AXON_DATA_DIR="$SMOKE_ROOT/data" timeout 180 ./scripts/axon source https://example.com --scope page --max-pages 1 --max-depth 0 --wait true --collection "$SMOKE_COLLECTION" --json
AXON_DATA_DIR="$SMOKE_ROOT/data" ./scripts/axon query "source pipeline" --limit 5 --collection "$SMOKE_COLLECTION" --json
AXON_DATA_DIR="$SMOKE_ROOT/data" ./scripts/axon reset --dry-run --stores jobs,ledger,code_index,watch,graph,memory,vectors,artifacts --collection "$SMOKE_COLLECTION" --json
AXON_DATA_DIR="$SMOKE_ROOT/data" ./scripts/axon reset --yes --stores jobs,ledger,code_index,watch,graph,memory,vectors,artifacts --collection "$SMOKE_COLLECTION" --json
```

**Exit criteria:**

- Removed surfaces are absent from help, MCP schema, OpenAPI, generated clients, and docs.
- Old surfaces fail before side effects if any temporary stubs remain.
- CLI `--json`, MCP, and REST return shared DTO shapes for canonical operations.
- Docs/reference artifacts match generated schemas.
- Beads and PR body describe real remaining follow-ups only.
- Live smoke proves local, web, ask/query, reset, and any claimed non-local source family paths in the intended deployment environment.
- PR #418 or successor is mergeable, has green required checks, and has a final review pass after CodeRabbit's skipped large-PR review.

## Integration Gates

### Gate A: Recovery Complete

- [ ] Integration worktree is clean except intentional changes.
- [ ] PR branch is updated onto current `origin/main`.
- [ ] Schema drift baseline is understood.
- [ ] Lane reports are converted into tasks or explicit deferrals.
- [ ] Every dirty final-fix worktree has an integration/reject/defer note.
- [ ] `schema-contract-sync` passes locally after the generated artifact update.

### Gate B: Core Contract Green

- [ ] Lane 2 payload/retrieval/prune tests pass.
- [ ] Local file/directory spike passes through route, ledger, document, payload, fake providers, and local security policy.
- [ ] Payload index profile is documented and reset-created index shape is known.
- [ ] Layering check shows no new adapter/service/job/document/graph cycles.
- [ ] Target-only fixtures prove no normal read/delete/prune path requires `url`, `seed_url`, `domain`, `source_type`, or `payload_schema_version`.

### Gate B1: Claimed Source Families Green

- [ ] Lane 3 route/scope/policy/fake-provider tests pass for each source family claimed by the PR.
- [ ] No source family acquisition is owned by `source/dispatch.rs`.
- [ ] Each claimed family has credential, SSRF/local/tool policy, redaction, payload, and partial-failure fixtures.

### Gate C: Lifecycle Green

- [ ] Lane 4 source refresh, watches, sessions, jobs, and reset tests pass.
- [ ] Old job/watch/session production paths are gone or explicitly non-product migration tools.
- [ ] Detached operations return pollable `job_id`s, or reset is the one documented synchronous destructive exception.
- [ ] Detached operations enforce stored auth/config snapshots.
- [ ] Provider reservation/backpressure fake tests pass.
- [ ] Legacy disposition table covers refresh, `fresh`, URL watch, `/v1/watch`, `sessions_legacy`, `JobKind::Crawl`, and legacy tables.

### Gate D: Enrichment And Graph Green

- [ ] Lane 5 enrichment seam is live for the first proven source family.
- [ ] Parser hints and graph candidates propagate.
- [ ] Full vertical waves are either deferred or have target-style fixtures and payload/redaction proof.

### Gate E: Surface Cutover Green

- [ ] Lane 6 removed-surface tests pass.
- [ ] CLI/MCP/REST parity tests pass.
- [ ] Generated docs and schemas are clean.
- [ ] `purge`, `dedupe`, `refresh`, `fresh`, singular watch routes, and MCP legacy parser variants are removed or explicitly reclassified with contract updates.
- [ ] Web, Palette, Android, and Chrome extension shipping code have no removed-route tokens unless explicitly out of scope.

### Gate F: Release Readiness

- [ ] Tier 0-3 tests pass.
- [ ] Selected live smoke passes.
- [ ] PR #418 or successor PR has no unresolved blocking review findings.
- [ ] #298 status and beads reflect the final branch truth.
- [ ] Full PR CI is rerun after integration, including jobs previously skipped or unproved: clippy/test/security/rest-api-parity/mcp-smoke/web/android where applicable.
- [ ] Release workflow dry run passes with publishing disabled.

## Agent Dispatch Plan

Use six persistent lane labels, but dispatch fresh agents per task inside each lane:

1. `lane-1-integration`: branch recovery, conflict resolution, schema snapshots, CI.
2. `lane-2-contracts`: payload, retrieval filters, prune safety, data integrity.
3. `lane-3-adapters`: adapter-owned acquisition and source-family ports.
4. `lane-4-runtime`: ledger, jobs, refresh, watches, sessions, reset.
5. `lane-5-enrichment`: enrichment, verticals, parse, graph, document pipeline.
6. `lane-6-surfaces`: CLI/MCP/REST/app surfaces, generated docs, tests, beads.

Recommended first dispatch wave:

- Lane 1: implement recovery branch preparation and schema baseline.
- Lane 2: implement payload/retrieval P0/P1 fix.
- Lane 3: prove route-auth-local file/directory source as the first source-family spike.
- Lane 4: implement Gate 3A job descriptor/event/heartbeat/auth snapshot/provider reservation fakes; no scheduler cutover yet.
- Lane 5: integrate `SourceEnricher` trait/Noop/Fake and runner insertion test on the first proven source family only.
- Lane 6: add removed-surface inventory tests locally; merge only once replacements exist or tests are non-failing.

Do not merge all six first-wave patches blindly. Integrate in this order:

1. Lane 1 recovery baseline.
2. Lane 2 payload and prune safety.
3. Lane 3 local file/directory source spike.
4. Lane 4 job/auth/provider lifecycle foundations.
5. Lane 5 enrichment runner.
6. Lane 6 removed-surface inventory tests.

After the first source family proves the pattern, use this Lane 3 port order:

1. local/file-directory parity check
2. web page/site/docs option completeness and warning propagation, if claimed by #418
3. git/github/gitlab/gitea/generic git, if claimed by #418
4. feed, if claimed by #418
5. registry/package, if claimed by #418
6. YouTube, if claimed by #418
7. Reddit, if claimed by #418
8. sessions, if claimed by #418
9. upload metadata-only or fail-closed follow-up
10. CLI tool metadata-only or fail-closed follow-up
11. MCP tool metadata-only or fail-closed follow-up

## Stop Conditions

- Stop if branch recovery reveals conflicts beyond `.monolith-allowlist` and `Cargo.toml` that change the architecture.
- Stop if the docs themselves contradict; otherwise choose clean break over backward compatibility.
- Stop if non-web acquisition cannot move into adapters without redesigning provider boundaries.
- Stop if reset would delete config files instead of only indexed/runtime stores.
- Stop if generated schemas expose removed public surfaces after cutover.
- Stop if a source-family port requires live tool execution before the sandbox/security milestone is complete.
- Stop if provider reservation/backpressure tests show background work can starve interactive ask/query.

## Final Proof Bundle

The branch is done only when the final handoff includes:

- PR branch, commit range, and mergeability state.
- `git diff --check` output.
- `cargo run --manifest-path xtask/Cargo.toml --no-default-features -- schemas generate --check` output.
- Tier 0-3 test command list with pass/fail status.
- Live smoke command list with pass/fail status and redacted service URLs.
- Removed-surface proof for CLI, MCP, REST, OpenAPI, generated clients, and docs.
- Side-effect-negative proof for old routes/actions that remain as temporary parser/router stubs.
- Payload proof for local, web, git, feed, YouTube, Reddit, registry, sessions, upload, CLI tool, and MCP tool fixtures.
- Reset/preflight receipt examples.
- Dirty final-fix worktree disposition table.
- Legacy disposition table for refresh, `fresh`, URL watch, `/v1/watch`, `sessions_legacy`, `JobKind::Crawl`, and old tables.
- Removed-config registry and config rewrite proof.
- Final review notes covering the dirty worktree integrations and CodeRabbit skipped-review gap.
- Bead status update list.
- PR #418 or successor PR checklist matching `docs/pipeline-unification/delivery/testing-contract.md`.
