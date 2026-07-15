# Finish Pipeline Unification Metaplan

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

Current audited baseline:

- `main` is `df4e832a2` (`Merge pull request #301 from jmagar/codex/session-log-20260630`).
- PR #339 was closed and its branch deleted as superseded by #340; do not resurrect it.
- PR #301 was merged as a docs-only session artifact; it does not add #298 implementation work.
- `gh pr list --state open` returned `[]` after cleanup.
- The previous plan was too narrow because it emphasized Phase 10+ while issue #298 still has unchecked Phase 6, 7, 8, and 9 items.
- Issue #298 was updated in place after the audit: 15 stale checklist items were checked off. This plan now tracks only remaining unchecked or partial work.

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

- `Implemented`: current `main` has direct implementation evidence; issue checkbox is likely stale.
- `Partial`: meaningful implementation exists, but the issue item is broader than current code.
- `Open`: current `main` does not satisfy the issue item.
- `Blocked by audit`: needs one more targeted check before changing issue state.

### Checklist Coverage Ledger

Live issue #298 currently has 127 unchecked bullets from the implementation tracker and lower tracker sections after the 2026-07-04 checklist update. This plan maps them as follows:

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
| Add full, minimal, invalid, and golden snapshot fixtures for every schema family. | Partial/Open | Schema generator contracts and some fixtures exist, but full family coverage is not proven. |
| Add aggregate cross-schema checks for CLI/MCP/OpenAPI -> API, app clients -> OpenAPI/API, enum projections, removed surfaces, config keys, database tables, vector payload indexes, and provider capabilities. | Partial/Open | Some xtask checks exist, including layering and repo-structure, but repo-structure currently fails and removed-surface/config/client drift remains. |

### Phase 6: Source Ledger / Generation / Search

| Issue item | Verdict | Evidence |
|---|---:|---|
| Move/generalize local code-index ledger/generation logic from `axon-code-index` into `axon-ledger`. | Checked stale / follow-up still exists | Issue checkbox is now checked because the ledger ownership moved enough to satisfy the line. Remaining `axon-code-index` local generation/cleanup debt is tracked under crate removal and Task 1 follow-up, not this checkbox. |
| Use committed generations for search. | Partial | Vector payloads stamp `committed_generation = "uncommitted"` before publish in `crates/axon-vectors/src/point.rs`; source tests cover generation reuse. Retrieval currently filters visibility/redaction/source but does not add a committed-generation filter in `crates/axon-retrieval/src/engine.rs:218`. |
| Move stale cleanup out of custom Qdrant scroll paths. | Checked stale / follow-up still exists | Issue checkbox is now checked because source prune/cleanup debt is the shared path. Remaining `axon-code-index` cleanup debt deletion is part of removing the old split crate in Task 1/Task 5. |

### Phase 7: Parse / LLM / Metadata / Graph

| Issue item | Verdict | Evidence |
|---|---:|---|
| Move LLM provider implementations from `axon-core`/`axon-extract` into `axon-llm`. | Implemented with caveat | Runtime providers live under `crates/axon-llm/src/runtime`; `crates/axon-llm/src/lib.rs` states the runtime backends were relocated. `crates/axon-core/src/llm.rs` still owns config DTOs embedded in `Config`, so close the checkbox only with that caveat. |
| Move vertical extraction parse facts into `axon-parse`/`axon-adapters`; keep structured `extract` orchestration in `axon-services`. | Restored with follow-up | The old vertical catalog is restored as `axon-extract` and re-entered at the web adapter acquisition boundary plus the single-page `scrape` fallback path. Auto-dispatch covers GitHub, Reddit, PyPI, npm, crates.io, docs.rs, Docker Hub, Hugging Face, dev.to, Shopify, Hacker News, Stack Overflow, and arXiv; Amazon/eBay remain explicit-only. Vertical parse facts/graph candidates are carried through the web prepare path without leaking private bridge fields into vector payloads. Follow-up: re-home the catalog modules into adapter/parser ownership once the new extractor/enricher boundary is fully designed. |
| Add source-specific metadata registries. | Partial | Vector payload shared/family validation exists in `crates/axon-vectors/src/payload.rs` and `payload_families.rs`; needs source-by-source coverage proof. |
| Implement parser families: code, Rust, JS/TS, Python, Docker, env examples, API schemas, sessions, CLI/MCP tools. | Partial/Open | Production registry includes code symbols for `rs/py/ts/tsx/js/jsx`, manifests, markdown, API schema, session JSONL, and tool-output JSONL. Docker and env example parsers are not in `production_registry()`. |
| Implement chunk profiles, source ranges, fallback visibility, and source-range validation. | Partial | Document chunk profiles include code, manifests, markdown/html, plain text, transcripts, structured records, API schemas, tool output, sessions, and atomic metadata. Need source-range validation audit before marking complete. |
| Implement manifest/runtime/API/schema parsing for package, dependency, service, env, endpoint, schema, and toolchain facts. | Partial/Open | Manifest and API schema parsers exist. Env/service/endpoint/toolchain parsing coverage is not proven complete. |
| Implement CLI/MCP tool output parsing/chunking with side-effect class, allowlist policy, argv/env/output redaction, artifact storage, and external resource graph nodes. | Partial/Open | Tool-output JSONL parser and `ToolOutput` chunk profile exist. Side-effect policy, allowlist, artifact storage, and external resource graph-node coverage still need implementation proof. |
| Implement shared metadata validation before embedding, including vector payload fields, store fields, metadata families, and promotion rules. | Partial | Vector payload validation requires generations, contract version, redaction, visibility, and family fields. Store field sets and promotion rules need targeted proof. |
| Add graph registry validation for canonical node kinds, edge kinds, evidence kinds, merge keys, conflict handling, authority, and confidence. | Implemented/mostly stale | `axon-graph` has closed registries and validation; tests reject unknown kinds and cover authority/conflict behavior. |
| Track ledger/vector/graph links and conflict rules in graph fixtures. | Partial | Graph conflict rules and SQLite graph writes are tested. Ledger/vector/graph fixture linkage needs exact fixture proof. |

