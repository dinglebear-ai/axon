# Finish Pipeline Unification Metaplan
Last Modified: 2026-07-16

> For agentic workers: REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development` or `superpowers:executing-plans` before implementation. Keep this file as the working checklist and update item status as evidence changes.

## Goal

Finish GitHub issue #298 by auditing every currently unchecked Phase 6-12 checklist item, marking stale checkboxes only when proven by code, and implementing the genuinely missing clean-break pipeline work.

## Concrete Plan Files

This metaplan is the audit and sequencing layer. Execute the concrete plan files below for implementation detail. The table is the complete concrete plan inventory currently present under `docs/pipeline-unification/plans/`; if a new concrete plan file is added, add it here and map it to issue #298 before using it as closeout evidence.

| Scope | Plan file |
|---|---|
| Phase 1 contract alignment | [`2026-07-04-phase-1-contract-alignment.md`](2026-07-04-phase-1-contract-alignment.md) |
| Phase 2 schema contract alignment | [`2026-07-04-align-phase-2-schema-contracts.md`](2026-07-04-align-phase-2-schema-contracts.md) |
| Phase 3 stores / providers / fakes alignment | [`2026-07-04-align-phase-3.md`](2026-07-04-align-phase-3.md) |
| Phase 4 resolver / router alignment | [`2026-07-04-align-phase-4.md`](2026-07-04-align-phase-4.md) |
| Phase 6 code search / generation cutover | [`2026-07-04-phase-6-code-search-generation-cutover.md`](2026-07-04-phase-6-code-search-generation-cutover.md) |
| Phase 7 parser / metadata / graph gaps | [`2026-07-04-phase-7-parser-metadata-graph-gaps.md`](2026-07-04-phase-7-parser-metadata-graph-gaps.md) |
| Phase 8 / Task 3A full durable job cutover | [`2026-07-04-full-durable-job-cutover.md`](2026-07-04-full-durable-job-cutover.md) |
| Phase 8 / Task 3B security, error, and memory completion | [`2026-07-04-phase-3b-security-error-memory-completion.md`](2026-07-04-phase-3b-security-error-memory-completion.md) |
| Phase 9 / Task 4 source-family matrix and tool sources | [`2026-07-04-phase-9-tool-source-families-matrix.md`](2026-07-04-phase-9-tool-source-families-matrix.md) |
| Phase 10 / Task 5A surface drift and generated artifacts | [`2026-07-04-phase-10-surface-drift-generated-artifacts.md`](2026-07-04-phase-10-surface-drift-generated-artifacts.md) |
| Phase 11 / Task 5B reset and preflight cutover | [`2026-07-04-phase-11-reset-preflight-cutover.md`](2026-07-04-phase-11-reset-preflight-cutover.md) |
| Phase 12 / Task 5C old crate removal and final issue sync | [`2026-07-04-phase-12-old-crate-removal-final-issue-sync.md`](2026-07-04-phase-12-old-crate-removal-final-issue-sync.md) |

Execution order follows `docs/pipeline-unification/delivery/dependency-order-map.md`: finish Phase 1-4 contract/schema/route alignment before Phase 6+ cutover work, finish Phase 6 and Phase 7 before job/security/memory completion, finish source-family ports before surface deletion, finish generated removal checks before deleting public surfaces, and finish reset/preflight blockers before old crate removal.

Historical 2026-07-04 audit baseline:

- `main` is `df4e832a2` (`Merge pull request #301 from jmagar/codex/session-log-20260630`).
- PR #339 was closed and its branch deleted as superseded by #340; do not resurrect it.
- PR #301 was merged as a docs-only session artifact; it does not add #298 implementation work.
- `gh pr list --state open` returned `[]` after cleanup.
- The previous plan was too narrow because it emphasized Phase 10+ while issue #298 still has unchecked Phase 6, 7, 8, and 9 items.
- Issue #298 was updated in place after the audit: 15 stale checklist items were checked off. This plan now tracks only remaining unchecked or partial work.

Live-tree reconciliation (2026-07-16): the active checkout is
`codex/frfr-issue-298-closeout-wave` at `ae7b775a2` with a shared dirty
closeout wave. Evidence below is from that live tree, not the older `main`
snapshot. This pass updates only this metaplan and the delivery implementation
checklist; it does not edit dated task-plan step checklists or mutate trackers.

## Engineering Review Corrections

- Concrete plan references now include Phase 2, Phase 3, and Phase 4 alignment plans. Do not treat them as implicit completed prerequisites unless their verification evidence is recorded.
- Phase 2 closeout must follow the tightened schema plan: every family in `docs/pipeline-unification/schemas/README.md` must be registry-backed, fixture-backed, snapshot-backed, markdown-backed where required, source-input checked, and covered by aggregate cross-checks. `ValidationOnly`, `Deferred`, skeleton artifacts, pseudo snapshots, substring invalid checks, and hard-coded `xtask` mirrors cannot satisfy Phase 2.
- Task 3A is a full durable job cutover, not a minimum cutover. Naming and acceptance criteria must stay aligned with `2026-07-04-full-durable-job-cutover.md`.
- Task 3B security/error/memory completion is a separate implementation track from the durable job cutover. Memory completion is not “minimum durable jobs.”
- Query/retrieve stay jobless unless they perform long-running provider/artifact work; this metaplan rule overrides broader durable-job wording.
- Full all-source fixture completeness cannot be treated as deferrable if this metaplan is used to close #298. Keep #298 open until required fixture/test contracts pass, or explicitly narrow the issue scope before closure.
- Individual phase plans must keep final workspace/live/Tier 5 checks as final cutover gates, not normal task-loop verification.

## Audit Verdicts For Phases 6-9

Status meanings:

- `Implemented`: the current live tree has direct implementation evidence.
- `Partial`: meaningful implementation exists, but the item is broader than current code.
- `Open`: the current live tree does not satisfy the item.
- `Blocked by audit`: needs one more targeted check before changing issue state.

### Checklist Coverage Ledger

The 2026-07-04 issue #298 snapshot had 127 unchecked bullets from the
implementation tracker and lower tracker sections. The counts below preserve
that audit provenance; the active task checklists later in this file are the
2026-07-16 live-tree reconciliation and no tracker was mutated in this pass.

