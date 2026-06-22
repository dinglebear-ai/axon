# Workspace Crate Extraction Inventory
Date: 2026-06-20
Branch: codex/crawl-memory-boundaries
Epic: axon_rust-23dw
Status: **Baseline only — no code was moved during the creation of this document.**

---

## 0. Cargo metadata confirmation

```
AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo metadata --format-version 1 --no-deps 2>&1 | head -3
{"packages":[{"name":"xtask","version":"0.1.0","id":"path+file:///…/xtask#0.1.0", ...
{"name":"axon","version":"5.16.5","id":"path+file:///…#axon@5.16.5", ...
```

The repo is a **single-package workspace** today: `[workspace]` with members `["xtask"]`; the main `axon` package at repo root is a member by virtue of being the workspace root. `resolver = "2"` is set. The `apps/palette-tauri/src-tauri` sub-tree declares its own independent `[workspace]` and is **not** a member of the root workspace.

---

## 1. Target crate list

Proposed future crates and their current source owners. The module-root files (`src/<name>.rs`) and the sibling directories (`src/<name>/`) are the extraction units.

| Proposed crate | Source module root(s) | LOC (non-test `.rs`) | Role |
|---|---|---|---|
| `axon-core` | `src/core.rs` + `src/core/` | ~22 000 | Config, HTTP client + SSRF, LLM backends, logging, content transforms, error types, paths, structured data, UI |
| `axon-vector` | `src/vector.rs` + `src/vector/` | ~21 600 | TEI embedding, Qdrant upsert/search/hybrid-RRF, SourceDocument planner, text/code chunking, ranking |
| `axon-services` | `src/services.rs` + `src/services/` | ~20 400 | Typed service façade — the contract boundary between entry points and business logic |
| `axon-cli` | `src/cli.rs` + `src/cli/` | ~11 900 | clap command handlers, output formatting (presentation only) |
| `axon-ingest` | `src/ingest.rs` + `src/ingest/` | ~10 400 | GitHub/GitLab/Gitea/Reddit/YouTube/RSS/sessions ingest drivers |
| `axon-web` | `src/web.rs` + `src/web/` | ~7 800 | Axum HTTP server, panel UI, direct REST routes, auth middleware |
| `axon-jobs` | `src/jobs.rs` + `src/jobs/` | ~7 400 | SQLite-backed async job queue, workers, watch scheduler |
| `axon-mcp` | `src/mcp.rs` + `src/mcp/` | ~6 600 | MCP stdio/HTTP server, schema, action-dispatch handlers |
| `axon-crawl` | `src/crawl.rs` + `src/crawl/` | ~6 400 | Spider-based HTTP/Chrome crawl engine, manifest, sitemap |
| `axon-extract` | `src/extract.rs` + `src/extract/` | ~5 400 | Per-site vertical extractor framework (13 verticals: crates.io, npm, pypi, github_repo, reddit, …) |
| `axon-code-index` | `src/code_index.rs` + `src/code_index/` | ~1 400 | Local Git checkout code-search index (freshness, store, indexer, manifest) |
| `axon-authz` | `src/authz.rs` | ~20 | Scope constants (`axon:read`, `axon:write`) + `scope_satisfies()` |
| `axon` (binary crate) | `src/main.rs` + `src/lib.rs` | ~310 | Entrypoint, `run()`/`run_once()` dispatch, `ServiceContext` wiring |

LOC figures cover only non-test source files (files not ending in `_tests.rs`). Test sidecars are excluded from crate boundary sizing but travel with their parent file.

---

## 2. Cross-module dependency map

Dependency directions derived from `use crate::` imports across each module subtree (test-only files excluded).

### 2.1 Observed import matrix