### Phase 8: Runtime / Error / Auth / Config / Memory

| Issue item | Verdict | Evidence |
|---|---:|---|
| Implement one durable job table family. | Implemented | Runtime job APIs use the durable `jobs` model with canonical `source`/`extract`/watch/prune/etc. kinds. The terminal jobs migration drops old family job storage, generated database schema artifacts are free of old family tables, and `cargo xtask schemas generate --check` is green. |
| Preserve panic guard behavior. | Implemented | `workers/panic_guard.rs` catches unwind and converts panics into job failures; worker code calls `panic_guard::run_catching`. |
| Preserve cancellation/recovery behavior. | Checked stale / follow-up still exists | Issue checkbox is now checked for preserved behavior. The clean-break unified job-table rewrite still has to carry the same semantics forward and is tracked by the one durable job-table item. |
| Emit progress to CLI, REST/SSE, MCP, logs, traces, and job rows. | Partial/Open | Job rows have `progress_json`; source progress helpers exist. Full CLI/REST/SSE/MCP parity is not proven and likely incomplete. |
| Implement required job fields, status/state-machine values, stage model, parent/child jobs, retry/recovery, retention, and transport job APIs. | Partial/Open | Service job runtime supports list/status/cancel/recover/count; one durable table, parent/child, stage model, event page, and retry API need implementation proof. |
| Ensure every stage result updates job progress and renders through CLI/MCP/REST status surfaces. | Partial/Open | Some source families call progress helpers; full stage-to-surface parity is not proven. |
| Implement structured `ApiError` taxonomy, transport mappings, item-level errors, retry/cooling fields, and redaction-failure handling. | Partial | `axon-error::ApiError` exists and REST renders envelopes. Need provider cooling/retry, item-level, and redaction-failure surface proof. |
| Implement fine-grained auth/security policy for read/write/admin/execute/local, job auth snapshots, SSRF, local path policy, tool execution policy, and audit events. | Partial | `axon-authz` has `Read/Write/Admin/Execute/Local` and fail-closed action mapping. Job auth snapshots and audit events are not proven. |
| Implement redaction detectors, metadata classification, redaction status/version on public writes, and fail-closed behavior before vector/event/artifact output. | Partial | Vector payloads require `redaction_status`; retrieval rejects non-clean or missing status. Redaction version/classification/fail-closed across events/artifacts needs proof. |
| Implement target database schema ownership, indexes, FK integrity, reset behavior, and schema-review fixtures; no old-data migration/backfill. | Partial/Open | Composed migration namespaces include ledger/jobs/observe/graph/memory, but old job tables remain active. |
| Implement target `.env.example` and `config.example.toml` shapes, deprecated env-key diagnostics, config snapshots, and `doctor` effective-config reporting. | Partial | Config snapshot exists. Repo still contains `AXON_MCP_*` docs/config references, so deprecated-key cleanup and docs are not complete. |
| Implement memory lifecycle: DTOs, statuses, scoring, decay, reinforcement, review, supersede/contradict/compact/forget, graph/vector integration, and surface parity. | Partial | SQLite memory store owns lifecycle, decay, reinforcement, review, supersede/contradict/status. Service dispatch exposes remember/list/search/show/link/supersede/context. The contract still requires Qdrant/vector-backed memory plus SQLite metadata and GraphStore links; current service explicitly does not write/read Qdrant memory points. |
| Proof: every detached operation is pollable by `job_id`. | Open | Legacy families are pollable; unified source/watch/prune/etc. job coverage is not complete. |
| Proof: failed/degraded events include structured `ApiError`/warning payloads. | Partial/Open | Source DTOs and errors exist, but full event/surface parity is not proven. |