| Issue section | Unchecked count | Plan coverage |
|---|---:|---|
| Phase 0: Contract Freeze And Issue Sync | 1 | Covered in Open PR Handling and Task 5: issue status/follow-up update. |
| Phase 2: Schema Generator And Drift Checks | 2 | Covered by `2026-07-04-align-phase-2-schema-contracts.md`, with final generated docs/schema verification also gated by Phase 12 and Task 5A. |
| Phase 6: Ledger-Owned Source Lifecycle | 1 | Covered one-to-one in Phase 6 audit and Task 1. |
| Phase 7: Document, Parser, Graph, And Payload Pipeline | 10 | Covered one-to-one in Phase 7 audit and Task 2. |
| Phase 8: Unified Jobs And Observability | 13 | Covered one-to-one in Phase 8 audit and Task 3. |
| Phase 9: Port Source Families | 8 | Covered one-to-one in Phase 9 audit and Task 4. |
| Phase 10: Surface Cutover | 21 | Covered in Phase 10 audit and Task 5. |
| Phase 11: Reset, Prune, And Empty-DB Cutover | 14 | Covered in Phase 11 audit and Task 5. |
| Phase 12: Release Readiness | 15 | Covered in Phase 12 audit and Task 5. |
| Crate Creation And Relocation Tracker | 31 | Covered in Crate Creation And Relocation Tracker audit and Task 5. |
| Do-Not-Start Rules | 10 | Covered in Dependency / Order Map And Do-Not-Start Rules. |
| Parallelizable Tracks | 4 | Covered as coordination constraints under Dependency / Order Map and implementation tasks. |
| First Four PRs / PR 1 leftovers | 0 | Completed during the checklist sync; no remaining implementation-plan action. |

The plan intentionally groups the coordination-only bullets instead of repeating every line as an implementation task; the phase and crate tracker items that name concrete missing behavior are tracked directly.

### Phase 0 And Phase 2 Leftovers

| Issue item | Verdict | Evidence |
|---|---:|---|
| Keep the artifact synced as commits land. | Open | This plan is the current artifact and must be updated as PRs land; issue #298 has been updated with the current audit but still needs final status when the remaining work lands. |
| Add full, minimal, invalid, and golden snapshot fixtures for every schema family. | Partial | All 12 registered families have valid/invalid fixtures and snapshots; the invalid fixtures are mostly generic non-object cases and the documented-example acceptance bar is not complete. |
| Add aggregate cross-schema checks for CLI/MCP/OpenAPI -> API, app clients -> OpenAPI/API, enum projections, removed surfaces, config keys, database tables, vector payload indexes, and provider capabilities. | Partial | Layering, repo-structure, docs/removal, and family schema checks exist and the first three pass from the existing xtask binary. App/config drift and a compiling aggregate schema check remain open. |

### Phase 6: Source Ledger / Generation / Search

| Issue item | Verdict | Evidence |
|---|---:|---|
| Move/generalize local code-index ledger/generation logic from `axon-code-index` into `axon-ledger`. | Implemented | `axon-code-index` is absent; source identity, manifests, generations, document status, leases, and cleanup debt are ledger-owned. |
| Use committed generations for search. | Implemented | Qdrant search/retrieve apply committed-generation fences, payload indexes cover the generation fields, and regressions preserve the prior committed generation after failed refresh. |
| Move stale cleanup out of custom Qdrant scroll paths. | Implemented | Generation cleanup uses bounded vector-store delete/scroll helpers and source cleanup debt; no legacy local-code refresh/cleanup symbol remains. |

### Phase 7: Parse / LLM / Metadata / Graph

| Issue item | Verdict | Evidence |
|---|---:|---|
| Move LLM provider implementations from `axon-core`/`axon-extract` into `axon-llm`. | Implemented with caveat | Runtime providers live under `crates/axon-llm/src/runtime`; `crates/axon-llm/src/lib.rs` states the runtime backends were relocated. `crates/axon-core/src/llm.rs` still owns config DTOs embedded in `Config`, so close the checkbox only with that caveat. |
| Move vertical extraction parse facts into `axon-parse`/`axon-adapters`; keep structured `extract` orchestration in `axon-services`. | Restored with follow-up | The old vertical catalog is restored as `axon-extract` and re-entered at the web adapter acquisition boundary plus the single-page `scrape` fallback path. Auto-dispatch covers GitHub, Reddit, PyPI, npm, crates.io, docs.rs, Docker Hub, Hugging Face, dev.to, Shopify, Hacker News, Stack Overflow, and arXiv; Amazon/eBay remain explicit-only. Vertical parse facts/graph candidates are carried through the web prepare path without leaking private bridge fields into vector payloads. Follow-up: re-home the catalog modules into adapter/parser ownership once the new extractor/enricher boundary is fully designed. |
| Add source-specific metadata registries. | Implemented | Adapter specs and vector payload family registries define and validate source-family metadata; registry tests cover the active adapter set. |
| Implement parser families: code, Rust, JS/TS, Python, Docker, env examples, API schemas, sessions, CLI/MCP tools. | Implemented | Docker and env parsers now join the production registry alongside code, manifest, API, session, and tool-output parsers. |
| Implement chunk profiles, source ranges, fallback visibility, and source-range validation. | Implemented | Required profiles exist and parse/document/graph/retrieval validation rejects or degrades invalid ranges before publication. |
| Implement manifest/runtime/API/schema parsing for package, dependency, service, env, endpoint, schema, and toolchain facts. | Implemented | Production fact extraction covers package/dependency plus service, env, endpoint, schema, and toolchain facts. |
| Implement CLI/MCP tool output parsing/chunking with side-effect class, allowlist policy, argv/env/output redaction, artifact storage, and external resource graph nodes. | Implemented | CLI and MCP source adapters enforce explicit execution, allowlists, non-shell argv, environment limits, timeout/output caps, redaction, artifact references, and external-resource facts. |
| Implement shared metadata validation before embedding, including vector payload fields, store fields, metadata families, and promotion rules. | Partial | Boundary validation and approved namespaces exist, but consistency across every ledger/artifact/memory/event/log/citation/trace projection remains unproven. |
| Add graph registry validation for canonical node kinds, edge kinds, evidence kinds, merge keys, conflict handling, authority, and confidence. | Implemented/mostly stale | `axon-graph` has closed registries and validation; tests reject unknown kinds and cover authority/conflict behavior. |
| Track ledger/vector/graph links and conflict rules in graph fixtures. | Implemented | `axon-graph/src/fixture_tests.rs` ties ledger generations, vector points, graph candidates, source ranges, and conflicts together. |

### Phase 8: Runtime / Error / Auth / Config / Memory