| Module | Imports from |
|---|---|
| `core` | (no intra-axon deps — pure leaf) |
| `authz` | (no intra-axon deps — pure leaf) |
| `vector` | `core`, `services` (types only: `AskResult`, `QueryHit`, `EvaluateResult`), `ingest` (file-path helpers: `is_indexable_doc_path`, `is_indexable_source_path`) |
| `crawl` | `core` |
| `extract` | `core`, `crawl`, `ingest` |
| `ingest` | `core`, `jobs`, `vector` |
| `code_index` | `core`, `services`, `vector` |
| `jobs` | `core`, `crawl`, `services`, `vector` |
| `services` | `core`, `crawl`, `extract`, `ingest`, `jobs`, `mcp` (schema types), `vector`, `code_index` |
| `mcp` | `authz`, `core`, `extract`, `jobs`, `services`, `vector`, `web` |
| `web` | `authz`, `core`, `jobs`, `mcp` (auth helpers), `services` |
| `cli` | `core`, `crawl`, `extract`, `ingest`, `jobs`, `mcp`, `services`, `vector` |

### 2.2 Flagged dependency cycles

**Cycle 1 — `vector` ↔ `services` (confirmed)**
`src/vector/ops/commands/ask.rs` imports `crate::services::types::AskResult`; `src/services/query.rs` imports `crate::vector::ops::commands::ask::ask_payload`. This is a genuine bi-directional dependency. Resolution: extract the shared type (`AskResult`, `QueryHit`, `EvaluateResult`) into a thin `axon-types` or `axon-contracts` crate that both sides depend on.

**Cycle 2 — `vector` ↔ `ingest` (confirmed)**
`src/vector/ops/file_ingest.rs` imports `crate::ingest::github::{is_indexable_doc_path, is_indexable_source_path}`. `src/ingest/` imports `crate::vector::*` for embedding. Resolution: move the two allowlist predicate functions (`is_indexable_doc_path`, `is_indexable_source_path`) into `axon-core` or a shared `axon-ingest-types` crate so `axon-vector` does not pull in all of `axon-ingest`.

**Cycle 3 — `services` ↔ `mcp` (soft)**
`src/services/` imports MCP schema/request types for ingest source mapping (`src/services/ingest/request.rs`). `src/mcp/server/` imports service functions. Resolution: move the shared request-mapping types out of `src/mcp/` into `axon-services` or a `axon-mcp-types` shim, so `axon-services` never depends on the full MCP crate.

**Cycle 4 — `crawl_engine → services` (documented, blocked)**
`src/crawl/engine/thin_refetch.rs` would need `services` for artifact persistence but `services` imports `crawl`. The `2026-05-21-services-layer-extraction.md` plan already records the resolution: pass a `persist_fn` boxed closure from the service layer into the crawl engine. The engine must never import `services`.

### 2.3 Clean dependencies (no cycles found)

- `core` has zero intra-axon imports — safe to extract first.
- `authz` has zero intra-axon imports — safe to extract as a micro-crate immediately.
- `crawl` imports only `core` — safe to extract after `core`.
- `jobs` imports `core`, `crawl`, `services`, `vector` — must wait for all four.

---

## 3. Public API / client surfaces

All surfaces below must remain stable across the extraction. "Stable" means: no wire-format changes, no removed routes, no changed CLI flag names or JSON field names.

### 3.1 CLI — `axon` binary subcommands

Defined in `src/core/config/types/` (the `CommandKind` enum) and dispatched in `src/lib.rs`. The full set as of this snapshot:

`scrape`, `map`, `endpoints`, `crawl`, `watch`, `monitor`, `extract`, `search`, `embed`, `brand`, `debug`, `diff`, `doctor`, `query`, `code-search`, `retrieve`, `ask`, `summarize`, `evaluate`, `train`, `suggest`, `sources`, `domains`, `stats`, `status`, `dedupe`, `purge`, `refresh`, `ingest`, `memory`, `sessions`, `research`, `screenshot`, `completions`, `mcp`, `serve`, `preflight`/`smoke`/`compose`, `setup`, `migrate`, `config`, `sync`, `update`, `palette`

The extraction must not rename any subcommand, remove any flag, or change any flag's default.

### 3.2 MCP tool schema

Single tool `axon` with `action`/`subaction` routing. Schema source of truth: `src/mcp/schema/` and `src/mcp/schema.rs`. Wire contract: `docs/reference/mcp/tool-schema.md`.

