---
date: 2026-05-17 23:42:52 EDT
repo: git@github.com:jmagar/axon.git
branch: work/axon_rust-2qva
head: 91f83dab
working directory: /home/jmagar/workspace/axon_rust/.worktrees/axon_rust-2qva
worktree: /home/jmagar/workspace/axon_rust/.worktrees/axon_rust-2qva
---

# REST API Worktree Session

## User Request

Execute bead `axon_rust-2qva` in an isolated worktree and migrate the palette/client API direction toward traditional REST endpoints such as `/v1/crawl`, `/v1/scrape`, `/v1/map`, and related first-party routes.

## Session Overview

Implemented the core REST route surface, kept `/v1/actions` as a deprecated compatibility route, and added service-layer security/performance fixes needed before exposing the new routes. Also added a machine-local zsh rule so future `axon_rust` worktrees reuse the root Cargo target directory.

## Sequence of Events

1. Created and worked in branch `work/axon_rust-2qva` under `.worktrees/axon_rust-2qva`.
2. Fixed action API scope defaults, LLM action scope promotions, destructive action auth, and `/v1/actions` deprecation headers.
3. Added DNS-aware SSRF validation with 2-second timeouts for scrape/map service entry points and a service-layer 50 URL scrape batch cap.
4. Added `HttpError`, expanded service taxonomy for operational failures, and removed the ask-only error classifier.
5. Added REST handlers for discovery, RAG, exploration, async job lifecycle/start routes, dedupe, and implemented watch operations.
6. Updated API parity docs and added `docs/API.md`.

## Key Findings

- `panel_first_run.rs` was coupled to `dispatch_action`; it now calls `services::crawl::crawl_start_with_context` and `services::query::ask` directly.
- The web layer still intentionally keeps `/v1/actions`, but every action response now includes deprecation and sunset headers.
- `POST /v1/migrate` remains intentionally absent because collection migration is long-running and needs its own async job model before HTTP exposure.

## Technical Decisions

- Used singular traditional REST paths (`/v1/crawl`, `/v1/embed`, `/v1/extract`, `/v1/ingest`) to match the corrected route direction.
- Kept async lifecycle behavior uniform through `job_lifecycle_router`.
- Applied auth grouping for new read/write route sets while preserving existing `/v1/ask` and `/v1/actions` compatibility behavior.
- Used `CARGO_TARGET_DIR=/home/jmagar/workspace/axon_rust/target` for verification to avoid per-worktree build cache churn.

## Files Modified

- `src/web/server/handlers/*.rs`: new REST handlers and shared job lifecycle router.
- `src/web/server/routing.rs`: route wiring and read/write auth grouping.
- `src/web/actions.rs`: deprecated compatibility response headers and stricter destructive auth.
- `src/web/panel_first_run.rs`: direct service calls instead of action dispatch.
- `src/services/{scrape,map}.rs`: DNS-aware SSRF validation timeouts and scrape batch cap.
- `src/services/error/taxonomy.rs` and `src/web/server/error.rs`: shared HTTP error taxonomy.
- `docs/API.md` and `docs/API-PARITY.md`: route documentation.
- `/home/jmagar/.config/zsh/.zshrc`: machine-local `CARGO_TARGET_DIR` hook for axon_rust worktrees.

## Commands Executed

- `CARGO_TARGET_DIR=/home/jmagar/workspace/axon_rust/target RUSTC_WRAPPER= cargo check --lib` passed.
- `CARGO_TARGET_DIR=/home/jmagar/workspace/axon_rust/target RUSTC_WRAPPER= cargo test --lib` passed.
- `CARGO_TARGET_DIR=/home/jmagar/workspace/axon_rust/target RUSTC_WRAPPER= cargo test --test http_api_parity_inventory` passed.
- `CARGO_TARGET_DIR=/home/jmagar/workspace/axon_rust/target RUSTC_WRAPPER= cargo test` passed.
- `CARGO_TARGET_DIR=/home/jmagar/workspace/axon_rust/target RUSTC_WRAPPER= just verify` passed.

## Errors Encountered

- Initial Cargo runs without the shared target setting paid the full worktree build cost. The fix was a shell-level target-dir hook plus explicit `CARGO_TARGET_DIR` during this run.
- Parallel Cargo verification caused file lock contention on the shared target directory. After that, verification was run serially.
- Several Axum handler futures were not `Send` because service calls returned non-`Send` boxed errors across awaits. The affected evaluate/watch handlers now isolate those paths and convert errors to strings before returning HTTP errors.

## Behavior Changes

- New first-party REST routes exist for discovery, RAG, exploration, async job families, dedupe, and implemented watch operations.
- `/v1/actions` is deprecated but still present.
- Scrape batch requests over 50 URLs fail before network fetch.
- Scrape/map service URLs now get DNS-aware SSRF validation with fail-closed timeout behavior.
- Future interactive zsh sessions under `~/workspace/axon_rust` reuse `/home/jmagar/workspace/axon_rust/target`.

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo check --lib` | compile cleanly | passed | ok |
| `cargo test --lib` | library tests pass | 1841 passed, 6 ignored | ok |
| `cargo test --test http_api_parity_inventory` | docs parity tests pass | 3 passed | ok |
| `cargo test` | full test suite passes | all unit, integration, and doctests passed | ok |
| `just verify` | fmt, clippy, check, tests pass | passed | ok |

## Risks and Rollback

- The new REST route surface is broader than the previous `/v1/actions` bridge; rollback is the feature branch commit, or selectively removing the new handler modules and routing entries.
- `/v1/evaluate` and `/v1/watch/{id}/run` use blocking task isolation around existing non-`Send` service futures; revisit if those services are made fully `Send`.

## Open Questions

- Bead `axon_rust-2qva.15` still asks for stricter single-call-site `route_layer` consolidation and an auth coverage integration test.
- Bead `axon_rust-2qva.16` remains deferred until route shapes are stable enough for OpenAPI generation.

## Next Steps

- Finish `axon_rust-2qva.15` as a follow-up hardening slice.
- Add generated OpenAPI/Swagger docs after routing consolidation stabilizes.