### Phase 9: Source Families / Tool Sources / Docs

| Issue item | Verdict | Evidence |
|---|---:|---|
| CLI tools/scripts source adapter. | Open/Partial | Tool-output parser exists, but a full CLI tool source adapter with execution policy, redaction, graph nodes, and fixtures is not proven. |
| MCP server/tool calls source adapter. | Open/Partial | MCP action schema references tool surfaces, but full MCP tool-call source adapter evidence is not proven. |
| `axon-memory` integration with shared preparation, payload, graph, and retrieval rules where applicable; memory is not a source adapter. | Partial | Memory is integrated as a service and SQLite store, not as a source adapter. Shared preparation/payload/graph/retrieval integration is not complete. |
| For each source: source-specific fixtures, docs, schemas, onboarding checklist, resolver/adapter/parser/graph/metadata/vector/source-job/degraded/auth/provider failure fixtures. | Open | `new-source-contract.md` requires these fixtures and docs. Current generated schemas exist, but completion across every source family is not proven and should remain unchecked until a per-source matrix passes. |
| Update generated CLI/MCP/REST capability docs and schemas. | Partial/Open | Generated API/client files exist, but stale surfaces and old env names remain; regenerate only after implementation catches up. |

### Phase 10: Surface Cutover

| Issue item | Verdict | Evidence |
|---|---:|---|
| Update CLI/MCP/web to consume only `axon-services`/`axon-api` for source operations. | Partial/Open | CLI uses services for many commands, but legacy public surfaces and generated docs/clients remain. Needs transport import audit before marking complete. |
| Remove transport imports of domain internals before deleting old surfaces. | Blocked by audit | Needs a targeted import/layering audit over `axon-cli`, `axon-mcp`, and `axon-web`. `cargo xtask check-layering` exists but was not run in this audit. |
| Implement `axon <source>`. | Checked stale / compatibility note | Issue checkbox is now checked for the source entrypoint. Before closing Phase 10, verify the final command spelling required by the contract (`axon <source>` versus `axon source <...>`) and update generated help/docs to match the accepted shape. |
| Implement `axon watch <source>`. | Open/Partial | Current `watch` exists, but issue/contract target is source-backed watches. Existing model still includes URL/watch legacy references. |
| Implement `axon watch exec <source>`. | Open/Partial | Current issue target maps to `/v1/watches/{watch_id}/exec`; stale `/v1/watch/{id}/run` references remain in generated clients/docs. |
| Remove old `embed`, `ingest`, `crawl`, `code-search`, `code-search-watch`, `purge`, `dedupe`, and legacy MCP action families from normal public surfaces; restore `axon scrape <url>` only as the canonical one-page SourceRequest projection. | Mostly implemented / follow-up remains | Focused verification now proves `crawl/embed/ingest/scrape/code_search/vertical_scrape` are absent from the MCP schema, `/v1/embed`/`ingest`/`scrape`/`crawl` return 404, `axon crawl` is reserved, and `axon scrape <url>` projects to `SourceRequest { scope=page, embed=true }`. `purge`/`dedupe` cleanup surface follow-up remains under prune cleanup. |
| Update CLI help, `axon --help`, `axon help`, completions, and parse fixtures from generated CLI registry. | Mostly implemented / follow-up remains | CLI scrape/map/source/crawl-reservation fixtures pass and generated CLI docs are refreshed. Remaining cleanup-surface command docs are tracked outside the crawl SourceRequest closeout. |
| Ensure MCP exposes one `axon` tool, strict action/subaction schema, no removed actions, shared envelopes, and action auth metadata. | Implemented for crawl/source cutover | `cargo test -p axon-mcp tool_schema -- --nocapture` and regenerated MCP schema fixtures prove removed indexing actions are absent and `action=source` is the indexing entrypoint. |
| Ensure REST OpenAPI includes every end-state route and excludes every removed route. | Implemented for crawl/source cutover | REST route test proves `/v1/embed`, `/v1/ingest`, `/v1/scrape`, and `/v1/crawl` return 404 while `/v1/sources` is mounted; generated schema/docs were refreshed. |
| Regenerate/copy OpenAPI artifacts for docs, web, Palette clients, and Android assets. | Open | Generated artifacts are stale relative to removal contract. |
| Update web, Android, Palette, and Chrome extension clients to generated DTOs and REST/SSE-only source/job/retrieval flows. | Open/Partial | Palette bridge still allowlists old routes. |
| Add app/client fixture tests for source submission, job progress, ask/query/retrieve, artifacts, redaction, and removed-route absence. | Open | Current evidence shows tests still validating old route shapes in Palette. |
| Add presentation generator/token outputs and token snapshot/accessibility/status-color parity tests. | Blocked by audit | Needs focused generator/test inventory. |
| Update MCP tool schema, REST OpenAPI, and web/Palette/Android/Chrome surfaces. | Open | Stale generated refs remain. |

