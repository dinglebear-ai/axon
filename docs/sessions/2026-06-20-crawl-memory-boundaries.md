---
date: 2026-06-20 07:06:38 EDT
repo: git@github.com:jmagar/axon.git
branch: codex/crawl-memory-boundaries
head: 7aaea319
working directory: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon
pr: none observed for branch codex/crawl-memory-boundaries
beads: axon_rust-pwpa
---

# Crawl memory boundaries

## User Request

Investigate and fix Axon crawl memory growth after an Android docs crawl shape reached very high RSS. Address unsafe uncapped crawls, count-only crawl buffering, queued HTML-owning Chrome refetch and extraction fallback tasks, permissive defaults, and add a RAM-percentage shutdown guard.

## Session Overview

Implemented bounded crawl memory safeguards and prepared the branch for quick-push. The work rejects unscoped uncapped crawls by default, lowers crawl broadcast defaults, adds a default page byte cap, adds an RSS guard, and moves semaphore acquisition before tasks take ownership of large HTML bodies.

## Sequence of Events

1. Reviewed the reported memory risks and confirmed the live failure shape was an unscoped uncapped crawl with high concurrency and no page-size cap.
2. Added tests for crawl broadcast sizing, default safety values, unscoped uncapped crawl rejection, and RSS-threshold detection.
3. Implemented the crawl guardrails in the runtime path and collector helpers.
4. Updated extraction fallback scheduling so queued tasks cannot retain full HTML before a permit is available.
5. Updated documentation and version-bearing files for a patch release.
6. Verified focused crawl, extraction, config, formatting, and version-sync checks.

## Key Findings

- `src/crawl/engine.rs:68` now rejects `max_pages=0` unless the crawl has `path_budgets`, `url_whitelist`, or `AXON_ALLOW_UNBOUNDED_BROAD_CRAWL=true`.
- `src/crawl/engine.rs:56` now respects the configured broadcast maximum directly instead of forcing the old 16,384 message floor.
- `src/crawl/engine/memory_guard.rs:30` adds the RSS guard with default 85% RAM threshold and `AXON_CRAWL_MEMORY_ABORT_PERCENT=0` disable support.
- `src/crawl/engine/collector/chrome_tasks.rs:137` and `src/core/content/engine.rs:158` acquire permits before spawning HTML-owning tasks.
- `src/core/config/types/config_impls.rs` now defaults crawl buffering to `512..2048` and page bodies to 4 MiB.

## Technical Decisions

- Treat every unscoped uncapped crawl as unsafe, not only root paths, because depth plus link fan-out can escape a narrow-looking URL.
- Keep Spider's count-based broadcast API, but reduce its default count and pair it with a page-size cap and RSS abort guard.
- Backpressure extraction fallback and Chrome refetch at the point where HTML is still on the collector stack, avoiding unbounded queued task ownership.
- Keep the RSS guard Linux-proc based because Axon production targets are Linux containers/hosts.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| modified | `CHANGELOG.md` | - | Add 5.16.5 release note | version bump |
| modified | `Cargo.toml` | - | Bump Axon package version to 5.16.5 | `cargo check` compiled `axon v5.16.5` |
| modified | `Cargo.lock` | - | Record Axon package version | updated by `cargo check` |
| modified | `README.md` | - | Bump displayed version | version sync |
| modified | `CLAUDE.md` | - | Document crawl memory safety knobs | docs update |
| modified | `apps/web/package.json` | - | Bump web package version | version sync |
| modified | `apps/web/package-lock.json` | - | Bump web lockfile version | version sync |
| modified | `apps/web/openapi/axon.json` | - | Bump OpenAPI version metadata | version sync |
| modified | `docs/reference/actions/crawl.md` | - | Document uncapped-crawl and RSS guards | docs update |
| modified | `src/core/config/types/config_impls.rs` | - | Lower defaults and cap page bodies | tests passed |
| modified | `src/core/config/types_tests.rs` | - | Update default assertions | tests passed |
| modified | `src/core/content/engine.rs` | - | Bound fallback task HTML ownership | tests passed |
| modified | `src/crawl/engine.rs` | - | Add safety validation and guard wiring | tests passed |
| modified | `src/crawl/engine/collector/chrome_tasks.rs` | - | Bound Chrome refetch task HTML ownership | tests passed |
| modified | `src/crawl/engine_tests.rs` | - | Add regression coverage | tests passed |
| created | `src/crawl/engine/memory_guard.rs` | - | RSS percentage guard | tests passed |

## Beads Activity

| id | title | action | final status | why |
|---|---|---|---|---|
| `axon_rust-pwpa` | Bound Axon crawl memory growth | created, claimed, closed | closed | Tracked the P1 memory-safety remediation and verification evidence. |

## Repository Maintenance

