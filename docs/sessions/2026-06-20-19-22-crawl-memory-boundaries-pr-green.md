---
date: 2026-06-20 19:22:15 EST
repo: git@github.com:jmagar/axon.git
branch: codex/crawl-memory-boundaries
head: 72849370
plan: /home/jmagar/workspace/axon_rust/docs/plans/2026-05-27-android-phase2-stubbed-modes.md
session id: bac7472a-6191-41d1-884c-575f7940e71b
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon/bac7472a-6191-41d1-884c-575f7940e71b.jsonl
working directory: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon 72849370 [codex/crawl-memory-boundaries]
pr: "#246 fix: bound crawl memory growth https://github.com/jmagar/axon/pull/246"
beads: axon_rust-pwpa
---

# Crawl memory boundaries PR closeout

## User Request

The user asked for a comprehensive memory-leak scoped review because Axon RSS was ballooning to 13-17 GB during broad crawls, then asked to implement the fixes, quick-push everything, create a PR, dispatch PR review agents, address all review findings, and confirm CI was passing.

## Session Overview

This session bounded crawl memory growth across crawl configuration, Spider page retention, Chrome refetch scheduling, extract fallback scheduling, and runtime RSS shutdown behavior. PR #246 was created, reviewed by PR toolkit agents, revised for review findings, pushed, and verified green in GitHub Actions on remote head `0093dcfb`; at save time the local branch also contained ahead commits `8f546bbe` and `72849370`.

## Sequence of Events

1. Reviewed the reported memory risks: uncapped broad-domain Spider crawls, count-only broadcast buffering, queued HTML-owning Chrome tasks, queued extract fallback tasks, and permissive daemon defaults.
2. Implemented crawl safeguards: unsafe uncapped broad crawls now require explicit scope or opt-in, page bodies default to a 4 MiB cap, broadcast defaults were lowered, and memory guard shutdown was added.
3. Bounded queued HTML ownership: Chrome fallback and extract fallback now acquire concurrency permits before spawning work that owns page HTML.
4. Dispatched PR review toolkit agents against PR #246 and addressed surfaced issues, including NaN handling for crawl memory abort percent and session-file naming feedback in the Claude transcript.
5. Fixed CI after the first PR run failed `env_config_boundary_matrix_is_current` by registering new env knobs in the env matrix, TOML destination allowlist, and migration registry.
6. Merged the current `origin/main`, resolved release metadata conflicts, and fixed the later `version-sync` CI failure by bumping CLI release metadata from `5.16.5` to `5.16.6`.
7. Re-ran local gates and watched the GitHub rollup for remote head `0093dcfb` until `ci-gate`, `test`, `release`, `mcp-smoke`, CodeQL, Android, Windows, and Compose checks were green.
8. Observed two pre-existing local ahead commits before this save artifact: `8f546bbe fix: address PR 246 review findings` and `72849370 docs: save session log`.

## Key Findings

- `src/crawl/engine/runtime.rs` previously only set Spider's page limit when `max_pages > 0`; a `max_pages=0`, depth-10, high-concurrency Android docs crawl matched the live RSS ballooning shape.
- `src/crawl/engine.rs` had a count-bounded crawl broadcast ring that could retain many large `spider::page::Page` values when the collector lagged.
- `src/crawl/engine/collector/chrome_tasks.rs` and `src/core/content/engine.rs` both spawned tasks before acquiring semaphores, allowing pending tasks to own full page HTML while waiting.
- `src/core/config/types/config_impls.rs` and related config parsing left broad daemon crawls without a hard page-byte or memory governor.
- GitHub CI first failed because `AXON_ALLOW_UNBOUNDED_BROAD_CRAWL` and `AXON_CRAWL_MEMORY_ABORT_PERCENT` were missing from the env/config boundary matrix.
- GitHub CI later failed `version-sync` because CLI code changed while the PR still advertised `5.16.5`, and tag `v5.16.5` already existed.

## Technical Decisions

- Treat `max_pages=0` broad crawls as dangerous by default; allow them only with explicit budgets, whitelists, or `AXON_ALLOW_UNBOUNDED_BROAD_CRAWL=true`.
- Keep explicit `--max-page-bytes 0` semantics, but default parsed configs to a 4 MiB page-body cap.
- Centralize crawl broadcast defaults and lower them to keep retained `Page` values bounded under normal daemon profiles.
- Add a crawl RSS memory guard that honors cgroup memory limits before host `/proc/meminfo`, shuts Spider down by control ID, and uses RAII cancellation.
- Propagate memory-abort errors through Chrome fallback instead of silently preserving the HTTP result.
- Use the repo-native `cargo xtask bump-version cli patch` and generated OpenAPI artifact flow for the release metadata repair.

## Files Changed