Proof:

- removed surfaces absent from generated schemas/help: `Open`
- new surfaces map to shared DTOs: `Partial`
- no compatibility aliases remain: `Open`

### Phase 11: Reset, Prune, And Empty-DB Cutover

| Issue item | Verdict | Evidence |
|---|---:|---|
| Move cleanup/dedupe/purge behavior into `axon-prune`. | Partial/Open | `axon-prune` exists and prune plans/cleanup debt are checked in issue text, but public `dedupe`/`purge` commands still dispatch directly. |
| Remove old split crates from workspace: `axon-vector`, `axon-code-index`, `axon-crawl`, `axon-ingest`, `axon-extract`. | Partial / re-scoped | `axon-vector`, `axon-code-index`, `axon-crawl`, and `axon-ingest` are absent from the current workspace. `axon-extract` is intentionally restored as a transitional vertical-extractor catalog so the unified pipeline does not lose source coverage; final closeout should re-home those modules behind adapter/parser ownership, not drop them. |
| Delete or relocate remaining root `src/*` domain modules so root `axon` is bootstrap only. | Blocked by audit | Needs root `src` module ownership check. |
| Implement reset plan/exec with receipts. | Partial/Open | `reset` command exists and is dispatched early; receipt artifact proof is not complete. |
| Make old stores block unified workers until reset / incompatible non-empty stores block unified workers. | Open | Legacy job tables remain active; no old-store blocker proof found. |
| Recreate fresh SQLite schema and Qdrant payload/index shape. | Open/Partial | New vector payload shape exists, but old job schema remains active. |
| Implement `axon preflight --config` stale/removed key reporting. | Open/Partial | `preflight` command exists; specific stale/removed key reporting not proven. |
| Implement `axon setup config rewrite --dry-run`. | Open/Partial | `setup` command exists; exact rewrite dry-run path not proven. |
| Implement `axon reset --dry-run` and `axon reset --yes`. | Partial | CLI has `reset`; dry-run/yes behavior needs test proof. |
| Write reset receipt artifact. | Open | Not proven. |
| Use forward-only schema migrations after new schema lands; no old-data migration/backfill. | Open | New schema has not fully landed because old tables/crates remain. |

Proof:

- Tier 5 cutover tests pass: `Open`
- fresh reindex from empty DB is supported path: `Partial/Open`

### Phase 12: Release Readiness