### Plans

No plan files were moved. The quick-push workflow constrained maintenance to session documentation and directly relevant bead updates.

### Beads

`axon_rust-pwpa` was closed after focused verification passed.

### Worktrees and branches

Observed registered worktrees: `/home/jmagar/workspace/axon`, `/home/jmagar/workspace/_no_mcp_worktrees/axon`, and `/home/jmagar/workspace/axon/.worktrees/lumen-style-code-search`. No worktree or branch cleanup was performed because quick-push scope is commit/push, not cleanup.

### Stale docs

Updated the crawl reference and root `CLAUDE.md` because the implementation changed runtime behavior and configuration defaults.

## Tools and Skills Used

- **Skills.** `vibin:quick-push`, `vibin:save-to-md`, `superpowers:systematic-debugging`, `superpowers:test-driven-development`, and `beads:beads`.
- **Shell and Git.** Used Cargo, git, bd, gh, and status/diff commands to edit, verify, branch, and prepare the push.
- **Lumen MCP.** Used semantic search during the implementation phase to inspect relevant code surfaces.
- **File tools.** Used patch edits for code and docs.

## Commands Executed

| command | result |
|---|---|
| `cargo test --locked crawl::engine::tests:: --lib` | Passed: 66 tests |
| `cargo test --locked core::content::engine::tests:: --lib` | Passed: 3 tests |
| `cargo test --locked parse_max_profile_flows_to_crawl_subscribe_buffer --lib` | Passed |
| `cargo test --locked extract_and_crawl_defaults_are_bounded_but_explicit_zero_stays_uncapped --lib` | Passed |
| `cargo fmt` | Passed |
| `git diff --check` | Passed |
| `cargo check` | Passed |
| `cargo xtask check-version-sync` | Passed: all CLI version-bearing files in sync at 5.16.5 |
| `bd close axon_rust-pwpa ...` | Closed bead |

## Errors Encountered

- Full `cargo test --locked --lib` failed on two unrelated vector chunking tests. Isolated rerun of `vector::ops::input::tests::chunk_text_short_returns_single_chunk` passed; Lumen search showed the likely cause is a parallel test mutating `AXON_MARKDOWN_CHUNK_MAX_CHARS`.
- An isolated payload-index test failed during the first full run but passed when rerun directly, indicating no relation to the crawl memory patch.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Unscoped uncapped crawls | `max_pages=0` could run broad crawls without a real memory boundary | Rejected unless scoped or explicitly overridden |
| Broadcast buffer | Default max was 16,384 pages and forced as a legacy floor | Default max is 2,048 and config max is honored |
| Page body size | No default page byte cap | Default cap is 4 MiB |
| Crawl RSS growth | No process RAM percentage guard | RSS guard aborts at configured host-RAM percentage |
| Chrome refetch | Queued tasks could own HTML before semaphore acquisition | Permit is acquired before task spawn |
| Extract fallback | Queued tasks could own HTML before fallback semaphore acquisition | Permit is acquired before task spawn |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test --locked crawl::engine::tests:: --lib` | Crawl guard tests pass | 66 passed | pass |
| `cargo test --locked core::content::engine::tests:: --lib` | Extraction tests pass | 3 passed | pass |
| `cargo test --locked parse_max_profile_flows_to_crawl_subscribe_buffer --lib` | Config parser test passes | passed | pass |
| `cargo test --locked extract_and_crawl_defaults_are_bounded_but_explicit_zero_stays_uncapped --lib` | Default/explicit-zero test passes | passed | pass |
| `cargo fmt` | Formatting clean | passed | pass |
| `git diff --check` | No whitespace errors | passed | pass |
| `cargo check` | Project compiles | passed | pass |
| `cargo xtask check-version-sync` | Versions in sync | passed at 5.16.5 | pass |
| `cargo test --locked --lib` | Full lib suite passes | unrelated vector env race observed | warn |

## Risks and Rollback

The stricter `max_pages=0` gate can reject existing automation that intentionally runs uncapped crawls without budgets. Roll back by reverting this branch, or temporarily set `AXON_ALLOW_UNBOUNDED_BROAD_CRAWL=true` for an intentional dangerous run.

## Decisions Not Taken

- Did not attempt to byte-bound Spider's internal broadcast ring because the exposed API is count-based.
- Did not fix the vector chunking env-race during this session because it is outside the crawl memory scope.

## Open Questions

- Whether production should set `AXON_CRAWL_MEMORY_ABORT_PERCENT` lower than the default 85% for daemon runs.
- Whether the vector chunking env-race should get a separate follow-up bead.

## Next Steps

- Commit and push the quick-push bundle on `codex/crawl-memory-boundaries`.
- Open a PR for review after push.
- Consider a follow-up to serialize or isolate vector chunk-size override tests.
