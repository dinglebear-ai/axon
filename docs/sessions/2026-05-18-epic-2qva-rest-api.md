---
date: 2026-05-18 15:33:26 EST
repo: git@github.com:jmagar/axon.git
branch: feat/rest-api-security-preconditions
head: b3aa7b60
working directory: /home/jmagar/workspace/axon_rust/.worktrees/rest-security-2qva1
worktree: /home/jmagar/workspace/axon_rust/.worktrees/rest-security-2qva1
pr: "#105 — feat(rest)!: epic 2qva — dedicated /v1/{resource} REST API (v4.1.0) — https://github.com/jmagar/axon/pull/105"
---

## User Request

`/work-it` the bead epic `axon_rust-2qva` ("REST API: replace /v1/actions envelope with dedicated per-resource routes") **inside the existing worktree** `.worktrees/rest-security-2qva1`, growing PR #105 to encompass the full epic alongside the security-hardening commits already on the branch.

## Session Overview

Implemented the full REST epic in five family-shaped commits + two review-fix commits, all on top of the existing security-hardening branch. Twenty-five new routes spanning read-only GET, sync POST, async job submit/status/cancel, and admin/destructive surfaces. `/v1/actions` kept and tagged with RFC 8594 `Deprecation` + `Link` headers. CHANGELOG entry, version bumped 4.0.0 → 4.1.0. Three review waves (lavra-review, silent-failure-hunter, code-reviewer, code-simplifier) run; all actionable findings addressed. 77 web:: tests pass; clippy clean.

## Sequence of Events

1. Explored worktree state — branch already had PR #105 open with security-hardening commits (`2qva.1`/`2qva.2` closed beads); CI all green.
2. Confirmed scope with the user (full epic in this worktree, not a new branch).
3. Read `services::action_api`, `web::actions`, `web::server::routing`, and the existing `/v1/ask` handler to anchor the route pattern; loaded `services/CLAUDE.md` + `jobs/CLAUDE.md` for the services-first / job runtime contracts.
4. Created bd issues for Families 1–5 (`axon_rust-{nz67,9be0,kkkh,wmt3,e7xc}`); also filed a follow-up bead for the `/v1/evaluate` Send-safety blocker.
5. Built the REST module infrastructure: `rest/state.rs`, `rest/auth.rs`, `rest/error.rs`, then Family 1 (5 GET handlers) and the wiring into `web::server::routing::router`. Verified compile + tests, committed (`2d408813`).
6. Family 2 (7 sync POST handlers). Hit a non-Send-future block at `services::query::evaluate`; punted `/v1/evaluate`, documented inline, filed follow-up bead. Committed (`3576204c`).
7. Family 3 (12 async job routes — submit + status + cancel for crawl/embed/extract/ingest). Used POST `.../cancel` instead of DELETE because axum 0.8 `MethodRouter` layers cover all methods on a path. Committed (`2ef8f22f`).
8. Family 4 (admin: migrate, dedupe, watch CRUD). Hit a second Send-future block in `jobs::watch_lite::run_watch_now`; fixed by narrowing the outcome error type to `Box<dyn Error + Send + Sync>` and materializing the error into a `String` before the await. Committed (`417b4a42`).
9. Family 5 (deprecation): added `Deprecation: true` + `Link: …; rel="successor-version"` to every `/v1/actions` response, bumped `Cargo.toml` 4.0.0 → 4.1.0, expanded `CHANGELOG.md` with all five families + known limitations. Committed (`1d47c3eb`) and pushed; PR title/body updated.
10. Spawned `lavra-review` agent on the new files. Findings: 1 Must Fix (crawl_status returns 200+null for missing jobs), 4 Should Fix (response shape, test gaps). Addressed in `247bba61` + new 4 tests.
11. Spawned 3 review agents in parallel (`code-simplifier` on async_jobs, `silent-failure-hunter` on the REST module, `code-reviewer` on the whole new surface). While they ran, also pulled current cubic comments from the PR.
12. Addressed the union of findings: scope-guard marker header so `jsonize_auth_error` no longer overwrites richer scope-guard messages; `migrate` validates `from == to`; `watch_create` validates `name`/`task_type`/`every_seconds`; classifier rewritten to detect upstream FIRST and drop fragile bad_request keywords; `HeaderValue::from_static` replaces `.parse().expect()`; deprecation headers stamped at the outer middleware so auth-layer 401/403s carry them; Link header expanded to 7 successors + `/v1/capabilities` discovery; README.md 4.0.0 → 4.1.0. Committed (`b3aa7b60`) and pushed.
13. Wrote this session log.