| Issue item | Verdict | Evidence |
|---|---:|---|
| Generate docs and schemas. | Checked stale / artifacts still need final regeneration | Issue checkbox is now checked for generator existence/runs. Final release still requires regenerated artifacts after the remaining surface removals. |
| Run fake-boundary tests. | Checked stale / final suite still required | Issue checkbox is now checked for existing fake-boundary coverage. Final release still requires the targeted suites listed below after implementation changes. |
| Run selected live smoke tests for local, web, git, ask/query, and reset. | Open | Not run in this audit. |
| Implement Tier 0-5 test model. | Partial/Open | `delivery/testing-contract.md` exists and CI has several gates; full tier model completion is not proven. |
| Default CI runs tiers 0-3. | Partial/Open | CI includes `check-repo-structure` and schema checks; exact Tier 0-3 mapping needs audit. |
| Live smoke is opt-in and skippable. | Blocked by audit | Needs workflow/env audit. |
| Tier 5 cutover cases pass. | Open | Cutover blockers remain. |
| Transport parity matrix covers CLI/MCP/REST. | Open/Partial | Contract/docs exist; current stale surfaces mean parity is not complete. |
| Adapter fixture families exist for web/local/git/registries/feeds/social/video/sessions/CLI/MCP. | Partial/Open | Some adapter fixtures exist; full family matrix not proven. |
| Removed surfaces cannot dispatch. | Open | `purge`/`dedupe` still dispatch; old app/docs routes remain. |
| `cargo xtask docs generate` and `cargo xtask docs generate --check` pass. | Open | Not run in this audit. Related structural gate `cargo xtask check-repo-structure` currently fails because PR0 skeleton rules still reject real `axon-prune` deps/tests. |
| Generated markdown headers, source input manifests, validated examples, and stale-doc CI failure behavior exist. | Partial/Open | Contracts exist; pass state not verified. |
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
| `axon-services` kept as orchestration facade/use-case layer only. | Partial/Open | Services still contain source-family implementation modules and legacy code-search/watch paths. |
| `axon-mcp`, `axon-web`, and `axon-cli` updated to shared DTO/tool schema surfaces only. | Partial/Open | `cargo xtask check-layering` passes, but Phase 10 stale public surfaces and generated artifacts remain. |
| `axon-vector` split into `axon-document`, `axon-embedding`, `axon-vectors`, and `axon-retrieval`. | Mostly implemented / verify reachability | Target crates exist and the old singular `axon-vector` workspace crate is absent. Final closeout still needs surface/retrieval fixture proof. |
| `axon-code-index` split into ledger/parse/document/jobs/vectors. | Mostly implemented / follow-up remains | The old `axon-code-index` workspace crate is absent. Code-search/source-generation behavior still needs final reachability and cleanup-debt proof under the new source pipeline. |
| `axon-crawl` split into adapters/route/ledger/document/jobs. | Implemented for web SourceRequest path | The old `axon-crawl` workspace crate is absent. Site/page/docs web acquisition now runs through Source jobs and the web adapter/source pipeline; the final durable-job schema has no crawl-specific runtime job family. |
| `axon-ingest` split into adapters/route/ledger/document/jobs. | Mostly implemented / verify reachability | The old `axon-ingest` workspace crate is absent. Final closeout still needs source-family fixture proof across feed, registry, Reddit, YouTube, sessions, and hosted git. |
| `axon-extract` split into llm/parse/adapters/services. | Restored with follow-up | Runtime LLM providers moved, and the vertical catalog is restored as `axon-extract` to preserve source coverage. It is wired through web adapter acquisition and single-page scrape fallback; final closeout should re-home catalog modules behind adapter/parser ownership once that boundary is designed. |
| Move/wrap/rewrite/delete rules applied with no legacy facades. | Partial / follow-up remains | Crawl's live runner is removed and `scrape` is restored as a SourceRequest projection. Follow-up remains for non-crawl cleanup/admin legacy surfaces and final per-source fixture breadth. |
| Every new real crate has `src/lib.rs`, `src/CLAUDE.md`, sibling `AGENTS.md`/`GEMINI.md` symlinks, and workspace membership. | Mostly implemented, but gate stale | The target crate set exists. `check-repo-structure` verifies this shape but currently fails on `axon-prune` because the checker still expects PR0 empty deps/no extra test modules. |
| Root `Cargo.toml` contains only target crate set after cutover. | Partial / re-scoped | Workspace contains the target crates plus restored transitional `axon-extract`. `cargo metadata --no-deps` reports 23 `crates/axon-*` members, 25 local packages including root `axon` and `xtask`. |
| Removed crates absent from workspace members after cutover. | Partial / re-scoped | `cargo metadata` no longer lists `axon-vector`, `axon-code-index`, `axon-crawl`, or `axon-ingest`; it does list intentionally restored `axon-extract`. |
| Each target crate has contract docs, agent docs, module ownership, fixtures/fakes, and tests matching crate READMEs. | Partial/Open | Some crate docs/tests exist. Full per-crate contract coverage is not proven. |
| No transport imports domain internals. | Implemented for current allowlist policy | `cargo xtask check-layering` passed with `OK: no new transport->domain-internal reaches.` Existing grandfathered debt remains in the checker allowlist. |
| Public APIs labeled and crate-specific fixtures/generated artifacts added. | Partial/Open | `xtask` public API surface tooling exists; per-crate final artifact coverage is not proven. |
| `cargo xtask check-repo-structure` validates required generated/fixture dirs. | Partial/Open | The command exists but fails in current `main`; it needs updating from PR0 skeleton assumptions to current/end-state rules. |
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
cargo xtask check-layering
PASS: OK: no new transport->domain-internal reaches.