| Issue item | Verdict | Evidence |
|---|---:|---|
| Implement one durable job table family. | Implemented | Runtime job APIs use the durable `jobs` model with canonical `source`/`extract`/watch/prune/etc. kinds. The terminal jobs migration drops old family job storage, generated database schema artifacts are free of old family tables, and `cargo xtask schemas generate --check` is green. |
| Preserve panic guard behavior. | Implemented | `workers/panic_guard.rs` catches unwind and converts panics into job failures; worker code calls `panic_guard::run_catching`. |
| Preserve cancellation/recovery behavior. | Checked stale / follow-up still exists | Issue checkbox is now checked for preserved behavior. The clean-break unified job-table rewrite still has to carry the same semantics forward and is tracked by the one durable job-table item. |
| Emit progress to CLI, REST/SSE, MCP, logs, traces, and job rows. | Implemented | Unified events/progress are persisted and rendered through CLI job events, REST/SSE, MCP task progress, and observe/tracing hooks. |
| Implement required job fields, status/state-machine values, stage model, parent/child jobs, retry/recovery, retention, and transport job APIs. | Implemented | The generated schema and job store contain the full lifecycle, attempts, stages, events, heartbeats, artifacts, reservations, snapshots, parent/root IDs, warnings, and terminal errors with state-machine tests. |
| Ensure every stage result updates job progress and renders through CLI/MCP/REST status surfaces. | Implemented | Source/watch/prune job stages publish durable progress and event pages through the shared transports; reset remains a CLI-only surface gap, not a second job store. |
| Implement structured `ApiError` taxonomy, transport mappings, item-level errors, retry/cooling fields, and redaction-failure handling. | Partial | `axon-error::ApiError` exists and REST renders envelopes. Need provider cooling/retry, item-level, and redaction-failure surface proof. |
| Implement fine-grained auth/security policy for read/write/admin/execute/local, job auth snapshots, SSRF, local path policy, tool execution policy, and audit events. | Implemented | Capability policy, enqueue-time auth snapshots, watch/retry/reclaim propagation, local/tool policy, SSRF controls, and audit event wiring are present. |
| Implement redaction detectors, metadata classification, redaction status/version on public writes, and fail-closed behavior before vector/event/artifact output. | Partial | Vector payloads require `redaction_status`; retrieval rejects non-clean or missing status. Redaction version/classification/fail-closed across events/artifacts needs proof. |
| Implement target database schema ownership, indexes, FK integrity, reset behavior, and schema-review fixtures; no old-data migration/backfill. | Implemented | Terminal migrations drop old family/watch/freshness tables; the generated schema contains only unified jobs/watch, ledger, graph, and memory ownership families. |
| Implement target `.env.example` and `config.example.toml` shapes, deprecated env-key diagnostics, config snapshots, and `doctor` effective-config reporting. | Partial | Config snapshot exists. Repo still contains `AXON_MCP_*` docs/config references, so deprecated-key cleanup and docs are not complete. |
| Implement memory lifecycle: DTOs, statuses, scoring, decay, reinforcement, review, supersede/contradict/compact/forget, graph/vector integration, and surface parity. | Implemented with batch follow-up | `VectorBackedMemoryStore` combines Qdrant, SQLite metadata, and graph mirrors; CLI/MCP expose the lifecycle, scoring, review, import/export, and context operations. Qdrant page and graph transaction batch knobs remain reserved. |
| Proof: every detached operation is pollable by `job_id`. | Implemented for active job-backed operations | Source, watch, extraction/research, memory jobs, graph mutation, prune, and provider work use the unified lifecycle; normal query/retrieve remain synchronous by contract. |
| Proof: failed/degraded events include structured `ApiError`/warning payloads. | Partial/Open | Source DTOs and errors exist, but full event/surface parity is not proven. |

### Phase 9: Source Families / Tool Sources / Docs

| Issue item | Verdict | Evidence |
|---|---:|---|
| CLI tools/scripts source adapter. | Implemented | The CLI-tool adapter defaults to metadata-only and gates explicit execution with command/env allowlists, timeout/output caps, non-shell argv, audit metadata, redaction, artifacts, and graph facts. |
| MCP server/tool calls source adapter. | Implemented | The MCP-tool adapter resolves/captures tool calls through the shared source contract with metadata-only policy and redacted output persistence. |
| `axon-memory` integration with shared preparation, payload, graph, and retrieval rules where applicable; memory is not a source adapter. | Implemented; contract superseded (2026-07-16) | Memory remains a distinct vector namespace/retrieval path with SQLite metadata and graph mirrors. The later narrow-adapter decision added one sanctioned projection: the `memory` adapter routes exactly one `memory://mem_<id>` record through shared preparation/publication while `axon-memory` keeps lifecycle and persistence. See `sources/adapter-scopes.md`. |
| For each source: source-specific fixtures, docs, schemas, onboarding checklist, resolver/adapter/parser/graph/metadata/vector/source-job/degraded/auth/provider failure fixtures. | Open | `new-source-contract.md` requires these fixtures and docs. Current generated schemas exist, but completion across every source family is not proven and should remain unchecked until a per-source matrix passes. |
| Update generated CLI/MCP/REST capability docs and schemas. | Partial/Open | Generated API/client files exist, but stale surfaces and old env names remain; regenerate only after implementation catches up. |

### Phase 10: Surface Cutover