| status | path | previous path | purpose | evidence |
| --- | --- | --- | --- | --- |
| modified | `CHANGELOG.md` | - | Record crawl-memory fixes and bump CLI release to `5.16.6`. | `git diff --name-status origin/main...HEAD` |
| modified | `CLAUDE.md` | - | Document crawl memory safety flags and defaults. | `git diff --name-status origin/main...HEAD` |
| modified | `Cargo.toml` | - | Bump Axon CLI package version. | commit `0093dcfb` |
| modified | `Cargo.lock` | - | Sync Axon package version. | commit `0093dcfb` |
| modified | `README.md` | - | Sync displayed version. | commit `0093dcfb` |
| modified | `apps/web/package.json` | - | Sync web package version. | commit `0093dcfb` |
| modified | `apps/web/package-lock.json` | - | Sync web lockfile version. | commit `0093dcfb` |
| modified | `apps/web/openapi/axon.json` | - | Sync OpenAPI version metadata and generated artifact. | `cargo xtask check-openapi-drift` passed |
| modified | `config.example.toml` | - | Document new crawl memory and unbounded-crawl config knobs. | commit `66ea77c5` |
| modified | `docs/reference/actions/crawl.md` | - | Document crawl memory safety behavior. | commit `b9bbd6bf` |
| modified | `docs/reference/env-matrix.toml` | - | Register new env knobs for boundary checks. | commit `5ea45c2f` |
| created | `docs/sessions/2026-06-20-crawl-memory-boundaries.md` | - | Earlier session log for the memory-boundary implementation. | commit `4f2b0117` |
| modified | `scripts/check-env-config-boundary.py` | - | Allow TOML destinations for new crawl memory env keys. | commit `5ea45c2f` |
| modified | `src/core/config/parse/build_config/config_literal.rs` | - | Resolve crawl memory abort percent and reject non-finite values. | transcript review fix |
| modified | `src/core/config/parse/build_config_tests.rs` | - | Cover config resolution changes. | local tests and CI |
| modified | `src/core/config/parse/env_registry/migration.rs` | - | Register env migration entries for new knobs. | commit `5ea45c2f` |
| modified | `src/core/config/parse/performance.rs` | - | Centralize bounded broadcast defaults by profile. | commit `66ea77c5` |
| modified | `src/core/config/parse/toml_config.rs` | - | Parse TOML crawl memory knobs. | commit `66ea77c5` |
| modified | `src/core/config/types.rs` | - | Expose new config fields. | commit `66ea77c5` |
| modified | `src/core/config/types/config.rs` | - | Carry crawl memory settings through `Config`. | commit `66ea77c5` |
| modified | `src/core/config/types/config_impls.rs` | - | Default max page bytes and broadcast settings safely. | commit `b9bbd6bf` |
| modified | `src/core/config/types/subconfigs.rs` | - | Add scrape/crawl safety subconfig fields. | commit `66ea77c5` |
| modified | `src/core/config/types_tests.rs` | - | Cover default and explicit-zero page-byte behavior. | local targeted tests |
| modified | `src/core/content/engine.rs` | - | Acquire fallback extraction permits before spawning HTML-owning work. | commit `b9bbd6bf` |
| modified | `src/crawl/engine.rs` | - | Lower broadcast retention and integrate memory guard/collector behavior. | commit `66ea77c5` |
| modified | `src/crawl/engine/collector/chrome_tasks.rs` | - | Acquire Chrome permits before spawning HTML-owning refetch work. | commit `b9bbd6bf` |
| created | `src/crawl/engine/memory_guard.rs` | - | Add RSS/cgroup memory guard and Spider shutdown behavior. | commit `66ea77c5` |
| modified | `src/crawl/engine_tests.rs` | - | Cover crawl limit, memory guard, and config behavior. | local targeted tests |
| modified | `src/services/crawl_sync.rs` | - | Apply uncapped broad crawl guard and Spider control IDs to sync paths. | commit `66ea77c5` |
| modified | `src/services/crawl_sync/chrome_fallback.rs` | - | Propagate memory-abort failures through Chrome fallback. | commit `66ea77c5` |

## Beads Activity

| id | title | action | final status | why it mattered |
| --- | --- | --- | --- | --- |
| `axon_rust-pwpa` | Bound Axon crawl memory growth | Created, claimed, implemented, and closed earlier in this session. | closed | Tracks the exact memory-safety task. `bd show axon_rust-pwpa --json` reports closure reason: scoped uncapped crawl gate, lower broadcast defaults, page byte cap, RSS abort guard, and bounded fallback/refetch tasks were implemented and verified. |

## Repository Maintenance

### Plans