## Key Findings

- `services::query::evaluate` (and its `vector/ops/commands/evaluate` internals) hold non-`Send` `Box<dyn Error>` across `.await` — blocks any dedicated multi-thread axum handler. Tracked as a separate bead; callers can still reach evaluate via `/v1/actions`.
- `services::crawl::crawl_status` returns a non-`Option` `CrawlJobResult` and serializes a missing job as `Value::Null`. Other three job kinds return `Option<…>`. Fixed in the REST handler with an explicit `result.payload.is_null()` → 404 check, plus a regression test that exercises all four kinds.
- axum 0.8 `MethodRouter::layer` applies to all methods on a path. Combining GET (read-scope) and POST (write-scope) on the same path therefore can't have distinct per-method scope guards. The workaround was separate paths: cancel = `POST /v1/{kind}/{id}/cancel`, watch create = `POST /v1/watch/create`.
- lab-auth's `AuthLayer` emits JSON for 401/403, not plain text. A naive "skip if response is JSON" check in `jsonize_auth_error` therefore caused lab-auth's `kind: "auth_failed"` to leak through unchanged and broke an existing test. The marker-header approach (`x-axon-scope-guard`) correctly separates our scope-guard responses from lab-auth's.
- The `services::system::sources` wire shape on `/v1/sources` intentionally drops chunk counts to match the MCP `handle_sources` payload; documented inline.

## Technical Decisions

- **One file per route family** under `src/web/server/handlers/rest/` rather than one giant handler file — keeps each file well under the repo's 500-line monolith cap.
- **Shared `RestState`** with lazy `ServiceContext` cell — the new routes use the same in-process worker runtime as `/v1/actions`; one server has one job runtime regardless of which surface enqueues.
- **Scope guard via per-route `from_fn` middleware** (`enforce_scope(ScopeGuard)`) rather than per-handler inline checks — keeps the route table declarative and avoids handler-body boilerplate.
- **`admin_write` variant of `ScopeGuard`** for migrate/dedupe — unconditional auth even in `LoopbackDev`, mirroring the existing `/v1/actions` invariant at `web::actions::authorize_action` so destructive surfaces are uniformly protected.
- **Cancel routes via POST `/cancel` suffix** instead of `DELETE /{id}` — see Key Findings.
- **Outer `stamp_deprecation_headers` middleware** for `/v1/actions` — covers auth-layer 401/403s as well as handler 200s without duplicating header logic in two places.
- **Conservative `classify_service_error`** — upstream detection runs first; bad_request fires only on very narrow URL-shape markers. Broad strings like `"is required"` / `"must be set"` come from server-side config errors and stay 500 so monitoring doesn't silently miss outages.
- **Skipped `/v1/evaluate`** rather than refactor `vector/ops/commands/evaluate` Send-safety mid-PR. Tracked as a follow-up.

## Files Modified

