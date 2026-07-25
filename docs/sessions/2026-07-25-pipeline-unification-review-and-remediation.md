---
date: 2026-07-25 01:23:40 EST
repo: git@github.com:jmagar/axon.git
branch: claude/pipeline-unification-review-052d57
head: 5838b1c31
working directory: /home/jmagar/workspace/axon/.claude/worktrees/pipeline-unification-review-052d57
worktree: /home/jmagar/workspace/axon/.claude/worktrees/pipeline-unification-review-052d57
pr: none
beads: axon_rust-enbmu (epic) + 56 children; 28 closed, 28 open
---

# Pipeline unification (#298) review and remediation

## User Request

> "Conduct comprehensive-review:full-review scoped to determining if our pipeline unification is fully complete"

then, after the review landed:

> "ok begin with all of your suggestions - use agents to speed the process along - sonnet 5 medium effort agents"

## Session Overview

Two phases. **Phase 1** ran a 6-agent comprehensive review and concluded the pipeline
unification is **NOT complete** — 5 Critical, 14 High, 24 Medium, 12 Low findings — and,
decisively, found that *the repo already knew*: `docs/pipeline-unification/plans/finish-unification-metaplan.md`
(dated 2026-07-16, **one day after** the closeout audit declared #298 complete) carries 26
unchecked boxes and an explicit instruction to keep the issue open.

**Phase 2** remediated roughly half of it with 13 Sonnet subagents across 6 waves.
**28 of 56 beads are closed; 28 remain open.** The workspace ends green — 0 test failures
across the full `--no-fail-fast` run, all 11 xtask gates passing — but the largest single
finding (C1, the web pipeline) is only **partially** collapsed, and the entire
provider-scheduling finding (P3 / Non-Negotiable #7) was not started.

**This session did not finish the work.** See *Next Steps*.

## Sequence of Events

1. **Scope + baseline.** Wrote `.full-review/00-scope.md` with a severity rubric defined as
   *distance from "unification complete"*, not general risk. Captured pre-flight greps as
   explicitly-unverified leads.
2. **Phase 1 (quality + architecture).** Two agents converged independently on the same two
   Criticals. Resolved a direct conflict between them by reading the code myself.
3. **Phase 2 (security + performance).** Security **refuted four** Phase-1 findings; performance
   contributed **three new Criticals** and settled Non-Negotiable #7 as violated.
4. **Checkpoint.** Presented the verdict; user chose "Phase 3 only, then report" and "file beads
   for ALL of the issues surfaced".
5. **Phase 3 (testing + docs).** Surfaced the metaplan — the decisive artifact — and explained
   structurally why green CI missed everything.
6. **Report + beads.** Wrote `.full-review/05-final-report.md`; filed an epic + 56 children.
   Deduplicated 11 beads created twice by a shell-quoting bug (a backtick in a
   double-quoted `--description` triggered command substitution).
7. **Remediation waves 1-6.** 13 agents, partitioned to be crate-disjoint per wave because the
   tree is build-coupled even when file-disjoint.
8. **Verification + triage.** Full workspace test run showed 8 failures; isolated 7 as artifacts
   of my own test-env override and 1 as genuinely ours; fixed it.

## Key Findings

- **C1 (Critical, parent).** Three parallel implementations of acquire→prepare→embed→publish
  lived in `axon-services` **above** the adapter boundary (~1,270 lines triplicated), diverging
  on **seven** measurable axes. `crates/axon-services/src/source/non_web.rs:30`,
  `web_source.rs:113`, `local_source/local_source_job.rs:15`.
- **C2 (Critical).** `dispatch_local` was the only family dispatcher omitting
  `SourceExecutionContext` (`source/dispatch_kind.rs:139-151`), so every worker-driven local
  index created a **second, orphaned, unrecoverable** job — its request payload lacked the
  `source_request` key that `runtime/job_runners/source_runner.rs:116-118` hard-requires.
- **P1/P2 (Critical).** The claim loop awaited the general permit **inside** the loop while
  holding a claimed job (`workers/unified.rs:121-151`), stalling *all* job kinds behind source
  work — and its doc comment stated the opposite, **twice**. Parked jobs then aged past the
  360s watchdog with no heartbeat and were requeued **while still alive**.
- **P3 (Critical, NOT fixed).** Only 2 of 9 provider capacity classes are gated; **no transport
  can set `JobPriority`**, so the interactive lane, the starvation watchdog, and the claim-order
  branches are all dead code.
- **The guardrails could not fail.** `check-layering` scanned 5 deleted crates with all 9
  allowlist entries pointing at deleted files; `check-crate-contracts` audited 22 of 23;
  `xtask docs generate --check` compares a header **to itself**; the CI E2E filter `worker_e2e`
  matched **zero tests** and exited 0.
- **Six review findings were wrong or over-severe** and were corrected during cross-checking —
  notably a Critical claim that `scrape` was a live ledger-free write path (it is dead code;
  Non-Negotiable #5 is *not* violated).

## Technical Decisions

- **Severity = distance from "unification complete."** Gave every agent one rubric so a
  security Medium and an architecture Medium meant the same thing.
- **Verified contested findings myself rather than averaging agents.** The `scrape` conflict
  changed a Critical into a Medium; averaging would have produced a false Critical.
- **Preserved token compatibility on S-1.** Rather than flip `scope_satisfies` (which would
  break every deployed token), added `has_explicit_scope` for elevation checks only, kept the
  widening, and amended `auth-contract.md` — the contract was the stale artifact, not the code.
- **Accepted a partial C1 collapse.** Told the agent explicitly to stop after local if web
  proved intractable. A correct 11-of-12 collapse beats a broken 12-of-12.
- **Made the generator emit content instead of hand-writing docs.** A hand-populated
  `commands.md` was reverted by `xtask schemas generate` within minutes, proving the file is
  generator-owned; the fix went into `xtask/src/schemas/families/markdown.rs`.
- **Ran agents in crate-disjoint waves.** The tree is build-coupled even when file-disjoint; a
  downstream crate cannot compile-verify while an upstream crate is mid-edit.

## Files Changed

125 files: **107 modified, 21 deleted, 6 created** (+3,321 / −5,169).

| status | path | purpose | evidence |
|---|---|---|---|
| modified | `xtask/src/checks/layering.rs` | Rewrote FORBIDDEN/ALLOWLIST against the live 23-crate graph; gate can now fail | agent proved failure with a scratch import, then removed it |
| modified | `xtask/src/schemas/families/markdown.rs` | cli family now renders 110 commands + a Removed Commands section | `schemas generate --check` OK |
| modified | `xtask/src/checks/crate_contracts_spec.rs` | Registered `axon-extract` | `check-crate-contracts: 23 crate(s)` |
| created | `docs/pipeline-unification/crates/axon-extract/` | The missing crate contract (+ AGENTS/GEMINI symlinks) | gate 22→23 |
| modified | `crates/axon-jobs/src/workers/unified.rs` | Acquire-before-claim; deleted two false doc comments | 2 new regression tests |
| created | `crates/axon-jobs/src/workers/unified/claim.rs` | Extracted claim SQL (monolith policy: 529→412 lines) | ≤500 lines |
| created | `crates/axon-services/src/source/dispatch/local.rs` | Local now routes through the shared runner | C2 fixed |
| created | `crates/axon-services/src/source/dispatch/local_collapse_tests.rs` | Differential test: legacy vs unified local path | both tests pass |
| created | `crates/axon-services/src/source/non_web/created_generation.rs` | Streaming acquire in 64-item batches (fixes git OOM risk) | `ACQUIRE_BATCH_SIZE` |
| modified | `crates/axon-adapters/src/adapter.rs` | Added `materialize()` + `reuse_policy()` trait defaults | 8 adapters moved onto the trait |
| modified | `crates/axon-mcp/src/server.rs`, `server/tasks.rs` | Replaced hardcoded `Visibility::Internal` with `VisibilityPolicy::ceiling_for` | S-3 fixed + regression tests |
| modified | `crates/axon-authz/src/lib.rs` | Added `has_explicit_scope` for elevation checks | S-1 fixed |
| deleted | `crates/axon-web/src/server/handlers/rest*` (12 files) | Removed the 2,292-line unmounted shadow router | tests repointed at the live router |
| deleted | `crates/axon-services/src/code_search_watch*` (5 files) | Removed ~1,120 lines implementing a removed surface | zero callers verified |
| deleted | `crates/axon-services/src/contract_write.rs` | Removed the dead ledger-free write path | zero callers verified |
| deleted | `migrations/` (3 files) | Dead Postgres DDL creating per-family job tables | removed from CLI shipping paths first |
| modified | `config.example.toml`, `docs/guides/configuration.md` | Deleted 8 dead knobs; documented 8 live ones incl. 7 data-deleting retention keys | `configuration.md` had 12 sections that hard-fail parsing |
| modified | `CHANGELOG.md` | 7.0.0 now documents every removed command/action/route | sourced from `removed_registry.rs` |
| modified | `docs/pipeline-unification/delivery/issue-298-closeout-audit-2026-07-15.md` | Added Scope-of-Audit + Superseded-By; corrected gate footnotes | links the metaplan |
| modified | `docs/reference/env-matrix.toml`, `scripts/check-env-config-boundary.py` | Registered 13 env keys (9 ours, 4 pre-existing gaps) | fixed the one genuine test failure |

## Beads Activity

**Epic `axon_rust-enbmu`** — "#298 pipeline unification is NOT complete" — with **56 children**
covering every finding (5 Critical, 14 High, 24 Medium, 12 Low, with Lows grouped so nothing
was lost). **28 closed, 28 open.**

Closed this session (28): F5, F7, F8, T4, T2, gates, A6, AUDIT, CHANGELOG, A4, A1, A2, P1, P2,
C2, S-3, S-1, M2, M3, M1, F10, M6, M9/T9, M10, P7, P4, P8, P9.

`axon_rust-drahp` (C1) was **updated, not closed** — its notes record exactly what collapsed
(local) and what did not (web), so the next session does not have to re-derive it.

11 duplicate beads created by a shell-quoting bug were deleted after verifying which copy had
the intact description.

## Repository Maintenance

- **Plans.** 14 files under `docs/plans/`; none relate to #298, so **nothing was moved**. The
  live #298 plan is `docs/pipeline-unification/plans/finish-unification-metaplan.md`, which
  still shows **26 unchecked boxes** — accurate, since the work is genuinely unfinished. Left
  as-is deliberately.
- **Beads.** Full pass above; `bd dolt push` succeeded.
- **Worktrees/branches.** 4 worktrees registered; **none removed**. This branch has 0 commits
  ahead of `origin/main` (all work is uncommitted), so nothing was safe to prune.
  `marketplace-no-mcp` is protected per `CLAUDE.md` and was not touched.
- **Stale docs.** Updated `crate-ownership.md`, `configuration.md`, `redaction-contract.md`,
  the closeout audit, `crate-structure.md`, `auth-contract.md`, `documentation-contract.md`,
  `testing.md`, and the contract-packet README banner. **Not done:** the 88 contract files
  frozen at `2026-06-30` (one README banner was judged the right fix over mass-editing 88 files).
- **Transparency.** All work is **uncommitted** — 125 dirty files. Nothing was committed or
  pushed except this session log.

## Tools and Skills Used

- **Skills.** `comprehensive-review:full-review` (orchestrated the 6-agent review),
  `vibin:save-to-md` (this document).
- **Subagents.** 19 total — 6 review (opus-default) + 13 remediation (Sonnet). Two Sonnet agents
  **derailed on their final message** (one emitted "I'll stop polling and wait for the Monitor
  task"), but their edits had already landed; checked `git status` rather than assuming loss,
  per prior session experience.
- **Shell/file tools.** `cargo check/test/clippy/fmt`, `./target/debug/xtask` (11 gates), `rg`,
  `git status/diff/log` (read-only), `bd`.
- **Issues encountered.** (1) A backtick inside a double-quoted `bd --description` triggered
  command substitution, silently mangling one bead and later causing 11 duplicates — switched to
  quoted heredocs. (2) My `AXON_CONFIG_PATH=/nonexistent` guidance was wrong; the loader requires
  a `.toml` extension, and it produced 7 phantom test failures. (3) `xtask schemas generate` and
  `xtask docs generate` write **different headers to the same file**, so running one makes the
  other report drift.

## Commands Executed

| command | result |
|---|---|
| `cargo check --workspace --all-targets` | clean (baseline and final) |
| `cargo test --workspace --no-fail-fast` | **0 failures** (final) |
| `./target/debug/xtask check-layering` | OK — and provably able to fail |
| `./target/debug/xtask check-crate-contracts` | OK — **23** crates (was 22) |
| `./target/debug/xtask docs check` | all checks passed (was FAILING) |
| `./target/debug/xtask check-public-api` | 23 crates, 3,144 items, in sync (was FAILING) |
| `./target/debug/xtask schemas generate --check` | OK |
| `./target/debug/xtask check-release-versions --mode pr` | all components `changed=false` |

## Errors Encountered

- **`env_config_boundary_matrix_is_current` failing.** Root cause: partly ours (8 config keys
  renamed/documented), partly **pre-existing** (`AXON_RUSTC_WRAPPER*` was never in the matrix, so
  this test was already red on `main`). Fixed both by registering 13 keys in
  `docs/reference/env-matrix.toml`, `MIGRATION_ENV_KEY_SPECS`, and the validator's
  `VALID_TOML_DESTINATIONS` allowlist.
- **7 phantom test failures** from my own `AXON_CONFIG_PATH` override — these tests exercise
  config-home resolution, so any override breaks them. Re-ran with the env unset to isolate.
- **`release_versions_tests.rs` pinned the old shipping-paths list** including `migrations`; the
  agent that deleted the directory flagged it but was out of scope. Fixed directly.

## Behavior Changes (Before/After)

| area | before | after |
|---|---|---|
| Source job concurrency | `crawl_job_concurrency_limit`, default **1** — serialized all 12 families + `map` | `pipeline.max-active-source-jobs`, default **4** |
| Worker claim loop | one parked source job could stall `extract`/`watch`/`prune`/`graph` | permit acquired before claiming; other kinds keep flowing |
| Long-running source jobs | requeued by the watchdog **while still executing** | never marked running until about to execute |
| Local source index | created **2-4** job rows, the child unrecoverable | exactly **1** root job, retryable |
| Remote MCP caller (`axon:read`) | received `Internal` visibility — local paths, provider internals | receives `Public`, matching REST |
| `/v1/search`, `/v1/research` | write elevation was a silent no-op | requires explicit `axon:write` |
| `axon stats` | "n/a" forever (read a key nothing wrote) | reports real values |
| Web source embedding | bypassed provider admission control | reserves embedding + vector capacity |
| Git-family index | whole corpus materialized (OOM risk) | streams in 64-item batches |
| Removed commands | undocumented in a major clean-break release | CHANGELOG 7.0.0 lists all with replacements |

## Verification Evidence

| command | expected | actual | status |
|---|---|---|---|
| `cargo test --workspace --no-fail-fast` | 0 failures | 0 failures | pass |
| `cargo check --workspace --all-targets` | clean | clean | pass |
| 11 `xtask` gates | all green | all green | pass |
| C2 pre-fix proof | new local test fails first | `left: 2, right: 1` | pass |
| P1 pre-fix proof | non-source job starves | `Elapsed(())` timeout | pass |
| P2 pre-fix proof | parked job flipped to running | `left: Running, right: Queued` | pass |
| layering gate can fail | scratch import detected | detected, then removed | pass |

## Risks and Rollback

- **All 125 files are uncommitted.** Rollback is `git checkout -- .` plus removing the 6
  untracked files. Nothing has been pushed.
- **Behavioral change with real blast radius:** source job concurrency 1→4 means four source
  jobs can now run at once. Combined with the claim-loop restructure this is the intended fix,
  but it is a genuine throughput/resource change that has **not been exercised against live
  TEI/Qdrant/Chrome** — only against fake providers.
- **`/v1/search` and `/v1/research` now require explicit `axon:write`.** The agent verified no
  code path mints a read-only token, so no current caller breaks — but an operator who manually
  issued a read-only token will see new 403s.
- **`axon-extract` re-homing is still an open contract question** (the metaplan calls the crate
  "transitional"); I documented it as-is rather than resolving it.

## Decisions Not Taken

- **Did not flip `scope_satisfies`.** Would have broken every deployed token; the narrow
  elevation fix achieves the security goal without that.
- **Did not force the full C1 collapse.** Web's `Map` discovery-only scope, 304 conditional
  reuse, and artifact rollback are load-bearing; a half-migrated web pipeline is worse than an
  honestly-deferred one.
- **Did not mass-edit the 88 frozen contract files.** One README banner is maintainable; 88
  edited headers would rot again.
- **Did not move `live-qdrant` onto the PR gate.** Real cost implications; flagged for a human.
- **Did not delete `code_search_refresh.rs`.** It carries finding F4 and needs a product
  decision about whether code-search is supported.

## Open Questions

- Is code-search a supported feature? `query/code_search.rs` and `code_search_refresh.rs` now
  have **no transport callers** — if it is dead, deleting it also removes F4 and the last
  consumer of the retained `local_source/` module.
- Should `axon-extract` be re-homed per the metaplan, or is the new contract the final word?
- Should the interactive job lane be built out (transports setting `JobPriority`) or removed?
  Right now it is elaborate dead code.
- `xtask schemas` and `xtask docs` both claim `docs/reference/mcp/pipeline-tool-schema.md` and
  write different headers — which generator should own it?

## Next Steps

**This session completed 28 of 56 findings. The 28 that remain are listed in beads under epic
`axon_rust-enbmu`.** The highest-value remaining work, in order:

1. **Finish C1 — collapse `web_source/` onto the shared runner** (`axon_rust-drahp`, P0). This is
   the largest single remaining item. Needs `ReusePolicy` wired for web's 304 path and `Map`
   scope modeled as a stage skip. Its notes already record what was done and what blocks it.
2. **P3 / Non-Negotiable #7 — provider throughput scheduling** (`axon_rust-nl7au`, P0).
   **Not started.** Make `JobPriority` settable from transports, route `query`/`ask`/`retrieve`
   through the reservation boundary, gate the remaining 7 capacity classes. Use
   `crates/axon-llm/src/reservation.rs` as the template — it is the one conformant boundary.
3. **S-2 job auth-snapshot escalation** (`axon_rust-9veac`, P1) — `/v1/search` still persists
   Read+Write+**Admin** for an `axon:read` caller.
4. **H4, H5, F4, F6** (P1) — site discovery bypassing `FetchProvider`; per-family `match`
   rewriting adapter output; `route: None` ingress; the two live boundary violations currently
   sitting in the layering allowlist with TODOs.
5. **The remaining test findings** (T1 full-family differential, T3 remaining pins, T6 SSRF
   testability, T7 output-parity) — these are what stop the fixed defects from regressing.
6. **The other empty doc families** — `config-toml.md`, `env.md`, `rest/openapi.md`,
   `api/errors.md`, `sources/graph.md` still render empty; only the `cli` family was fixed.

**Immediate commands for the next session:**

```bash
bd show axon_rust-enbmu          # the epic and all 28 open children
bd ready                          # unblocked work
cargo test --workspace --no-fail-fast   # confirm the tree is still green
```

**Before any of that:** this session's 125 changed files are **uncommitted**. Decide whether to
commit them as one remediation branch or split them by wave — the changes are independently
verifiable, and the beads record which files belong to which finding.
