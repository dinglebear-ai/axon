---
date: 2026-06-20 19:23:23 EDT
repo: git@github.com:jmagar/axon.git
branch: codex/crawl-memory-boundaries
head: 0093dcfb
working directory: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon
pr: #246 fix: bound crawl memory growth https://github.com/jmagar/axon/pull/246
beads: axon_rust-23dw, axon_rust-23dw.1, axon_rust-23dw.2, axon_rust-23dw.3, axon_rust-23dw.4, axon_rust-23dw.5, axon_rust-23dw.6, axon_rust-23dw.7, axon_rust-23dw.8, axon_rust-23dw.9, axon_rust-23dw.10, axon_rust-23dw.11, axon_rust-23dw.12, axon_rust-23dw.13, axon_rust-23dw.14, axon_rust-23dw.15, axon_rust-23dw.16, axon_rust-23dw.17
---

# Crate extraction workspace plan

## User Request

Explore transitioning Axon into a proper Rust workspace with reusable crates for chunking, embedding, vector-store/upsert, retrieval, API contracts, adapters, and app surfaces. Produce a complete plan first, run an engineering review, apply the review recommendations to the beads, and save the session to markdown.

## Session Overview

Created and reviewed the Beads epic for extracting Axon from one primary Rust crate into a multi-crate workspace. The plan remains implementation-ready but now carries explicit review guardrails for `/v1/actions`, job-runner boundaries, security invariants, generated-client parity, and final dependency-feature gates.

## Sequence of Events

1. Discussed target crate granularity and decided the core reusable crates should include chunking, embedding, vector-store/upsert, retrieval, content/crawl/ingest/jobs, services, API, CLI, MCP, and web adapters.
2. Created the plan-first Beads epic `axon_rust-23dw` with 17 child beads and an 11-wave dependency graph.
3. Ran `lavra-eng-review` against the plan and identified missing or under-specified acceptance criteria.
4. Added review comments to the epic, then updated the epic and all 17 child beads with review-driven notes.
5. Validated the swarm graph and saved this session artifact.

## Key Findings

- The migration is not a simple directory move. The main preparatory work is cycle-breaking around `services -> mcp`, `vector -> services`, `jobs -> services`, and small upward `core` dependencies.
- `/v1/actions` must be tracked as a first-class product surface alongside CLI, MCP, REST/web, Android, Tauri, and Chrome extension clients.
- `axon-jobs` needs an explicit runner boundary before extraction: jobs should own queue/state/payload/worker mechanics, while services or domain crates register handlers.
- Auth and URL-safety behavior are high-risk extraction seams: MCP/Web auth warnings, unauthenticated response envelopes, and SSRF/DNS-aware validation need explicit preservation.
- Generated clients under `apps/` need verification as part of API and web extraction, not as a late cleanup afterthought.

## Technical Decisions

- No separate tests crate: tests should move with the crate that owns the behavior, with cross-crate integration coverage kept at the workspace level.
- `axon-api` is an early extraction because neutral DTOs and generated-client contracts are needed to break services/MCP coupling.
- `axon-auth` and `axon-observability` remain earned crates: the beads now allow an explicit no-extract decision if the boundary would only add ceremony.
- Compatibility re-exports and temporary shims should be used during moves, then removed only after downstream crates compile.
- Final feature profiles must be proven with `cargo tree -e features` and duplicate/heavy dependency checks before being documented.

## Files Changed

| status | path | previous path | purpose | evidence |
|---|---|---|---|---|
| created | `docs/sessions/2026-06-20-crate-extraction-workspace-plan.md` | - | Save the session plan/review/update handoff | Written by this save-to-md pass |
| modified | Beads tracker data | - | Updated `axon_rust-23dw` and all 17 child beads with review recommendations | `bd update` outputs and `bd show axon_rust-23dw --json` |
| renamed | `docs/sessions/2026-06-20-07-04-migration-guards-and-ci-cleanup.md` | `docs/sessions/2026-06-20-migration-guards-and-ci-cleanup.md` | Pre-existing dirty rename, not created by this session | `git status --short` before save showed the rename |
| modified | `src/core/config/parse/build_config/config_literal.rs` | - | Pre-existing dirty source change, not created by this session | `git status --short` before save showed the modification |

## Beads Activity