| Issue item | Verdict | Evidence |
|---|---:|---|
| Update CLI/MCP/web to consume only `axon-services`/`axon-api` for source operations. | Implemented | Canonical source operations enter through shared API/service DTOs; the current layering check reports no new transport-to-domain-internal reaches. |
| Remove transport imports of domain internals before deleting old surfaces. | Implemented for enforced boundary | `./target/debug/xtask check-layering` passes; the checker continues to make existing allowlisted debt explicit. |
| Implement `axon <source>`. | Implemented | Bare source and explicit source parsing both produce the canonical `SourceRequest`; generated command docs match. |
| Implement `axon watch <source>`. | Implemented | Watches persist a canonical source request plus auth/config snapshots and enqueue unified source jobs on due ticks. |
| Implement `axon watch exec <source>`. | Implemented | CLI/MCP/REST watch execution routes through the canonical watch store and records the resulting source job ID. |
| Remove old `embed`, `ingest`, `crawl`, `code-search`, `code-search-watch`, `purge`, `dedupe`, and legacy MCP action families from normal public surfaces; restore `axon scrape <url>` only as the canonical one-page SourceRequest projection. | Implemented | Generated CLI/MCP/REST inventories and negative dispatch checks omit the removed surfaces; dedupe/purge are prune subactions and scrape is the retained one-page projection. |
| Update CLI help, `axon --help`, `axon help`, completions, and parse fixtures from generated CLI registry. | Implemented | Generated command registry/docs and completion/parse fixtures describe the clean-break surface. |
| Ensure MCP exposes one `axon` tool, strict action/subaction schema, no removed actions, shared envelopes, and action auth metadata. | Partial | The one-tool schema and removed-action checks exist, but generated actions `reset`, `collections`, `artifacts`, `uploads`, and `chat` are not handled by the dispatcher. |
| Ensure REST OpenAPI includes every end-state route and excludes every removed route. | Partial | Removed routes are absent and return 404, but contracted reset plan/exec routes are missing. |
| Regenerate/copy OpenAPI artifacts for docs, web, Palette clients, and Android assets. | Implemented with runtime caveat | Generated artifacts are copied across app clients, but faithfully reflect the current REST omission of reset. |
| Update web, Android, Palette, and Chrome extension clients to generated DTOs and REST/SSE-only source/job/retrieval flows. | Partial | Canonical source/job flows are updated; app settings still expose removed `AXON_MCP_*` and tuning env keys, and reset has no REST client flow. |
| Add app/client fixture tests for source submission, job progress, ask/query/retrieve, artifacts, redaction, and removed-route absence. | Partial | App fixtures cover the canonical routes and removal checks, but the current dirty app suites were not rerun and reset parity is absent. |
| Add presentation generator/token outputs and token snapshot/accessibility/status-color parity tests. | Implemented | The presentation/token generator and app token/accessibility/status-color snapshots are present in the live tree and CI jobs are enabled. |
| Update MCP tool schema, REST OpenAPI, and web/Palette/Android/Chrome surfaces. | Partial | Source/job/removal artifacts are current; MCP dispatch, reset REST, and stale app settings remain. |

Proof:

- removed surfaces absent from generated schemas/help: `Implemented`
- new surfaces map to shared DTOs: `Partial` (reset/runtime dispatch gaps)
- no removed compatibility aliases remain: `Implemented`

### Phase 11: Reset, Prune, And Empty-DB Cutover

| Issue item | Verdict | Evidence |
|---|---:|---|
| Move cleanup/dedupe/purge behavior into `axon-prune`. | Partial/Open | Dedupe/purge are prune subactions, but execution is real only for vector source/generation/collection selectors; artifact, graph, memory, ledger, retention, and cache delete adapters are missing. |
| Remove old split crates from workspace: `axon-vector`, `axon-code-index`, `axon-crawl`, `axon-ingest`, `axon-extract`. | Partial / re-scoped | `axon-vector`, `axon-code-index`, `axon-crawl`, and `axon-ingest` are absent from the current workspace. `axon-extract` is intentionally restored as a transitional vertical-extractor catalog so the unified pipeline does not lose source coverage; final closeout should re-home those modules behind adapter/parser ownership, not drop them. |
| Delete or relocate remaining root `src/*` domain modules so root `axon` is bootstrap only. | Implemented | Root `src/main.rs` and `src/lib.rs` are thin bootstrap/re-export entrypoints. |
| Implement reset plan/exec with receipts. | Partial | CLI reset builds checksum-bound plans, executes with `--yes`, and writes receipts, but it has no resume entrypoint and is absent from REST/MCP dispatch. |
| Make old stores block unified workers until reset / incompatible non-empty stores block unified workers. | Implemented | Doctor/startup compatibility checks block workers on incompatible non-empty stores until reset or explicit developer override. |
| Recreate fresh SQLite schema and Qdrant payload/index shape. | Implemented | Reset recreates the unified SQLite namespaces and target Qdrant collection/index shape. |
| Implement `axon preflight --config` stale/removed key reporting. | Implemented | Preflight reports incompatible and removed configuration before side effects. |
| Implement `axon setup config rewrite --dry-run`. | Implemented | Setup exposes a non-mutating config rewrite preview with stale-key diagnostics. |
| Implement `axon reset --dry-run` and `axon reset --yes`. | Implemented | Dry-run is the default; execution requires `--yes` and a checksum-bound plan. |
| Write reset receipt artifact. | Implemented | Reset records plan/config/auth IDs, inventory checksum, chunk receipts, completion state, and output receipt path. |
| Use forward-only schema migrations after new schema lands; no old-data migration/backfill. | Implemented | Terminal forward migrations drop old job/watch/freshness tables and the generated database schema contains only target families. |

Proof:

- Tier 5 cutover tests pass: `Open`
- fresh reindex from empty DB is supported path: `Implemented`; final live smoke remains open

### Phase 12: Release Readiness

| Issue item | Verdict | Evidence |
|---|---:|---|
| Generate docs and schemas. | Checked stale / artifacts still need final regeneration | Issue checkbox is now checked for generator existence/runs. Final release still requires regenerated artifacts after the remaining surface removals. |
| Run fake-boundary tests. | Checked stale / final suite still required | Issue checkbox is now checked for existing fake-boundary coverage. Final release still requires the targeted suites listed below after implementation changes. |
| Run selected live smoke tests for local, web, git, ask/query, and reset. | Open | Not run in this audit. |
| Implement Tier 0-5 test model. | Partial/Open | `delivery/testing-contract.md` exists and CI has several gates; full tier model completion is not proven. |
| Default CI runs tiers 0-3. | Partial/Open | CI includes `check-repo-structure` and schema checks; exact Tier 0-3 mapping needs audit. |
| Live smoke is opt-in and skippable. | Implemented | Infra/live Qdrant jobs run only on schedule or explicit `workflow_dispatch` inputs; normal CI keeps them skipped. |
| Tier 5 cutover cases pass. | Open | Cutover blockers remain. |
| Transport parity matrix covers CLI/MCP/REST. | Open/Partial | Contract/docs exist; current stale surfaces mean parity is not complete. |
| Adapter fixture families exist for web/local/git/registries/feeds/social/video/sessions/CLI/MCP. | Partial/Open | Some adapter fixtures exist; full family matrix not proven. |
| Removed surfaces cannot dispatch. | Implemented | Generated removal and negative dispatch tests cover removed CLI/MCP/REST surfaces; dedupe/purge exist only as prune subactions. |
| `cargo xtask docs generate` and `cargo xtask docs generate --check` pass. | Partial/Open | Existing `xtask docs check` passes, but compiling `cargo xtask docs generate --check` is blocked by unrelated in-flight `axon-adapters` errors. |
| Generated markdown headers, source input manifests, validated examples, and stale-doc CI failure behavior exist. | Partial | Headers/checksums/check-mode exist; the docs checker found no marked examples, so documented-example validation is not complete. |
| Run mandatory PR reviews. | Open | Not applicable until implementation PRs are ready. |
| Update issue with final status and follow-up issues. | Open | Final status cannot be updated until gaps above close. |