Key schema types in `src/mcp/schema/requests.rs` and `src/mcp/schema/responses.rs` that must not change shape:
- `AxonToolRequest { action, subaction, params }`
- `AxonToolResponse { ok, action, subaction, data }`

Any crate extraction that touches `src/mcp/` must keep these shapes binary-identical.

### 3.3 HTTP/REST endpoints

Canonical source: `src/web/server/routing.rs` and `docs/reference/api-parity.md`. Direct REST under `/v1` is the canonical client/server API.

Stable routes that must not be removed or renamed:

| Route | Auth |
|---|---|
| `GET /healthz` | none |
| `GET /readyz` | none |
| `GET /api-docs/openapi.json` | none |
| `GET /v1/capabilities` | axon:read or axon:write |
| `GET /v1/collections` | axon:read or axon:write |
| `POST /v1/ask`, `POST /v1/ask/stream` | axon:read or axon:write |
| `POST /v1/chat`, `POST /v1/chat/stream` | axon:read or axon:write |
| `POST /v1/crawl`, `GET /v1/crawl`, `GET /v1/crawl/{id}`, etc. | axon:write |
| `POST /v1/embed`, `GET /v1/embed`, `GET /v1/embed/{id}`, etc. | axon:write |
| `POST /v1/extract`, `GET /v1/extract`, etc. | axon:write |
| `POST /v1/ingest`, `GET /v1/ingest`, etc. | axon:write |
| `POST /v1/query` | axon:read or axon:write |
| `POST /v1/search` | axon:read or axon:write |
| `POST /v1/research` | axon:read or axon:write |
| `GET /v1/sources`, `GET /v1/domains`, `GET /v1/stats`, `GET /v1/status` | axon:read or axon:write |
| `POST /v1/scrape`, `POST /v1/summarize`, `POST /v1/screenshot` | axon:read or axon:write |
| `POST /v1/memory` | axon:write |
| `/v1/mobile/sessions*` | axon:read or axon:write |
| `/api/panel/*` | panel token |

The legacy `POST /v1/actions` envelope returns 404 and must not be resurrected.

OpenAPI contract file: `apps/web/openapi/axon.json`. This is the source for generated TypeScript client types and must be regenerated (not hand-edited) after any route change.

### 3.4 Android generated client

Location: `apps/android/`. The generated REST client is derived from `apps/web/openapi/axon.json`. Any extraction that changes an HTTP response shape must regenerate the OpenAPI spec and rebuild the Android client before merging.

Key files:
- `apps/android/app/build.gradle.kts` — `versionName` drives the `android-v*` release tag.
- `apps/android/` — shipping path in `release/components.toml`.

### 3.5 Tauri palette app

Location: `apps/palette-tauri/`. Declares its own independent Cargo workspace (`apps/palette-tauri/src-tauri/Cargo.toml`). Communicates with the axon server over REST. Not directly coupled to `src/` Rust crates; coupling is through the HTTP API surface. Any extraction that changes REST response shapes affects the palette app.

Version files (all three must move together):
- `apps/palette-tauri/src-tauri/tauri.conf.json`
- `apps/palette-tauri/package.json`
- `apps/palette-tauri/src-tauri/Cargo.toml`

### 3.6 Chrome extension

Location: `apps/chrome-extension/`. Shipping path in `release/components.toml`. Version source: `apps/chrome-extension/manifest.json`. Communicates with the axon server over REST. Affected by the same HTTP API surface stability requirements as palette.

### 3.7 Web panel

Location: `apps/web/`. Bundled into the CLI release (shipping path: `apps/web` in `release/components.toml`). Version file: `apps/web/package.json`. OpenAPI contract: `apps/web/openapi/axon.json`. The panel uses `/api/panel/*` routes (panel-token auth) that are excluded from `/v1` parity accounting but must remain stable for the panel to function.

---

## 4. Edit-ownership matrix

Files that multiple extraction beads will touch simultaneously. Parallel workers must coordinate on these or sequence through them.