| bead | title | action | final status | why it mattered |
|---|---|---|---|---|
| `axon_rust-23dw` | Extract Axon into reusable Rust workspace crates | Created earlier in the session, commented, and updated notes | open | Owns the full workspace extraction plan and now records the review recommendation summary |
| `axon_rust-23dw.1` | Baseline dependency and public API inventory | Updated notes | open | Requires `/v1/actions`, security invariants, and edit ownership matrix in the first inventory bead |
| `axon_rust-23dw.2` | Extract axon-api contracts | Updated notes | open | Promotes `/v1/actions` and generated-client verification into API acceptance |
| `axon_rust-23dw.3` | Prepare axon-core and remove upward dependencies | Updated notes | open | Locks plugin env mapping before config parse and stronger test-build gates |
| `axon_rust-23dw.4` | Split observability and auth seams | Updated notes | open | Keeps auth/observability earned and preserves auth warning/envelope behavior |
| `axon_rust-23dw.5` | Extract axon-chunking public library | Updated notes | open | Adds source-range, multibyte, repeated-content, and citation/snippet offset guardrails |
| `axon_rust-23dw.6` | Extract axon-embedding TEI client | Updated notes | open | Adds TEI 413/429/5xx retry and large-document split preservation |
| `axon_rust-23dw.7` | Extract axon-vector-store Qdrant storage | Updated notes | open | Adds stale-tail/delete generation-fence and dependency-leak checks |
| `axon_rust-23dw.8` | Extract axon-retrieval ranking and context | Updated notes | open | Adds retrieval-to-service/API DTO mapping fixtures and no-retuning warning |
| `axon_rust-23dw.9` | Extract axon-content and vertical extraction | Updated notes | open | Adds SSRF/URL-safety requirements for content and vertical paths |
| `axon_rust-23dw.10` | Extract axon-crawl Spider and screenshot runtime | Updated notes | open | Adds DNS-aware validation before Spider/Chrome sinks and `!Send` compile gates |
| `axon_rust-23dw.11` | Extract axon-jobs and invert worker service calls | Updated notes | open | Requires explicit runner-boundary design before code movement |
| `axon_rust-23dw.12` | Extract axon-ingest provider adapters | Updated notes | open | Requires shared validation across CLI, MCP, `/v1/actions`, and REST/web |
| `axon_rust-23dw.13` | Extract axon-services facade | Updated notes | open | Makes `/v1/actions` an adapter-facing services contract |
| `axon_rust-23dw.14` | Extract axon-cli adapter crate | Updated notes | open | Preserves plugin/config ordering and representative adapter smoke checks |
| `axon_rust-23dw.15` | Extract axon-mcp adapter crate | Updated notes | open | Preserves auth behavior and prevents services from depending on MCP schema |
| `axon_rust-23dw.16` | Extract axon-web adapter and client app packaging | Updated notes | open | Requires `/v1/actions` route parity and generated-client verification |
| `axon_rust-23dw.17` | Workspace manifests, feature profiles, docs, and release gates | Updated notes | open | Adds duplicate/heavy dependency and feature leakage gates |

## Repository Maintenance

### Plans

Scanned `docs/plans/` and `docs/plans/complete/`. No plan file was moved because this session created tracker planning work rather than completing an existing checked-in plan file. Several historical plans remain active-looking or ambiguous and were left untouched.

### Beads

Updated the directly relevant epic and all 17 child beads. Did not close any beads because the implementation work has not started and all child beads remain open by design.

### Worktrees and branches

Inspected worktrees and branches. No worktree or branch cleanup was performed. The active branch is `codex/crawl-memory-boundaries`; additional worktrees exist for `marketplace-no-mcp` and `codex/lumen-style-code-search`, and both have explicit branch ownership or long-lived context.

### Stale docs

No stale docs were edited. The relevant stale-doc issue is now captured in `axon_rust-23dw.1`, which requires the first implementation bead to document which older workspace plans are stale and why.

### Transparency

Existing dirty files were observed before writing this artifact: a renamed session doc and a modified Rust config literal file. They were not touched by this session and must not be included in the session-log commit.

## Tools and Skills Used

- Shell commands: used `bd`, `git`, `gh`, `date`, `test`, and repository listing commands to inspect state, update beads, and validate the graph.
- Beads CLI: created and updated the workspace extraction epic and child beads, added review comments, and validated swarmability.
- Skills: used `lavra-plan`, `lavra-eng-review`, and `vibin:save-to-md`.
- Memory: prior Axon memory informed the engineering review, especially the recurring `/v1/actions` parity risk.
- No browser automation or external web search was used.

## Commands Executed