Proof:

- docs match generated artifacts: `Open`
- PR checklist is complete: `Open`
- no known contract gaps remain: `Open`

### Crate Creation And Relocation Tracker

| Issue item | Verdict | Evidence |
|---|---:|---|
| `axon-core` slimmed to config/paths/redaction/utilities/HTTP safety/artifacts only. | Partial/Open | `axon-core` still owns LLM config DTOs (`crates/axon-core/src/llm.rs`) and broad config/runtime support. Some of this is intentional until follow-up split decisions land, but the issue item is not fully proven complete. |
| `axon-services` kept as orchestration facade/use-case layer only. | Partial | Source acquisition moved to adapters and layering passes, but final facade-only ownership across all service modules remains to be reviewed. |
| `axon-mcp`, `axon-web`, and `axon-cli` updated to shared DTO/tool schema surfaces only. | Partial/Open | `cargo xtask check-layering` passes, but Phase 10 stale public surfaces and generated artifacts remain. |
| `axon-vector` split into `axon-document`, `axon-embedding`, `axon-vectors`, and `axon-retrieval`. | Mostly implemented / verify reachability | Target crates exist and the old singular `axon-vector` workspace crate is absent. Final closeout still needs surface/retrieval fixture proof. |
| `axon-code-index` split into ledger/parse/document/jobs/vectors. | Mostly implemented / follow-up remains | The old `axon-code-index` workspace crate is absent. Code-search/source-generation behavior still needs final reachability and cleanup-debt proof under the new source pipeline. |
| `axon-crawl` split into adapters/route/ledger/document/jobs. | Implemented for web SourceRequest path | The old `axon-crawl` workspace crate is absent. Site/page/docs web acquisition now runs through Source jobs and the web adapter/source pipeline; the final durable-job schema has no crawl-specific runtime job family. |
| `axon-ingest` split into adapters/route/ledger/document/jobs. | Mostly implemented / verify reachability | The old `axon-ingest` workspace crate is absent. Final closeout still needs source-family fixture proof across feed, registry, Reddit, YouTube, sessions, and hosted git. |
| `axon-extract` split into llm/parse/adapters/services. | Restored with follow-up | Runtime LLM providers moved, and the vertical catalog is restored as `axon-extract` to preserve source coverage. It is wired through web adapter acquisition and single-page scrape fallback; final closeout should re-home catalog modules behind adapter/parser ownership once that boundary is designed. |
| Move/wrap/rewrite/delete rules applied with no legacy facades. | Partial / follow-up remains | Crawl's live runner is removed and `scrape` is restored as a SourceRequest projection. Follow-up remains for non-crawl cleanup/admin legacy surfaces and final per-source fixture breadth. |
| Every new real crate has `src/lib.rs`, `src/CLAUDE.md`, sibling `AGENTS.md`/`GEMINI.md` symlinks, and workspace membership. | Implemented | The target crate set and transitional `axon-extract` have the required files/symlinks; the current repo-structure check passes. |
| Root `Cargo.toml` contains only target crate set after cutover. | Partial / re-scoped | Workspace contains the target crates plus restored transitional `axon-extract`. `cargo metadata --no-deps` reports 23 `crates/axon-*` members, 25 local packages including root `axon` and `xtask`. |
| Removed crates absent from workspace members after cutover. | Partial / re-scoped | `cargo metadata` no longer lists `axon-vector`, `axon-code-index`, `axon-crawl`, or `axon-ingest`; it does list intentionally restored `axon-extract`. |
| Each target crate has contract docs, agent docs, module ownership, fixtures/fakes, and tests matching crate READMEs. | Partial/Open | Some crate docs/tests exist. Full per-crate contract coverage is not proven. |
| No transport imports domain internals. | Implemented for current allowlist policy | `cargo xtask check-layering` passed with `OK: no new transport->domain-internal reaches.` Existing grandfathered debt remains in the checker allowlist. |
| Public APIs labeled and crate-specific fixtures/generated artifacts added. | Partial/Open | `xtask` public API surface tooling exists; per-crate final artifact coverage is not proven. |
| `cargo xtask check-repo-structure` validates required generated/fixture dirs. | Implemented | `./target/debug/xtask check-repo-structure` passes on the live tree. |
| Keep `axon-core` limited; ban miscellaneous helpers/provider clients. | Partial/Open | Needs targeted module/dependency audit after old split crates are removed. |

### Dependency / Order Map And Do-Not-Start Rules

| Issue item | Verdict | Evidence |
|---|---:|---|
| Source routing/jobs/schemas/removal checks exist before deleting old public surfaces. | Partial/Open | Source and schema scaffolding exists, but public surface deletion and removal checks are incomplete. |
| Old vectors are not searchable through new query path; reset/reindex target payload shape is enforced. | Partial/Open | Target payload shape exists, but old-store blockers/reset cutover are not complete. |
| New sources follow `new-source-contract.md`. | Open | Phase 9 per-source fixture matrix remains incomplete. |
| Schema JSON and generated markdown stay paired. | Partial/Open | Schema/docs generator contracts exist; generated artifacts are stale relative to current removal contract. |
| Broad live smoke waits for fake-boundary proof. | Open | Fake-boundary and live smoke passes have not been run for final release readiness. |

### Verification Results Captured During This Audit

```text
./target/debug/xtask docs check
PASS: 504 Markdown links, 115 removed-surface docs, 110-file inventory

./target/debug/xtask check-layering
PASS: no new transport->domain-internal reaches

./target/debug/xtask check-repo-structure
PASS

cargo xtask docs generate --check
BLOCKED: current shared `axon-adapters` edits do not compile (duplicate session
extension helper, missing `anyhow` wiring, and an invalid self-import).
```

Interpretation: structural docs, layering, and repository shape pass. Compiling
generator/test gates remain unverified because of unrelated in-flight adapter
work; this reconciliation does not repair that shared code.

## Open PR Handling

- [x] Close or supersede PR #339 after confirming its intended Phase 10 CLI removals are already on `main` or reapplying only missing pieces. Closed as superseded by #340.
- [x] Decide whether PR #301 should merge as a docs-only session artifact. Merged at `df4e832a2`.

## Implementation Plan

Execution rule: do not treat this as one giant branch. Work in dependency order, update this file and issue #298 after each merged PR, and keep old-crate deletion until the replacement runtime path, generated surfaces, and reset/preflight blockers are proven.