`find docs/plans -maxdepth 2 -type f` showed many existing complete and non-complete plan files. None were clearly part of this crawl-memory PR closeout, so no plans were moved. The active plan reported by `.claude/current-plan` points at `/home/jmagar/workspace/axon_rust/docs/plans/2026-05-27-android-phase2-stubbed-modes.md`, which is outside the current repo root and was not changed.

### Beads

`bd list --all --sort updated --reverse --limit 100 --json`, `tail -200 .beads/interactions.jsonl`, and `bd show axon_rust-pwpa --json` were run. The directly relevant bead was already closed with implementation and verification evidence, so no bead mutation was needed during this save-to-md pass.

### Worktrees And Branches

`git worktree list --porcelain` showed three registered worktrees: the current PR branch, `marketplace-no-mcp`, and `codex/lumen-style-code-search`. `CLAUDE.md` documents `marketplace-no-mcp` as an intentional long-lived branch, and `codex/lumen-style-code-search` is a separate active worktree. No worktrees or branches were removed. `git log --oneline origin/codex/crawl-memory-boundaries..HEAD` showed two pre-existing local ahead commits, `8f546bbe` and `72849370`, before this session artifact was staged; they were left intact.

### Stale Docs

Docs touched by the session were already updated in the PR: `CLAUDE.md`, `README.md`, `docs/reference/actions/crawl.md`, `docs/reference/env-matrix.toml`, and `config.example.toml`. The current save pass did not find an additional stale-doc update that was safe and directly scoped.

### Transparency

No cleanup was hidden: plan moves were skipped because no current completed plan was identified, bead mutation was skipped because the relevant bead was already closed, and branch/worktree cleanup was skipped because the observed worktrees were active or intentionally long-lived.

## Tools and Skills Used

- **Skills and plugins.** Used `vibin:save-to-md` for this artifact; earlier work used `vibin:quick-push`, GitHub CI/pr skills, PR review toolkit agents, and Lumen semantic search guidance.
- **Shell and Git.** Used `git`, `gh`, `cargo`, `npm`, `bd`, and repo xtasks for implementation, verification, PR state, CI logs, and maintenance evidence.
- **MCP and app tools.** Used Lumen semantic search after the Lumen instruction was exposed, plus PR review toolkit subagents from the Claude transcript.
- **File editing tools.** Used patch/edit flows for Rust, config, docs, and generated session artifacts.
- **Subagents.** PR review agents found actionable review items including non-finite memory percent handling and session-file naming convention feedback.
- **External CLIs.** `gh` inspected PR checks and logs; `cargo xtask` handled release version and OpenAPI checks; `bd` provided bead state.

## Commands Executed

| command | result |
| --- | --- |
| `git status --short --branch` | Branch `codex/crawl-memory-boundaries` clean and tracking origin after PR push. |
| `gh pr view 246 --json ...` | PR #246 is mergeable on head `0093dcfb`; status rollup green. |
| `gh pr checks 246` | Final rollup passed, including `ci-gate`, `test`, `clippy`, `release`, `mcp-smoke`, Android, Windows, CodeQL gate, and Compose smoke. |
| `gh api /repos/jmagar/axon/actions/jobs/82520651707/logs` | Retrieved failing `version-sync` log showing `5.16.5` was not greater than existing tag `v5.16.5`. |
| `cargo xtask bump-version cli patch` | Bumped CLI release metadata to `5.16.6`. |
| `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr` | Passed after version bump. |
| `cargo xtask check-version-sync` | Passed at `5.16.6`. |
| `cargo xtask check-openapi-drift` | Passed after committing canonical generated OpenAPI artifact. |
| `git push` | Pre-push passed and pushed `0093dcfb` to `origin/codex/crawl-memory-boundaries`. |
| `bd show axon_rust-pwpa --json` | Confirmed relevant bead was closed with implementation and verification summary. |

## Errors Encountered

- **CI env boundary failure.** `env_config_boundary_matrix_is_current` failed because `AXON_ALLOW_UNBOUNDED_BROAD_CRAWL` and `AXON_CRAWL_MEMORY_ABORT_PERCENT` were missing from the env matrix and migration registry. Fixed by updating `docs/reference/env-matrix.toml`, `scripts/check-env-config-boundary.py`, and `src/core/config/parse/env_registry/migration.rs`.
- **CI version-sync failure.** GitHub rejected `5.16.5` because CLI code changed and tag `v5.16.5` already existed. Fixed by bumping CLI metadata to `5.16.6`, correcting changelog placement, and regenerating OpenAPI artifacts.
- **OpenAPI drift during pre-push.** The first version-bump commit used the bump command's rewritten OpenAPI JSON shape and failed `check-openapi-drift`. Running `cargo xtask check-openapi-drift`, staging its canonical output, and amending the commit fixed it.
- **Review finding: NaN memory percent.** PR review toolkit noted that `"NaN".parse::<f64>()` could silently disable the memory guard. Fixed by requiring `percent.is_finite() && percent > 0.0`.