| File / section | Touched by | Risk |
|---|---|---|
| `Cargo.toml` (root) | Every extraction bead that adds a new `crates/<name>/` member | High — concurrent edits will conflict. Serialize all `[workspace] members` additions through a single gatekeeper bead. |
| `Cargo.toml` `[dependencies]` | Every bead whose crate needs a dependency already held at root | High — must de-duplicate transitive deps into workspace-level `[workspace.dependencies]` once. |
| `src/lib.rs` | Bead that migrates `run()`/`run_once()` to use new crate paths; bead that splits CLI dispatch | Medium — only two structural changes needed; sequence after individual module extractions are stable. |
| `src/services/CLAUDE.md` | Beads touching the services layer contract (cycle-break bead, type-extraction bead) | Low — doc-only; update after each services change. |
| `src/vector/CLAUDE.md` | Beads touching `axon-vector` extraction | Low — doc-only. |
| `src/core/config/types/config.rs` (865 LOC) | Every bead that adds or removes a `Config` field; the `axon-core` extraction bead | High — this file is a known shared struct literal; any non-`Option` field addition requires updating test helpers in `src/cli/commands/research.rs`, `src/cli/commands/search.rs`, and `src/jobs/common/` helpers. See CLAUDE.md gotcha "Adding fields to Config struct". |
| `apps/web/openapi/axon.json` | Any bead that changes an HTTP response DTO shape | High — regenerate from `cargo xtask generate-openapi` (or equivalent); never hand-edit. |
| `vendor/lab-auth/` | Bead that updates rusqlite or sqlx versions | High — the vendored lab-auth patch exists to resolve a `links = "sqlite3"` conflict between rusqlite 0.32 and sqlx-sqlite 0.8; any dependency bump touching either must re-validate the patch. |
| `release/components.toml` | Bead that adds a new shippable binary or changes shipping paths | Medium — must also update `.github/workflows/auto-tag.yml` if a new release workflow is added. |
| Compatibility re-export shims (`pub use`) | The bead that performs each module extraction | Medium — required to keep all existing `use axon::vector::*` call sites compiling without a mass update pass. Shims live in the old location and `pub use` from the new crate path. |
| `src/mcp/schema/` | Cycle-break bead (services ↔ mcp) and MCP extraction bead | High — shared request/response types must be moved without changing their `serde` wire names. |

---

## 5. Security invariants

These must be preserved byte-for-byte across all extraction work. They must not be refactored, silenced, or moved to a different startup path without an explicit security review.

### 5.1 SSRF / DNS-aware URL validation

**File:** `src/core/http/ssrf.rs`

The primary SSRF guard for all outbound HTTP. `validate_url()` rejects private IP ranges, loopback, and cloud metadata endpoints. `validate_url_with_dns()` adds a blocking DNS resolution step for use before handing URLs to non-reqwest fetchers.

Invariants to preserve:
- Both functions must be called on every user-supplied URL before any outbound connection attempt.
- The test-only `ALLOW_LOOPBACK` thread-local bypass (`#[cfg(test)]`) must never exist in production builds.
- The `LoopbackGuard` RAII type must only be available under `#[cfg(test)]`.
- When `axon-core` is extracted as a crate, `ssrf.rs` must remain in that crate (not inlined into `axon-crawl` or `axon-vector`), because `axon-vector` also needs SSRF protection for Qdrant/TEI URL construction.

Relevant note from CLAUDE.md: The `firewall` Spider feature flag is intentionally NOT enabled because `spider_firewall`'s build.rs fetches GitHub blocklists unauthenticated. `validate_url()` in `src/core/http/ssrf.rs` is the sole SSRF guard; this must not be removed as defense-in-depth while the Spider feature is disabled.

### 5.2 MCP/HTTP auth startup warnings

**File:** `src/mcp/auth.rs`

Auth policy is selected at startup and enforced via the `AuthPolicy` enum. The invariant table:

| `AXON_MCP_AUTH_MODE` | `AXON_MCP_HTTP_TOKEN` | bind | policy |
|---|---|---|---|
| `oauth` | any | any | `Mounted { auth_state: Some(_) }` |
| `bearer` (default) | set | any | `Mounted { auth_state: None }` |
| `bearer` (default) | unset | loopback | `LoopbackDev` |
| `bearer` (default) | unset | non-loopback | **rejected at startup** |