Blocking gates promoted from the contracts/review:

- [x] Do not delete public surfaces until generated CLI/MCP/REST registries prove removed commands/actions/routes are absent and new surfaces map to shared DTOs.
- [ ] Do not claim surface cutover until the generated route/action/command inventories match `surfaces/command-contract.md`, `surfaces/rest-contract.md`, and `surfaces/tool-contract.md`, including jobs, watches, graph, memory, artifacts, uploads, prune, reset, collections, providers, capabilities, status, doctor, preflight, smoke, and help.
- [x] Do not collapse job tables until the job contract distinguishes async/detached mutating/provider work from synchronous read paths; `query`/`retrieve` must not create job rows unless they perform long-running provider/artifact work.
- [ ] Do not delete old stores/crates until reset/preflight has dry-run cardinality estimates, chunked execution, checkpointed receipts, and interruption recovery.
- [x] Do not merge parser/tool-source expansion without fail-closed redaction and source-range validation before vector, graph, event, or artifact writes.
- [ ] Do not expose destructive reset/prune over CLI/MCP/REST without `axon:admin`, dry-run plan IDs, explicit confirmation, audit events, and receipt artifacts.
- [x] Do not execute CLI/MCP tool sources by default; execution must be explicit, allowlisted, timeout/output-capped, non-shell-expanded, environment-limited, audited, and redacted before persistence.
- [ ] Do not accept web/render/network sources without SSRF and render-provider parity tests for private IPs, redirects, DNS rebinding, loopback/link-local, and `file:`/local schemes.
- [x] Do not accept local filesystem sources without `axon:local`, symlink-resolved containment, default secret-path denies, and absolute-path redaction.
- [x] Do not treat Qdrant, jobs, artifacts, or caches as the ledger; `SourceLedger` must remain the system of record for source/item/manifest/generation/document/cleanup state.
- [ ] Do not store large raw content in SQLite or Qdrant payloads; use `ArtifactStore`/`DocumentCache` with artifact metadata, visibility, content hash, byte count, and retention policy.
- [x] Do not run broad/live verification as the default task check; use targeted fake-boundary/schema checks per task, reserve full workspace/live/Tier 5 checks for cutover gates.

Residual gates: full surface parity is blocked by missing reset REST/MCP dispatch;
reset/prune are not resumable; all-network/render and large-content fixture
coverage has not been rerun in the non-compiling shared tree.

### Task 1: Finish Phase 6 Code Search / Generation Cutover

- [x] Audit the remaining `axon-code-index` generation and cleanup paths and classify each as `delete after cutover`, `wrap temporarily`, or `port to axon-ledger`.
- [x] Replace remaining runtime `axon-code-index` generation and cleanup debt ownership with `axon-ledger`/source pipeline equivalents only where the source pipeline is already the active path.
- [x] Remove legacy `refresh_legacy_code_search_index_with_progress` once target local-source code search is the only path.
- [x] Make retrieval/search paths consistently exclude uncommitted generations unless explicitly querying staged data.
- [x] Add/verify Qdrant payload indexes and filters needed for generation-safe search/prune: `source_id`, `generation`, `committed_generation`, `visibility`, and `redaction_status`.
- [x] Add bounded Qdrant scroll/delete batching for generation prune/reset paths; no unbounded point scans.
- [x] Delete custom local-code Qdrant generation cleanup once `axon-prune` drains cleanup debt.
- [x] Ensure unchanged items reuse previous document/vector state by generation reference instead of re-embedding.
- [x] Ensure cleanup debt order follows the contract: vector deletes, artifact deletes, graph prune, memory prune, ledger prune, job/cache retention.
- [x] Verify with targeted tests for local source refresh, failed refresh querying last committed generation, and generation-pruned search.
- [x] Failure guard: failed refresh must keep last committed local-code results searchable; add a regression test before changing generation filters.

Evidence: `axon-code-index` and its legacy refresh symbol are absent; ledger
generations, committed-generation filters/indexes, bounded server-side deletes,
unchanged-item carry-forward, cleanup ordering, and failed-refresh regressions
are present in ledger/vector/retrieval/source tests.

Suggested checks:

```bash
cargo test -p axon-services code_search --no-fail-fast
cargo test -p axon-vectors committed_generation --no-fail-fast
cargo test -p axon-retrieval generation --no-fail-fast
```

### Task 2: Finish Phase 7 Parser / Metadata / Graph Gaps

- [x] Add missing production parser families for Docker files and env examples.
- [x] Prove or implement source-range validation for every required chunk profile.
- [x] Complete service/env/endpoint/toolchain fact extraction.
- [x] Complete CLI/MCP tool-output policies: side-effect class, allowlist, argv/env/output redaction, artifact refs, and external-resource graph nodes.
- [x] CLI/MCP tool sources default to metadata-only/no-exec mode; any execution path requires explicit opt-in, no shell expansion, command/tool allowlists, environment allowlists, timeout/output caps, and audit metadata.
- [x] Add source-family metadata registry tests for every adapter family.
- [x] Promote all source-specific metadata fields into approved namespaces from `sources/metadata-payload.md`; unknown adapter metadata defaults to internal and must not become public by absence of detector hits.
- [ ] Ensure required shared metadata is consistent across ledger, status, vector payloads, artifacts, memory rows, graph evidence, job events, logs/traces, citations, and ask/evaluate traces.
- [x] Add ledger/vector/graph fixture tests tying source generations, vector points, and graph candidates together.
- [x] Failure guard: parser-produced graph facts must not publish invalid source ranges; add a fixture that rejects or degrades bad spans.
- [x] Failure guard: CLI/MCP tool-output ingestion must redact argv/env/stdout/stderr before artifact or vector writes.

Residual: no single fixture proves every shared metadata field is projected
consistently across all stores, public transports, logs/traces, citations, and
ask/evaluate traces.

Suggested checks:

```bash
cargo test -p axon-parse --no-fail-fast
cargo test -p axon-document --no-fail-fast
cargo test -p axon-vectors payload --no-fail-fast
cargo test -p axon-graph --no-fail-fast
```

### Task 3A: Full Durable Job Cutover