### New (REST module)
- `src/web/server/handlers/rest.rs` — module root + router assembly + `guarded()` helper.
- `src/web/server/handlers/rest/state.rs` — `RestState` (cfg + lazy ServiceContext + auth_required flag).
- `src/web/server/handlers/rest/auth.rs` — `ScopeGuard` (read/write/admin_write), `enforce_scope` middleware, `jsonize_auth_error` with marker-header discriminator.
- `src/web/server/handlers/rest/error.rs` — `RestErrorBody`, `rest_error()`, `classify_service_error()` upstream-first heuristic, `map_service_error()`.
- `src/web/server/handlers/rest/read_only.rs` — Family 1 (5 GET handlers).
- `src/web/server/handlers/rest/sync_post.rs` — Family 2 (7 POST handlers).
- `src/web/server/handlers/rest/async_jobs.rs` — Family 3 (4 submit + 4 status + 4 cancel handlers) with shared `ctx_only` / `ctx_and_job_id` / `cancel_response` helpers.
- `src/web/server/handlers/rest/admin.rs` — Family 4 (migrate, dedupe, watch CRUD) with per-route validation.
- `src/web/server/handlers/rest/types.rs` — per-route request body structs (deny_unknown_fields).
- `src/web/server/handlers/rest_tests.rs` — 13 sidecar tests covering wiring, auth, scope, body validation, UUID parsing, 404 semantics.

### Modified
- `src/web/server/handlers.rs` — declare `pub(crate) mod rest`.
- `src/web/server/routing.rs` — merge `rest::router(...)` alongside `actions::router(...)`.
- `src/web/actions.rs` — Family 5 deprecation: `with_deprecation_headers` (HeaderValue::from_static), `stamp_deprecation_headers` outer middleware, expanded Link header.
- `src/web/actions/tests.rs` — `actions_response_carries_deprecation_headers` regression.
- `src/jobs/watch_lite.rs` — `run_watch_now` outcome error narrowed to `Box<dyn Error + Send + Sync>`; error materialized into `Option<String>` before the await so the future stays `Send`.
- `Cargo.toml` — 4.0.0 → 4.1.0.
- `Cargo.lock` — version bump.
- `CHANGELOG.md` — 4.1.0 entry covering all five families + Known Limitations + Send-safety note.
- `README.md` — version banner 4.0.0 → 4.1.0.

## Commands Executed

- `cargo check --bin axon` after each family — clean by end of each.
- `cargo test --lib rest::` — 13 passes after the review-fix wave.
- `cargo test --lib web` — 77 passes (rest:: + actions:: + existing).
- `cargo clippy --bin axon -- -D warnings` — clean.
- `cargo fmt` — applied throughout.
- `git push` — pushed after each family; current HEAD `b3aa7b60` on origin/feat/rest-api-security-preconditions.
- `gh pr edit 105 --title ... --body ...` — retitled PR to cover the full epic.

## Errors Encountered

- **Non-Send future in `services::query::evaluate`** — `vector/ops/commands/evaluate/streaming.rs:39` holds a `Box<dyn StdError>` across an `.await`; tried materializing into `String` before the await and even wrapping in a helper that consumed by value, but `evaluate_payload`'s deeper `tokio::join!` uses `Result<_, Box<dyn Error>>` which is also non-Send. Rolled back and dropped `/v1/evaluate` from the REST surface for now.
- **Non-Send future in `jobs::watch_lite::run_watch_now`** — similar pattern but recoverable: narrowing the local `outcome` to `Box<dyn Error + Send + Sync>` and materializing into `Option<String>` before the next await made the entire `Send` chain clean. Fix kept.
- **`jsonize_auth_error` overwriting scope-guard 403s** — lab-auth's response is JSON, so a content-type discriminator broke an existing `bearer_only_read_routes_require_auth` test. Replaced with a marker-header (`x-axon-scope-guard`) approach. Tests green.

## Behavior Changes (Before/After)