The last row — rejecting a non-loopback bind with no token — is a hard startup invariant and must survive any extraction of `axon-mcp`. If `axon-mcp` is extracted as a crate, it must receive the resolved `AuthPolicy` from the binary entrypoint; it must not soften the non-loopback/no-token rejection.

**File:** `src/web/auth.rs`

Panel password is initialized with `subtle::ConstantTimeEq` comparison (`PanelPassword::verify`). Constant-time comparison must not be replaced with `==` during any refactor.

### 5.3 Unauthenticated envelope handling

The legacy `POST /v1/actions` route is tombstoned (returns 404). This must not be accidentally resurrected by a routing refactor. The route is documented in `docs/reference/api-parity.md` as `POST /v1/actions — removed legacy action envelope — Always returns 404 with direct REST migration text`.

### 5.4 Authz scope constants

**File:** `src/authz.rs`

`AXON_READ_SCOPE = "axon:read"` and `AXON_WRITE_SCOPE = "axon:write"` are the OAuth scope strings embedded in tokens. Changing these strings would invalidate all existing issued tokens. When `axon-authz` is extracted, these constants must remain at exactly these string values.

### 5.5 Collection name injection guard

**File:** `src/vector/ops/qdrant/utils.rs` (collection name validator)

The Qdrant collection name is validated against `[A-Za-z0-9_.-]{1,255}` with no leading/trailing dot and no `..`. This is a path-injection guard because Qdrant URLs interpolate the collection name without percent-encoding. The validator must be preserved in `axon-vector` and must not be bypassed by new code paths in `axon-services` or `axon-cli`.

---

## 6. Active-work conflict map

Branches and in-progress epics that share seams with the extraction.

### 6.1 `codex/crawl-memory-boundaries` (current branch)

Recent commits:
```
e0b239ad feat(android): add System tab to Settings with doctor health check
7823caf8 test(memory): add missing ID-scheme and pool-seam unit tests
9b9cc917 fix(android): remove dead NotYetWiredPage stub composable
```

**Seams touched:**
- `src/services/memory/` — `memory` service module (cycle-break bead for `services ↔ vector` will also touch memory types)
- `apps/android/` — Android client (memory and doctor endpoints)
- `src/vector/` — memory pool seam tests

**Conflict risk:** HIGH. Any extraction bead that moves `src/services/memory/` or `src/services/types/service/` will conflict with work landing on this branch. Coordinate: this branch must merge to `main` or be rebased before the `axon-services` extraction bead begins.

### 6.2 `codex/lumen-style-code-search` (worktree agent-aac3cfb1ad96b7d56)