| command | result |
|---|---|
| `bd show axon_rust-23dw --json` | Read the epic and confirmed notes/comments/child bead state |
| `bd list --parent axon_rust-23dw --json` | Read the 17 child beads |
| `bd update axon_rust-23dw.* --append-notes ...` | Added review-driven notes to all child beads |
| `bd update axon_rust-23dw --append-notes ...` | Added the epic-level recommendation summary |
| `bd comments add axon_rust-23dw ...` | Added review comments for `/v1/actions`, jobs runner boundary, and security invariants |
| `bd swarm validate axon_rust-23dw` | Passed: 17 issues, 11 waves, max parallelism 3, first ready bead `axon_rust-23dw.1` |
| `git status --short` | Showed pre-existing dirty rename and Rust source modification |
| `git worktree list --porcelain` | Confirmed active and sibling worktrees |
| `gh pr view --json number,title,url` | Confirmed active PR #246 |
| `git commit -m "docs: save session log"` | Created path-limited session artifact commit after staging only this file |
| `git push` | Pushed branch state; upstream and HEAD both resolved to `5f9dd646` after push |
| `git diff-tree --no-commit-id --name-only -r HEAD` | Verified the session commit touched only this artifact |

## Errors Encountered

- Transcript discovery was noisy because the requested save skill is Claude-session oriented while this work happened in Codex. No clean transcript path was used in the metadata.
- `test -e docs/sessions/2026-06-20-crate-extraction-workspace-plan.md` returned nonzero as expected, confirming the target artifact did not already exist.
- Initial `git commit --only` attempts failed for the new ignored session file because Git did not accept the path as a known file. I recovered by ensuring only this artifact was staged, then committed normally.
- A transient `.git/index.lock` appeared after the failed commit attempt. I checked process state; by the next status check the lock file was gone and Git operations resumed normally.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Tracker plan | Crate extraction plan existed with 17 child beads | Epic and all child beads now include engineering-review guardrails |
| `/v1/actions` | Mentioned indirectly through services/action API context | Required as a first-class surface in inventory/API/services/web/adapters |
| Jobs extraction | Required `jobs -> services` inversion | Requires an explicit runner-boundary design before moving code |
| Security invariants | Mentioned in scattered relevant beads | Explicitly required for auth, crawl, content, MCP, and web beads |
| Runtime behavior | No code behavior changed | No runtime behavior changed; tracker-only planning update |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `bd swarm validate axon_rust-23dw` | Epic remains valid and executable | Passed: 17 issues, 11 waves, max parallelism 3 | pass |
| `bd show axon_rust-23dw --json` | Epic notes and child bead notes reflect review amendments | Output showed epic notes and amended child bead notes | pass |
| `git status --short` | Only pre-existing dirty files plus this generated artifact before commit | Pre-existing dirty rename/source change observed; generated artifact added by save pass | warn |
| `git diff-tree --no-commit-id --name-only -r HEAD` | Latest session artifact commit contains only the generated markdown file | Output listed only `docs/sessions/2026-06-20-crate-extraction-workspace-plan.md` | pass |

## Risks and Rollback

- Risk: Bead notes are tracker data, not a code diff, so rollback should use `bd update` to replace or clear the appended notes if the plan direction changes.
- Risk: Existing dirty files could be accidentally committed if broad staging is used. The save workflow must stage and commit only this session artifact path.
- Rollback: remove or amend `docs/sessions/2026-06-20-crate-extraction-workspace-plan.md` with a path-limited commit, and use Beads history or explicit `bd update` calls to revise the tracker notes.

## Decisions Not Taken

- Did not begin code extraction. The first implementation bead is still `axon_rust-23dw.1`.
- Did not create a tests crate. Ownership stays with behavior crates and workspace integration tests.
- Did not move any docs/plans files to `complete/` because none was conclusively completed by this session.
- Did not clean branches or worktrees because active ownership and long-lived branch context were observed.

## References

- Beads epic: `axon_rust-23dw`
- Active PR: `https://github.com/jmagar/axon/pull/246`
- Skill: `/home/jmagar/.codex/plugins/cache/dendrite-no-mcp/vibin/local/skills/save-to-md/SKILL.md`
- Older relevant docs referenced by the plan: `docs/plans/2026-03-11-modular-workspace-and-capability-gating.md`, `docs/plans/complete/2026-05-06-modular-workspace-plan.md`, `docs/reference/api-parity.md`

## Open Questions

- The first inventory bead still needs to decide whether `axon-auth` and `axon-observability` become real crates or remain adapter-owned seams.
- The exact job-runner boundary shape is intentionally deferred to `axon_rust-23dw.11`.
- The final public API stability level for each reusable crate still needs to be documented after the inventory and first extractions.

## Next Steps

1. Start with `bd update axon_rust-23dw.1 --claim`.
2. Produce the checked-in workspace extraction inventory document.
3. Confirm `/v1/actions`, generated clients, manifest ownership, and security invariants in that inventory before moving source files.
4. Then proceed to `axon_rust-23dw.2` for `axon-api`.