| Surface | Before | After |
|---|---|---|
| `GET /v1/sources` etc. | 404 — not mounted | 200 + JSON (`axon:read`-scoped) |
| `POST /v1/{query,retrieve,suggest,map,search,research,scrape}` | 404 | 200/JSON with typed body validation |
| `POST /v1/crawl` etc. | 404 | 202 + `JobStartOutcome` (was only reachable via `/v1/actions`) |
| `GET /v1/{kind}/{id}` | 404 | 200 + status JSON, or 404 if job missing |
| `POST /v1/{kind}/{id}/cancel` | 404 | 200 + `{ canceled }` |
| `POST /v1/migrate` / `dedupe` | 404 | 202/200 with **unconditional** auth (even LoopbackDev) |
| `POST /v1/actions` | 200/JSON | 200/JSON + `Deprecation: true` + `Link` headers |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo check --bin axon` | clean | clean | ok |
| `cargo test --lib rest::` | 13 pass | 13 pass | ok |
| `cargo test --lib web` | 0 regressions | 77 pass | ok |
| `cargo clippy --bin axon -- -D warnings` | clean | clean | ok |
| `gh pr view 105 statusCheckRollup` (pre-latest-push) | all green | all green at `247bba61` | ok |

## Risks and Rollback

- **`jobs::watch_lite::run_watch_now` signature change** — outcome error narrowed to `Send + Sync`. Affects callers that downcast the error; none found in the workspace. Rollback: revert the type narrowing; lose `/v1/watch/{id}/run` ability but everything else stays.
- **`/v1/actions` deprecation headers** — purely additive; no client behavior change unless a client looks at headers.
- **Scope-guard marker header (`x-axon-scope-guard`)** — clients shouldn't depend on it; reserved by axon. Documented inline.

## Decisions Not Taken

- **Refactor `vector/ops/commands/evaluate` to be `Send`-safe.** Out of scope for this PR; deeper than just `streaming.rs`. Filed follow-up.
- **DELETE for cancel** — would have needed a per-method scope-guard mechanism axum 0.8 doesn't provide cleanly. POST `/cancel` is the pragmatic alternative.
- **Major version bump (4.x → 5.0.0).** The breaking changes (`required_scope` catch-all, scope promotions) already shipped under 4.0.0 on this branch in earlier commits; the REST additions are additive, so 4.1.0 (minor) is correct.
- **Sharing test scaffolding between `actions/tests.rs` and `rest_tests.rs`.** cubic flagged the duplication; deferred — both files already have their own `EnvGuard` / `spawn` shapes diverged enough that an extraction would add complexity without much DX win.

## References

- bead epic `axon_rust-2qva` and children `nz67` / `9be0` / `kkkh` / `wmt3` / `e7xc`
- PR https://github.com/jmagar/axon/pull/105
- `src/services/CLAUDE.md` (services-first contract)
- `src/jobs/CLAUDE.md` (ServiceJobRuntime + LiteBackend)
- `src/web/actions.rs::authorize_action` (unconditional-auth invariant)
- `src/services/action_api.rs::required_scope` (scope map)

## Open Questions

- Does the Cargo.lock version bump auto-resolve cleanly when this PR rebases on main, or will the lockfile collide? (Local rebase succeeded; expecting CI to confirm.)
- Should the chunk-count drop on `/v1/sources` actually be reversed (i.e., include counts in the new wire shape)? Two reviewers flagged it as a regression vs `/v1/actions`. Currently documented as intentional alignment with the MCP shape — open to change.

## Next Steps

### Started but not completed

- (none — every shipped family was implemented end to end and verified)

### Follow-on tasks (not yet started)

- **`POST /v1/evaluate`** — refactor `services::query::evaluate` + `vector/ops/commands/evaluate` to use `Box<dyn Error + Send + Sync>` so a multi-thread axum handler can drive it. Tracked as a separate bead.
- **`utoipa-axum` OpenAPI annotations** + `/api-docs/openapi.json` + Swagger UI (bead `axon_rust-2qva.16`, P3). Was originally planned as Family 6 but split into a follow-up so this PR stays focused on routes + tests.
- **Reply to cubic comments** flagging items already fixed in `b3aa7b60` (most are stale — cubic re-flagged them against the same commit that fixed them because its diff is against `main`). A short reply per thread will close them out.
- **Cubic-flagged test duplication** in `rest_tests.rs` vs `actions/tests.rs` — extract a shared `test_support` helper if the duplication becomes painful with future tests.