## Behavior Changes (Before/After)

| area | before | after |
| --- | --- | --- |
| Uncapped broad crawls | `max_pages=0` could run broad docs crawls without an effective memory boundary. | Uncapped broad crawls require explicit scope or opt-in. |
| Page retention | Broadcast buffering and queued tasks could retain many large page bodies. | Broadcast defaults are lower and HTML-owning work waits for permits before spawning. |
| Page body size | No default page-byte governor in normal parsed configs. | Crawl page bodies default to a 4 MiB cap unless explicitly disabled. |
| Runtime memory | Long-running crawls had no process RSS percent abort guard. | Crawl guard aborts and shuts down Spider when RSS exceeds configured percent of cgroup/host memory. |
| CI release metadata | PR advertised already-tagged `5.16.5`. | PR advertises `5.16.6`; release/version gates pass. |

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `cargo test extract_and_crawl_defaults_are_bounded_but_explicit_zero_stays_uncapped` | Default cap present, explicit zero preserved. | Passed. | pass |
| `cargo test crawl::engine::tests::` | Focused crawl engine tests pass. | Passed. | pass |
| `cargo test --test config_home_pipeline` | Config home pipeline tests pass. | Passed. | pass |
| `cargo test --locked --features test-helpers --test env_config_boundary env_config_boundary_matrix_is_current -- --nocapture` | Env matrix includes new knobs. | Passed. | pass |
| `cargo check` | Workspace compiles. | Passed after merge. | pass |
| `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr` | Release metadata valid. | Passed at `5.16.6`. | pass |
| `cargo xtask check-version-sync` | CLI version files in sync. | Passed at `5.16.6`. | pass |
| `cargo xtask check-openapi-drift` | Generated OpenAPI artifacts in sync. | Passed. | pass |
| `git push` | Pre-push hooks pass. | Passed: version-sync, web build, clippy, OpenAPI drift. | pass |
| `gh pr checks 246` | PR checks pass. | Passed for remote head `0093dcfb` with `ci-gate`, `mcp-smoke`, `release`, `test`, CodeQL gate, Android, Windows, and Compose smoke green. | pass |

## Risks and Rollback

- The main behavior risk is stricter handling of unbounded broad crawls. Operators who intentionally relied on `max_pages=0` for broad crawls must now provide budgets/whitelists or set `AXON_ALLOW_UNBOUNDED_BROAD_CRAWL=true`.
- The memory abort guard can terminate large crawls at the configured RSS threshold. Set `AXON_CRAWL_MEMORY_ABORT_PERCENT=0` only for controlled debugging if the guard must be disabled.
- Rollback path: revert PR #246 or specifically revert commits `0093dcfb`, `5ea45c2f`, `66ea77c5`, and `b9bbd6bf`, then rerun `cargo xtask check-release-versions`, `cargo xtask check-openapi-drift`, and PR CI.

## Decisions Not Taken

- Did not remove or clean the `marketplace-no-mcp` worktree/branch because repo instructions identify it as intentional long-lived state.
- Did not delete `codex/lumen-style-code-search` because it is a separate active worktree and branch.
- Did not move plan files to `docs/plans/complete/` because none were proven completed by this crawl-memory session.
- Did not rely only on local tests after CI failed; the failing GitHub job logs were inspected and the PR was watched until the final remote `ci-gate` passed.

## References

- PR #246: https://github.com/jmagar/axon/pull/246
- GitHub Actions CI run: https://github.com/jmagar/axon/actions/runs/27886158450
- GitHub Actions CodeQL run: https://github.com/jmagar/axon/actions/runs/27886158451
- GitHub Actions Compose smoke run: https://github.com/jmagar/axon/actions/runs/27886158457
- Bead `axon_rust-pwpa`: `bd show axon_rust-pwpa --json`
- Transcript: `/home/jmagar/.claude/projects/-home-jmagar-workspace-axon/bac7472a-6191-41d1-884c-575f7940e71b.jsonl`

## Open Questions

- The active plan path from `.claude/current-plan` points into `/home/jmagar/workspace/axon_rust`, not the current `/home/jmagar/workspace/axon` repo. It was recorded as observed context but not acted on.
- No merge was performed in this save-to-md pass; PR #246 remains open. Remote PR checks were observed green on `0093dcfb`, while local ahead commits existed before the session note commit.

## Next Steps

1. Merge PR #246 when ready.
2. After merge, deploy or restart the running Axon service so the daemon uses the new crawl memory safeguards.
3. For the Android docs crawl shape that triggered the investigation, rerun with an explicit budget/whitelist or accept the intentional unbounded-crawl opt-in.
4. Monitor RSS during the next broad crawl and confirm the configured guard emits the expected abort/shutdown behavior if the threshold is reached.