- [x] Define which operations are job-backed versus synchronous before changing storage. Job-backed: detached/long-running source acquisition, watch, extraction/research/provider work, memory compaction/import, graph mutation, prune, provider_probe, and reset. Synchronous read paths such as normal `query`/`retrieve` must stay jobless unless they perform long-running provider/artifact work.
- [x] Collapse active job persistence to one durable job table family for the job-backed operations required by the clean-break source path first.
- [x] Implement the full job status/state-machine contract: `queued`, `pending`, `running`, `waiting`, `blocked`, `canceling`, `completed`, `completed_degraded`, `failed`, `canceled`, `expired`, and `skipped`; invalid transitions fail without mutation.
- [x] Store required job fields from `runtime/job-contract.md`, including `auth_snapshot`, `config_snapshot_id`, `stage_plan`, `requirements`, `result_schema`, parent/root job ids, attempt number, warnings, and current/terminal `ApiError`.
- [x] Add SQLite composite indexes for status/list/event access patterns and prove `jobs list/status/events` stays O(page size), not O(total jobs/events).
- [ ] Add cursor pagination, retention pruning, and write-rate limits/coalescing for progress/event rows.
- [x] Make events append-only with monotonic per-job sequence and resumable `after_sequence` cursors for REST/SSE/MCP/CLI.
- [x] Preserve panic guard, cancellation, recovery, heartbeat, stale reclaim, and retry semantics in the unified model before removing any legacy job table readers.
- [x] Add job event pages and progress rendering through CLI, MCP, REST/SSE, and job rows for the minimum source/watch/reset/prune path.
- [x] Keep full logs/traces parity as follow-up unless needed to make CLI/MCP/REST status correct.
- [x] Failure guard: a job canceled or recovered before and after migration must remain pollable by the same `job_id`.
- [x] Failure guard: stale recovery must not double-run provider-heavy stages or double-publish generations while the original attempt is still alive.

Residual: event/job cursors and retention exist, but watch-store cursor
pagination explicitly rejects cursors and the full progress write-rate/
coalescing requirement is not proven.

### Task 3B: Security, Error, And Memory Completion

- [x] Add auth snapshots, audit events, and policy enforcement for admin/execute/local/tool paths.
- [x] Enforce auth snapshots on watches, retries, stale reclaim, child jobs, prune, reset, local, and execute jobs; workers must run with the enqueue-time capability snapshot, not current defaults.
- [ ] Finish `ApiError` event propagation, provider cooling/retry fields, item-level errors, and redaction-failure handling.
- [x] Finish memory graph/vector/retrieval integration with Qdrant-backed vectors, SQLite metadata, and graph mirrors.
- [x] Keep memory as its own job kind, Qdrant namespace/collection payload family, and retrieval policy path; memory must not become a general-purpose source adapter or pollute source retrieval without explicit memory search/context intent. (Superseded in part, 2026-07-16: the narrow `memory://mem_<id>` single-record projection adapter is the sanctioned exception — see `sources/adapter-scopes.md`.)
- [x] Complete the memory contract surface, not only remember/search/show: update, reinforce, supersede, contradict, pin, archive, forget, review, compact, import/export, scope graph links, decay profiles, review queues, contradiction penalties, and context token-budget assembly.
- [x] Enforce memory status rules: forgotten never returns, archived excluded unless requested, superseded returns only explicitly, contradicted returns with warnings, and review memories are lower-confidence.
- [ ] Add memory batch boundaries: embedding/upsert batch size, Qdrant pagination, metadata indexes, graph mirror transaction strategy, and partial-failure recovery.
- [x] Add old-store blockers/reset behavior for non-empty legacy job tables.
- [ ] Add detector fixtures and fail-closed tests for every public write surface: vector payloads, job events, artifacts, graph evidence, memory, CLI JSON, MCP responses, REST responses, and traces.
- [x] Emit `RedactionReport`/status data required by the redaction contract: `redaction_status`, `redaction_version`, visibility, redacted/dropped field counts, and detector names for public payload writes.
- [x] Failure guard: redaction failure must fail closed before vector/event/artifact writes, especially for CLI/MCP tool sources and memory.

Residual: memory embedding/upsert recovery exists, but Qdrant page size and
graph transaction batch size are explicitly reserved for future use. Full
detector coverage across every listed public write surface and complete
structured error projection remain unproven.

Suggested checks:

```bash
cargo test -p axon-jobs --no-fail-fast
cargo test -p axon-services jobs --no-fail-fast
cargo test -p axon-web jobs --no-fail-fast
cargo test -p axon-mcp jobs --no-fail-fast
cargo test -p axon-memory --no-fail-fast
```

### Task 4: Finish Phase 9 Tool Source Families And Per-Source Matrix

- [ ] Build a source-family matrix covering local, git, web, feed, youtube, reddit, sessions, registry, CLI tool/script, and MCP tool/call sources.
- [ ] For touched source families in this cutover, add resolver, adapter, parser, graph, metadata, vector payload, source-job, degraded, auth, and provider-failure fixtures.
- [ ] Prove full all-family fixture completeness before marking #298 complete. Do not demote the matrix required by `sources/new-source-contract.md`, `delivery/testing-contract.md`, or the tightened Phase 2 schema plan to optional release hardening.
- [ ] For every new/touched source, complete the `sources/new-source-contract.md` onboarding rows: identity, resolver, router, adapter, scopes, ledger, parsing, graph, chunking, metadata, auth/secrets, observability, error handling, tests, and docs.
- [x] Adapter specs must expose stable adapter name/version, supported source kinds/schemes/shorthands, default scopes, credentials, option schema, parser families, metadata families, watch/refresh support, local/network/render/tool capabilities, degraded modes, and required/optional graph facts.
- [x] Enforce source adapter batching for prepare/embed/vector/graph writes; avoid item-by-item Qdrant/SQLite writes in source-family ports.
- [ ] Add web/feed/video/registry SSRF fixtures covering private IPs, redirects, DNS rebinding, loopback/link-local, `file:` schemes, and Chrome/render-provider parity.
- [x] Add local source policy fixtures covering `axon:local`, symlink-resolved containment, default denies for `.env`, SSH/cloud/Codex/Gemini/browser-profile paths, and absolute-path redaction.
- [x] Complete CLI tool/script source adapter behavior.
- [x] Complete MCP server/tool-call source adapter behavior.
- [ ] Regenerate CLI/MCP/REST capability docs and schemas after all source families pass.

Residual: the all-family matrix/onboarding proof remains incomplete, including
the full resolver-to-provider-failure fixture rows and DNS-rebinding/render
parity breadth. Final capability regeneration is therefore still gated.

Suggested checks:

```bash
cargo test -p axon-adapters --no-fail-fast
cargo test -p axon-services source --no-fail-fast
cargo xtask schemas generate --check
```

### Task 5A: Surface Drift And Generated Artifacts