Branch: `codex/lumen-style-code-search` (merged to `main` as PR #245 based on recent commit log — `d44cbf51 feat: add Lumen-style local code search`).

**Seams touched:** `src/code_index/` (new module), `src/cli/commands/code_search.rs`, `src/vector/ops/commands/code_search.rs`, `src/services/`.

**Conflict risk:** LOW (already merged). The `code_index` module is new and relatively self-contained. The extraction bead for `axon-code-index` should be clean. However, the `code_index` ↔ `services` and `code_index` ↔ `vector` import seams were just introduced and must be accounted for in the cycle-break bead.

### 6.3 `marketplace-no-mcp` (long-lived branch)

**Seams touched:** `src/mcp/`, `plugins/axon/.claude-plugin/plugin.json`, build.rs MCP feature gating.

**Conflict risk:** MEDIUM. The `axon-mcp` extraction bead changes the import path for MCP types. The `marketplace-no-mcp` branch will need rebasing or a separate compatibility patch after the MCP crate is extracted. Do NOT merge `marketplace-no-mcp` into `main` before the MCP extraction is complete.

### 6.4 In-progress plans that overlap extraction seams

| Plan | File | Overlapping seams |
|---|---|---|
| `2026-03-11-modular-workspace-and-capability-gating.md` | `docs/plans/` | This plan is the precursor to the extraction epic. Its proposed crate names (`axon-core`, `axon-crawl`, `axon-rag`, `axon-jobs`, `axon-services`, `axon-mcp`, `axon-web-server`, `axon-cli`) align with the target crate list in §1. The `crates/` layout it describes is now `src/` (post-flattening). |
| `2026-05-21-services-layer-extraction.md` | `docs/plans/` | Active (not all beads closed). Beads dvo.2–dvo.6 touch `src/services/`, `src/mcp/server/artifacts/`, `src/cli/commands/crawl/audit/`. These must land on `main` before the `axon-services` extraction bead begins, or the extraction bead will carry partially-moved code. |
| `axon_rust-30y` (adaptive full-doc skip gate) | Referenced in `src/vector/CLAUDE.md` | Touches `src/vector/ops/commands/ask/context.rs` — one of the largest vector files. Sequence this before the `axon-vector` extraction bead. |
| `axon_rust-d71.*` (hybrid search tuning) | Referenced in `src/vector/CLAUDE.md` | Multiple sub-beads touching `src/vector/ops/commands/ask/`, `src/vector/ops/qdrant/hybrid.rs`. Same risk as above. |

---

## 7. Stale docs list

These documents contain references to the retired Postgres/Redis/AMQP runtime, the old `crates/` directory layout, or the removed `POST /v1/actions` legacy envelope. They should be updated or archived as part of the extraction epic, not left to mislead future agents.

| Document | Stale references | Why stale |
|---|---|---|
| `docs/sessions/2026-02-19-module-split-and-amqp-push.md` | `crates/cli/`, `crates/core/`, AMQP consumer code | Session log from before the `crates/` → `src/` flattening and the Postgres/AMQP removal. Accurate as history but misleading if used as a path reference. |
| `docs/sessions/2026-03-07-reboot-ui-cleanup-workers-filetree.md` | `crates/cli/commands/refresh.rs`, `crates/cli/commands/ingest_common.rs` | Pre-flattening `crates/` paths. |
| `docs/sessions/2026-03-10-screenshot-warnings-cli-color-vibrance.md` | `crates/core/ui.rs`, `crates/mcp/server/handlers_system.rs`, `crates/services/screenshot.rs`, `crates/cli/commands/screenshot.rs` | All `crates/` paths. Current equivalents are `src/core/ui.rs`, `src/mcp/server/handlers_system.rs`, `src/services/screenshot.rs`, `src/cli/commands/screenshot.rs`. |
| `docs/sessions/2026-03-16-ingest-worker-dns-fix-job-recovery.md` | `crates/ingest/github/files.rs`, `docker exec axon-postgres psql …` | Pre-flattening paths + direct Postgres query. Postgres is removed; jobs are SQLite-only. |
| `docs/sessions/2026-03-11-v0-19-0-acp-persistence-release.md` | `crates/services/acp/persistent_conn.rs` | ACP (`acp`) module no longer exists in `src/`; appears to have been removed or superseded. |
| `docs/plans/2026-03-11-modular-workspace-and-capability-gating.md` | `crates/` workspace layout, `AXON_PG_URL`, `AXON_REDIS_URL`, `AXON_AMQP_URL`, `axon-rag` crate name | Written before Postgres/Redis/AMQP removal and before `src/` flattening. The crate names and layout are still useful as intent, but all path and env-var references need updating. The `axon-rag` name should become `axon-vector` to match the current module name. |
| `docs/archive/server-mode-capability-tiers.md` | References `node-pty` server, old capability tier model | Archived but still references `[workspace]` as a future state, which is now partially true (xtask is already a workspace member). |
| `docs/archive/server-mode-routing-contract.md` | (verify — likely contains pre-SQLite job routing assumptions) | Archive-tier; check before referencing in any extraction design. |

Additionally, the root `Cargo.toml` comment block explains the `vendor/lab-auth` patch in terms of `rusqlite 0.32` vs `rusqlite 0.39` and `libsqlite3-sys` conflict. This is still accurate and must be preserved when the workspace is restructured. Any bead that adds new members to `[workspace]` must verify the patch remains effective.

---

## 8. Dependency cycle notes

Summary of all confirmed cycles and the bead responsible for breaking each one.

| Cycle | Modules involved | Break mechanism | Bead |
|---|---|---|---|
| **Cycle 1** | `vector` ↔ `services` | Extract shared types (`AskResult`, `QueryHit`, `EvaluateResult`, `QueryHit`) into `axon-types` (or `axon-contracts`) crate. Both `axon-vector` and `axon-services` depend on `axon-types`. | Types-extraction bead (prerequisite to both `axon-vector` and `axon-services` beads) |
| **Cycle 2** | `vector` ↔ `ingest` | Move `is_indexable_doc_path` and `is_indexable_source_path` from `src/ingest/github/` into `axon-core` (or a new `axon-ingest-types` crate). `axon-vector` depends on `axon-core`; `axon-ingest` also depends on `axon-core`. | `axon-core` extraction bead or a preparatory PR before either extraction begins |
| **Cycle 3** | `services` ↔ `mcp` | Move MCP request-mapping types (currently in `src/mcp/schema/requests.rs` and consumed by `src/services/ingest/request.rs`) into `axon-services` or a `axon-mcp-types` micro-crate that `axon-services` can depend on without depending on all of `axon-mcp`. | MCP types extraction bead (prerequisite to the `axon-mcp` extraction bead) |
| **Cycle 4** | `crawl/engine` ↔ `services` | Pass a `persist_fn: impl Fn(path, bytes) -> Result` callback from `services::crawl` into `crawl::engine::thin_refetch::run_thin_refetch`. The crawl engine itself never imports `services`. Documented in `docs/plans/2026-05-21-services-layer-extraction.md` as the dvo.7 resolution pattern. | `axon-crawl` extraction bead (must land the callback refactor before splitting the crate) |

### 8.1 Safe extraction order (topological)

Given the cycles above, the safe bead sequence is:

```
1. axon-types (shared result/contract types)            ← breaks Cycle 1
2. axon-authz                                           ← no deps, trivial
3. axon-core                                            ← absorbs is_indexable_* (breaks Cycle 2)
4. axon-crawl                                           ← depends on axon-core; callback refactor breaks Cycle 4
5. axon-vector                                          ← depends on axon-core, axon-types
6. axon-extract                                         ← depends on axon-core, axon-crawl, axon-ingest
7. axon-mcp-types (schema/request types only)           ← breaks Cycle 3
8. axon-jobs                                            ← depends on axon-core, axon-crawl, axon-vector
9. axon-ingest                                          ← depends on axon-core, axon-jobs, axon-vector
10. axon-code-index                                     ← depends on axon-core, axon-vector
11. axon-services                                       ← depends on axon-core, axon-crawl, axon-extract,
                                                           axon-ingest, axon-jobs, axon-mcp-types, axon-vector,
                                                           axon-code-index, axon-types
12. axon-mcp                                            ← depends on axon-authz, axon-core, axon-extract,
                                                           axon-jobs, axon-services, axon-vector, axon-web
13. axon-web                                            ← depends on axon-authz, axon-core, axon-jobs,
                                                           axon-mcp, axon-services
14. axon-cli                                            ← depends on nearly everything
15. axon (binary)                                       ← links everything
```

Steps 4–10 can be parallelized once steps 1–3 are merged.

---

## 9. Cargo metadata verification

Command run during document creation:

```bash
AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo metadata --format-version 1 --no-deps 2>&1 | head -3
```

Output (truncated):

```json
{"packages":[{"name":"xtask","version":"0.1.0","id":"path+file:///home/jmagar/workspace/axon/.claude/worktrees/agent-a37453cacd41d9c0d/xtask#0.1.0",...
```

The command succeeded. The workspace currently has two members: `axon` (root package, version `5.16.5`) and `xtask` (version `0.1.0`). No `crates/` sub-packages exist yet. All future extraction beads will add members to the `[workspace] members` list in the root `Cargo.toml`.