cargo xtask check-repo-structure
FAIL: PR0 target crate axon-prune must keep [dependencies] empty
FAIL: PR0 target crate axon-prune must keep [dev-dependencies] empty
FAIL: axon-prune test modules are rejected as unexpected PR0 module files
```

Interpretation: layering is currently protected against new transport-domain reaches, but repo-structure validation is stale relative to the real `axon-prune` implementation and must be updated before Phase 12 can be closed.

## Open PR Handling

- [x] Close or supersede PR #339 after confirming its intended Phase 10 CLI removals are already on `main` or reapplying only missing pieces. Closed as superseded by #340.
- [x] Decide whether PR #301 should merge as a docs-only session artifact. Merged at `df4e832a2`.

## Implementation Plan

Execution rule: do not treat this as one giant branch. Work in dependency order, update this file and issue #298 after each merged PR, and keep old-crate deletion until the replacement runtime path, generated surfaces, and reset/preflight blockers are proven.

Blocking gates promoted from the contracts/review:

- [ ] Do not delete public surfaces until generated CLI/MCP/REST registries prove removed commands/actions/routes are absent and new surfaces map to shared DTOs.
- [ ] Do not claim surface cutover until the generated route/action/command inventories match `surfaces/command-contract.md`, `surfaces/rest-contract.md`, and `surfaces/tool-contract.md`, including jobs, watches, graph, memory, artifacts, uploads, prune, reset, collections, providers, capabilities, status, doctor, preflight, smoke, and help.
- [ ] Do not collapse job tables until the job contract distinguishes async/detached mutating/provider work from synchronous read paths; `query`/`retrieve` must not create job rows unless they perform long-running provider/artifact work.
- [ ] Do not delete old stores/crates until reset/preflight has dry-run cardinality estimates, chunked execution, checkpointed receipts, and interruption recovery.
- [ ] Do not merge parser/tool-source expansion without fail-closed redaction and source-range validation before vector, graph, event, or artifact writes.
- [ ] Do not expose destructive reset/prune over CLI/MCP/REST without `axon:admin`, dry-run plan IDs, explicit confirmation, audit events, and receipt artifacts.
- [ ] Do not execute CLI/MCP tool sources by default; execution must be explicit, allowlisted, timeout/output-capped, non-shell-expanded, environment-limited, audited, and redacted before persistence.
- [ ] Do not accept web/render/network sources without SSRF and render-provider parity tests for private IPs, redirects, DNS rebinding, loopback/link-local, and `file:`/local schemes.
- [ ] Do not accept local filesystem sources without `axon:local`, symlink-resolved containment, default secret-path denies, and absolute-path redaction.
- [ ] Do not treat Qdrant, jobs, artifacts, or caches as the ledger; `SourceLedger` must remain the system of record for source/item/manifest/generation/document/cleanup state.
- [ ] Do not store large raw content in SQLite or Qdrant payloads; use `ArtifactStore`/`DocumentCache` with artifact metadata, visibility, content hash, byte count, and retention policy.
- [ ] Do not run broad/live verification as the default task check; use targeted fake-boundary/schema checks per task, reserve full workspace/live/Tier 5 checks for cutover gates.

### Task 1: Finish Phase 6 Code Search / Generation Cutover

- [ ] Audit the remaining `axon-code-index` generation and cleanup paths and classify each as `delete after cutover`, `wrap temporarily`, or `port to axon-ledger`.
- [ ] Replace remaining runtime `axon-code-index` generation and cleanup debt ownership with `axon-ledger`/source pipeline equivalents only where the source pipeline is already the active path.
- [ ] Remove legacy `refresh_legacy_code_search_index_with_progress` once target local-source code search is the only path.
- [ ] Make retrieval/search paths consistently exclude uncommitted generations unless explicitly querying staged data.
- [ ] Add/verify Qdrant payload indexes and filters needed for generation-safe search/prune: `source_id`, `generation`, `committed_generation`, `visibility`, and `redaction_status`.
- [ ] Add bounded Qdrant scroll/delete batching for generation prune/reset paths; no unbounded point scans.
- [ ] Delete custom local-code Qdrant generation cleanup once `axon-prune` drains cleanup debt.
- [ ] Ensure unchanged items reuse previous document/vector state by generation reference instead of re-embedding.
- [ ] Ensure cleanup debt order follows the contract: vector deletes, artifact deletes, graph prune, memory prune, ledger prune, job/cache retention.
- [ ] Verify with targeted tests for local source refresh, failed refresh querying last committed generation, and generation-pruned search.
- [ ] Failure guard: failed refresh must keep last committed local-code results searchable; add a regression test before changing generation filters.

Suggested checks:

```bash
cargo test -p axon-services code_search --no-fail-fast
cargo test -p axon-vectors committed_generation --no-fail-fast
cargo test -p axon-retrieval generation --no-fail-fast
```

### Task 2: Finish Phase 7 Parser / Metadata / Graph Gaps

- [ ] Add missing production parser families for Docker files and env examples.
- [ ] Prove or implement source-range validation for every required chunk profile.
- [ ] Complete service/env/endpoint/toolchain fact extraction.
- [ ] Complete CLI/MCP tool-output policies: side-effect class, allowlist, argv/env/output redaction, artifact refs, and external-resource graph nodes.
- [ ] CLI/MCP tool sources default to metadata-only/no-exec mode; any execution path requires explicit opt-in, no shell expansion, command/tool allowlists, environment allowlists, timeout/output caps, and audit metadata.
- [ ] Add source-family metadata registry tests for every adapter family.
- [ ] Promote all source-specific metadata fields into approved namespaces from `sources/metadata-payload.md`; unknown adapter metadata defaults to internal and must not become public by absence of detector hits.
- [ ] Ensure required shared metadata is consistent across ledger, status, vector payloads, artifacts, memory rows, graph evidence, job events, logs/traces, citations, and ask/evaluate traces.
- [ ] Add ledger/vector/graph fixture tests tying source generations, vector points, and graph candidates together.
- [ ] Failure guard: parser-produced graph facts must not publish invalid source ranges; add a fixture that rejects or degrades bad spans.
- [ ] Failure guard: CLI/MCP tool-output ingestion must redact argv/env/stdout/stderr before artifact or vector writes.

Suggested checks:

```bash
cargo test -p axon-parse --no-fail-fast
cargo test -p axon-document --no-fail-fast
cargo test -p axon-vectors payload --no-fail-fast
cargo test -p axon-graph --no-fail-fast
```

### Task 3A: Full Durable Job Cutover

- [ ] Define which operations are job-backed versus synchronous before changing storage. Job-backed: detached/long-running source acquisition, watch, extraction/research/provider work, memory compaction/import, graph mutation, prune, provider_probe, and reset. Synchronous read paths such as normal `query`/`retrieve` must stay jobless unless they perform long-running provider/artifact work.
- [ ] Collapse active job persistence to one durable job table family for the job-backed operations required by the clean-break source path first.
- [ ] Implement the full job status/state-machine contract: `queued`, `pending`, `running`, `waiting`, `blocked`, `canceling`, `completed`, `completed_degraded`, `failed`, `canceled`, `expired`, and `skipped`; invalid transitions fail without mutation.
- [ ] Store required job fields from `runtime/job-contract.md`, including `auth_snapshot`, `config_snapshot_id`, `stage_plan`, `requirements`, `result_schema`, parent/root job ids, attempt number, warnings, and current/terminal `ApiError`.
- [ ] Add SQLite composite indexes for status/list/event access patterns and prove `jobs list/status/events` stays O(page size), not O(total jobs/events).
- [ ] Add cursor pagination, retention pruning, and write-rate limits/coalescing for progress/event rows.
- [ ] Make events append-only with monotonic per-job sequence and resumable `after_sequence` cursors for REST/SSE/MCP/CLI.
- [ ] Preserve panic guard, cancellation, recovery, heartbeat, stale reclaim, and retry semantics in the unified model before removing any legacy job table readers.
- [ ] Add job event pages and progress rendering through CLI, MCP, REST/SSE, and job rows for the minimum source/watch/reset/prune path.
- [ ] Keep full logs/traces parity as follow-up unless needed to make CLI/MCP/REST status correct.
- [ ] Failure guard: a job canceled or recovered before and after migration must remain pollable by the same `job_id`.
- [ ] Failure guard: stale recovery must not double-run provider-heavy stages or double-publish generations while the original attempt is still alive.

### Task 3B: Security, Error, And Memory Completion

- [ ] Add auth snapshots, audit events, and policy enforcement for admin/execute/local/tool paths.
- [ ] Enforce auth snapshots on watches, retries, stale reclaim, child jobs, prune, reset, local, and execute jobs; workers must run with the enqueue-time capability snapshot, not current defaults.
- [ ] Finish `ApiError` event propagation, provider cooling/retry fields, item-level errors, and redaction-failure handling.
- [ ] Finish memory graph/vector/retrieval integration. Contract source says memory is Qdrant/vector-backed with SQLite metadata and graph mirrors; current SQLite-only service is not the target end state.
- [ ] Keep memory as its own job kind, Qdrant namespace/collection payload family, and retrieval policy path; memory must not become a source adapter or pollute source retrieval without explicit memory search/context intent.
- [ ] Complete the memory contract surface, not only remember/search/show: update, reinforce, supersede, contradict, pin, archive, forget, review, compact, import/export, scope graph links, decay profiles, review queues, contradiction penalties, and context token-budget assembly.
- [ ] Enforce memory status rules: forgotten never returns, archived excluded unless requested, superseded returns only explicitly, contradicted returns with warnings, and review memories are lower-confidence.
- [ ] Add memory batch boundaries: embedding/upsert batch size, Qdrant pagination, metadata indexes, graph mirror transaction strategy, and partial-failure recovery.
- [ ] Add old-store blockers/reset behavior for non-empty legacy job tables.
- [ ] Add detector fixtures and fail-closed tests for every public write surface: vector payloads, job events, artifacts, graph evidence, memory, CLI JSON, MCP responses, REST responses, and traces.
- [ ] Emit `RedactionReport`/status data required by the redaction contract: `redaction_status`, `redaction_version`, visibility, redacted/dropped field counts, and detector names for public payload writes.
- [ ] Failure guard: redaction failure must fail closed before vector/event/artifact writes, especially for CLI/MCP tool sources and memory.

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
- [ ] Adapter specs must expose stable adapter name/version, supported source kinds/schemes/shorthands, default scopes, credentials, option schema, parser families, metadata families, watch/refresh support, local/network/render/tool capabilities, degraded modes, and required/optional graph facts.
- [ ] Enforce source adapter batching for prepare/embed/vector/graph writes; avoid item-by-item Qdrant/SQLite writes in source-family ports.
- [ ] Add web/feed/video/registry SSRF fixtures covering private IPs, redirects, DNS rebinding, loopback/link-local, `file:` schemes, and Chrome/render-provider parity.
- [ ] Add local source policy fixtures covering `axon:local`, symlink-resolved containment, default denies for `.env`, SSH/cloud/Codex/Gemini/browser-profile paths, and absolute-path redaction.
- [ ] Complete CLI tool/script source adapter behavior.
- [ ] Complete MCP server/tool-call source adapter behavior.
- [ ] Regenerate CLI/MCP/REST capability docs and schemas after all source families pass.

Suggested checks:

```bash
cargo test -p axon-adapters --no-fail-fast
cargo test -p axon-services source --no-fail-fast
cargo xtask schemas generate --check
```

### Task 5A: Surface Drift And Generated Artifacts

- [ ] Remove stale generated client surfaces and stale `AXON_MCP_*` docs/config references where the clean-break contract requires renamed envs.
- [ ] Build or update generated removal checks before deleting remaining public `purge`/`dedupe`/legacy action surfaces.
- [ ] Remove remaining public `refresh`, `fresh`, `purge`, `dedupe`, and legacy action/route/DTO/config surfaces only after generated schema/help/client/docs absence checks pass.
- [ ] Ensure removed CLI commands are absent and cannot dispatch: `embed`, `ingest`, `crawl`, `code-search`, `code-search-watch`, `purge`, `dedupe`, `refresh`, and `fresh`; ensure `axon scrape <url>` exists only as `SourceRequest { scope=page, embed=true }` plus clean-content output.
- [ ] Ensure removed MCP actions are absent and cannot dispatch: `embed`, `ingest`, `scrape`, `crawl`, `code_search`, `code_search_watch`, `vertical_scrape`, `purge`, and `dedupe`.
- [ ] Ensure removed REST routes are absent from router/OpenAPI/generated clients: `/v1/embed`, `/v1/ingest`, `/v1/scrape`, `/v1/crawl`, `/v1/purge`, `/v1/dedupe`, and `/v1/watch/{id}/run`.
- [ ] Ensure removed DTO fields and config keys from `delivery/surface-removal-contract.md` are absent from generated schemas and fail validation with known replacements.
- [ ] Add negative dispatch tests proving removed CLI commands, MCP actions, and REST routes cannot reach old auth mappings or old handlers.
- [ ] Regenerate and check CLI help, MCP schema, REST OpenAPI, web/Palette/Android assets, and removed-route absence fixtures together.
- [ ] Regenerate and validate every schema family from `schemas/schema-generator-contract.md`: API, CLI, OpenAPI, MCP, config, events, errors, database, graph, vector-payload, and providers; each needs declared inputs, JSON/markdown artifacts, valid/invalid fixtures, golden snapshots, and documented example validation.
- [ ] Generated artifacts must include source input manifests/checksums and fail in `--check` mode without writing.

### Task 5B: Reset / Preflight Cutover

- [ ] Prove `axon reset --dry-run`, `axon reset --yes`, reset receipts, and incompatible-store blockers before deleting old storage code.
- [ ] Require `axon:admin` for destructive reset/prune execution across CLI/MCP/REST; dry-run may be read-only, but execution must bind to a reusable plan ID and explicit confirmation.
- [ ] Emit audit events for reset/prune planning, confirmation, execution, interruption, resume, and completion.
- [ ] Reset/prune dry-run must report cardinality estimates for SQLite rows, artifacts, and Qdrant points before mutation.
- [ ] Reset/prune execution must be chunked, resumable, and receipt-driven; interruption must either resume or leave workers blocked with an actionable receipt.
- [ ] Reset must cover the full clean-slate inventory from `delivery/cutover-contract.md`: jobs, ledger/source/code-index/watch/memory tables, graph, vectors, artifacts, config validation, OAuth/static token guidance, and fresh schema/Qdrant recreation.
- [ ] `doctor`/startup must detect incompatible non-empty stores and block unified workers before side effects until reset or explicit developer override.
- [ ] Preserve/rewrite config intentionally only; never silently discard `.env` or `config.toml`.
- [ ] Prove `axon preflight --config` and `axon setup config rewrite --dry-run` stale/removed-key reporting.
- [ ] Update `cargo xtask check-repo-structure` from PR0 skeleton validation to the current/end-state contract, including real target-crate dependencies, required fixtures/generated artifacts, and removed-crate absence after cutover.

### Task 5C: Old Crate Removal And Final Issue Sync

- [ ] Keep `axon-vector`, `axon-code-index`, `axon-crawl`, and `axon-ingest` absent from the workspace; keep restored `axon-extract` until vertical extractor coverage is re-homed behind adapter/parser ownership with equivalent tests.
- [ ] Remove old crate/module names only after proving no compatibility facades, no transport imports of domain internals, no old code paths reachable from canonical surfaces, and root `axon` is bootstrap only.
- [ ] Run the issue checklist again from Phase 6 through Phase 12 and update issue #298 with exact checked/unchecked changes.
- [ ] File follow-up issues for deferrable hardening that is not required for the clean-break cutover: presentation/token parity, Android/Chrome client parity, full logs/traces status parity, and any memory graph/vector enhancement not required for the current memory contract. Do not defer required all-source fixture completeness if #298 is being marked complete.

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