- [ ] Remove stale generated client surfaces and stale `AXON_MCP_*` docs/config references where the clean-break contract requires renamed envs.
- [x] Build or update generated removal checks before deleting remaining public `purge`/`dedupe`/legacy action surfaces.
- [x] Remove remaining public `refresh`, `fresh`, `purge`, `dedupe`, and legacy action/route/DTO/config surfaces only after generated schema/help/client/docs absence checks pass.
- [x] Ensure removed CLI commands are absent and cannot dispatch: `embed`, `ingest`, `crawl`, `code-search`, `code-search-watch`, `purge`, `dedupe`, `refresh`, and `fresh`; ensure `axon scrape <url>` exists only as `SourceRequest { scope=page, embed=true }` plus clean-content output.
- [x] Ensure removed MCP actions are absent and cannot dispatch: `embed`, `ingest`, `scrape`, `crawl`, `code_search`, `code_search_watch`, `vertical_scrape`, `purge`, and `dedupe`.
- [x] Ensure removed REST routes are absent from router/OpenAPI/generated clients: `/v1/embed`, `/v1/ingest`, `/v1/scrape`, `/v1/crawl`, `/v1/purge`, `/v1/dedupe`, and `/v1/watch/{id}/run`.
- [x] Ensure removed DTO fields and config keys from `delivery/surface-removal-contract.md` are absent from generated schemas and fail validation with known replacements.
- [x] Add negative dispatch tests proving removed CLI commands, MCP actions, and REST routes cannot reach old auth mappings or old handlers.
- [ ] Regenerate and check CLI help, MCP schema, REST OpenAPI, web/Palette/Android assets, and removed-route absence fixtures together.
- [ ] Regenerate and validate every schema family from `schemas/schema-generator-contract.md`: API, CLI, OpenAPI, MCP, config, events, errors, database, graph, vector-payload, and providers; each needs declared inputs, JSON/markdown artifacts, valid/invalid fixtures, golden snapshots, and documented example validation.
- [x] Generated artifacts must include source input manifests/checksums and fail in `--check` mode without writing.

Residual: old `AXON_MCP_*` and tuning keys remain in app settings/config docs;
the aggregate generator cannot be rerun while `axon-adapters` does not compile;
and the full schema bar still lacks meaningful per-family invalid/documented
example validation.

### Task 5B: Reset / Preflight Cutover

- [x] Prove `axon reset --dry-run`, `axon reset --yes`, reset receipts, and incompatible-store blockers before deleting old storage code.
- [ ] Require `axon:admin` for destructive reset/prune execution across CLI/MCP/REST; dry-run may be read-only, but execution must bind to a reusable plan ID and explicit confirmation.
- [ ] Emit audit events for reset/prune planning, confirmation, execution, interruption, resume, and completion.
- [ ] Reset/prune dry-run must report cardinality estimates for SQLite rows, artifacts, and Qdrant points before mutation.
- [ ] Reset/prune execution must be chunked, resumable, and receipt-driven; interruption must either resume or leave workers blocked with an actionable receipt.
- [x] Reset must cover the full clean-slate inventory from `delivery/cutover-contract.md`: jobs, ledger/source/code-index/watch/memory tables, graph, vectors, artifacts, config validation, OAuth/static token guidance, and fresh schema/Qdrant recreation.
- [x] `doctor`/startup must detect incompatible non-empty stores and block unified workers before side effects until reset or explicit developer override.
- [x] Preserve/rewrite config intentionally only; never silently discard `.env` or `config.toml`.
- [x] Prove `axon preflight --config` and `axon setup config rewrite --dry-run` stale/removed-key reporting.
- [x] Update `cargo xtask check-repo-structure` from PR0 skeleton validation to the current/end-state contract, including real target-crate dependencies, required fixtures/generated artifacts, and removed-crate absence after cutover.

Residual: reset is CLI-only and has no resume command; prune has no reusable
plan-id receipt and estimates/deletes only its implemented vector boundary.
Consequently cross-transport admin/audit/resume guarantees remain open.

### Task 5C: Old Crate Removal And Final Issue Sync

- [x] Keep `axon-vector`, `axon-code-index`, `axon-crawl`, and `axon-ingest` absent from the workspace; keep restored `axon-extract` until vertical extractor coverage is re-homed behind adapter/parser ownership with equivalent tests.
- [ ] Remove old crate/module names only after proving no compatibility facades, no transport imports of domain internals, no old code paths reachable from canonical surfaces, and root `axon` is bootstrap only.
- [ ] Run the issue checklist again from Phase 6 through Phase 12 and update issue #298 with exact checked/unchecked changes.
- [ ] File follow-up issues for deferrable hardening that is not required for the clean-break cutover: presentation/token parity, Android/Chrome client parity, full logs/traces status parity, and any memory graph/vector enhancement not required for the current memory contract. Do not defer required all-source fixture completeness if #298 is being marked complete.

Residual: root bootstrap and enforced layering pass, but facade/module-name
cleanup is not fully proven. Issue/follow-up mutation remains intentionally
unchecked because this reconciliation was explicitly docs-only and no Beads or
GitHub tracker mutation was requested.

Final verification before claiming #298 complete:

```bash
cargo fmt --all -- --check
cargo xtask check
cargo test --workspace --no-fail-fast
```

Task-level verification should be narrower than the final gate:

- Phase 6: targeted `axon-services`, `axon-vectors`, and `axon-retrieval` generation/prune tests.
- Phase 7/9: parser/source-family fake-boundary tests plus schema generation checks for touched families.
- Phase 8: job storage/event pagination/auth/redaction tests; no live provider dependency by default.
- Phase 10/11: generated removal checks, reset/preflight dry-run tests, and targeted Qdrant/SQLite fake tests.
- Live smoke and full workspace tests are final cutover gates, not the default loop for every small PR.

Before marking #298 complete, run the Tier 5 cutover cases from `delivery/testing-contract.md`: incompatible store block, reset dry-run/yes, removed config validation, removed CLI/MCP/REST absence, old job/code-index/Qdrant payload absence, canonical local and web source reindex, canonical ask/query retrieval from target payloads, and provider backpressure during fresh reindex.

Final issue #298 closeout also requires the tightened Phase 2 schema acceptance bar: all required schema families are registry-backed, all generated JSON and markdown artifacts are produced from the same model, all valid/invalid/golden/example fixtures pass, aggregate CLI/MCP/OpenAPI/API/client/config/database/vector/provider cross-checks pass, and no schema family remains skeleton, validation-only, deferred, or backed by a hand-maintained `xtask` mirror.
