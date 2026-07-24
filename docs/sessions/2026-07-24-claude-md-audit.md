---
date: 2026-07-24 01:59:18 EST
repo: git@github.com:jmagar/axon.git
branch: fix/web-source-publish-invariant-redaction-skips
head: ec77de941
session id: 3786d3bf-6e8f-4459-affb-9ca525962051
transcript: /home/jmagar/.claude/projects/-home-jmagar-workspace-axon/3786d3bf-6e8f-4459-affb-9ca525962051.jsonl
working directory: /home/jmagar/workspace/axon
worktree: /home/jmagar/workspace/axon
pr: #456 chore: post-#454 follow-ups — self-hosted CI runners, xtask audit-sync, design-docs modernization (https://github.com/dinglebear-ai/axon/pull/456)
beads: No bead activity observed
---

# CLAUDE.md audit and reconciliation

## User Request
"claude md update" → after clarification, "Audit & improve" the repo's CLAUDE.md
files. The user pushed for exhaustiveness ("look harder", "keep auditing all of
the claude.md files"), then to commit and push the results.

## Session Overview
Audited every CLAUDE.md file in the repo (~48 real files, excluding a stale
harness worktree copy) against the live code, not just against filenames. Found
and fixed staleness in 18 CLAUDE.md files plus one Justfile recipe bug, verifying
each finding against the code before editing. Committed as a single path-scoped
docs commit (`ec77de941`) and pushed to the feature branch. No shipping code
touched, so no version bump or release trigger per the repo's release rules.

## Sequence of Events
1. Invoked `claude-md-management:revise-claude-md`, then (per user choice)
   `claude-md-management:claude-md-improver`.
2. Discovery: enumerated all CLAUDE.md files; excluded `.claude/worktrees/agent-*`
   (stale harness copy). Verified root file currency (crates, ports, image tags,
   DB table counts, migration counts, release table) — all accurate.
3. First pass flagged one stale item (`docs/CLAUDE.md`); user pushed back hard on
   "only one".
4. Deeper pass stress-tested the root file's assertions and found two real gaps
   (undocumented compose files; a `Justfile` bug), plus a stale `axon-vector`
   (singular, deleted crate) reference in `axon-retrieval`.
5. Fixed the clearly-evidenced crate-guide contradictions (extract, retrieval,
   authz, cli, api), each verified against the crate's real module layout.
6. Fanned out four parallel read-only auditor subagents over the remaining ~40
   files (2 crate-guide clusters, 22 pipeline-unification doc-contracts, 3
   docs/ files).
7. Re-verified every subagent finding against the code (caught false positives),
   then applied fixes across 11 more files.
8. Final consistency check, committed path-scoped, rebased and pushed; confirmed
   on remote.

## Key Findings
- Root `CLAUDE.md` is highly current; its hard numbers all verified: 29 DB tables,
  `schema_version` 1, 7 canonical migrations, ports 53333/53334/52000/6000/9222,
  images qdrant `v1.18.2` / tei `89-1.9`.
- Root `CLAUDE.md` omitted 2 of 4 compose files, including
  `docker-compose.external-qdrant.yaml` — this homelab's actual mode (Qdrant on
  tootie). Following the docs verbatim would start a bundled Qdrant that OOMs.
- `axon-extract/src/CLAUDE.md` was the worst: its "add a vertical" steps pointed
  at a `registry.rs` and `list()` that no longer exist — dispatch moved to
  `axon-adapters::vertical_registry` (`crates/axon-adapters/src/vertical_registry.rs`).
- Vector payload versioning changed scheme: `payload_schema_version = 8` →
  `payload_contract_version = "2026-07-01"` (`crates/axon-api/src/reset.rs:21`).
- `axon-vectors/src/CLAUDE.md` claimed "most modules are markers" — inverted; only
  `query.rs`/`health.rs` are 3-line markers, the rest are implemented.
- Multiple crate guides named nonexistent types (`run_mcp_server`, `WebServer`,
  `GraphCandidateIngest`, `GraphMergePolicy`, `DataDirs`, `SafePath`, `ArtifactPath`,
  `SecretString`, `EffectiveConfig`) or mis-attributed ownership.
- `Justfile:439` `stop` recipe still pkilled removed per-family worker names.
- The 22 `docs/pipeline-unification/crates/*/CLAUDE.md` and 2 of 3 remaining
  `docs/` files were verified clean.

## Technical Decisions
- Trusted nothing on the second pass: verified every asserted number, port, type,
  and command against the code — the first pass's over-trust was the user's
  complaint.
- Used read-only auditor subagents (not editing agents) so all edits stayed
  centralized and each finding was re-verified before applying; discarded false
  positives (e.g. `axon-parse` `parse.rs` is a correct cross-crate ref; an
  `axon-core` grep was polluted by matching the doc itself).
- Treated parenthetical "target module"/"planned" names as forward-looking, not
  staleness — except where the file now actually exists under a different shape.
- Fixed the `Justfile` bug inline rather than deferring, at the user's request.

## Files Changed
All changes are in commit `ec77de941` (docs-only + one Justfile recipe).

| status | path | purpose | evidence |
|---|---|---|---|
| modified | CLAUDE.md | document external-qdrant + llama compose files | grep found 4 compose files, 2 undocumented |
| modified | docs/CLAUDE.md | fix stale `src/jobs/migrations/` path | root path gone; migrations now per-crate |
| modified | docs/reference/mcp/CLAUDE.md | drop stale "pre-#298/future" framing | tool-contract.md/tool-schema.md are present-tense |
| modified | crates/axon-extract/src/CLAUDE.md | dispatch/registry moved to axon-adapters; payload_contract_version | no `list`/`registry.rs` in crate |
| modified | crates/axon-retrieval/src/CLAUDE.md | remove deleted `axon-vector` (singular) refs | crate is `axon-vectors` (plural) |
| modified | crates/axon-vectors/src/CLAUDE.md | correct inverted marker claim + add core modules | only query/health are markers |
| modified | crates/axon-mcp/src/CLAUDE.md | `run_mcp_server` → `AxonMcpServer`/`run_stdio_server` | lib.rs re-exports |
| modified | crates/axon-web/src/CLAUDE.md | `WebServer` → `router`/`PanelRuntimeState`/`openapi_document` | lib.rs re-exports |
| modified | crates/axon-graph/src/CLAUDE.md | nonexistent `GraphCandidateIngest`/`GraphMergePolicy` | grep found neither |
| modified | crates/axon-core/src/CLAUDE.md | 4 nonexistent type parentheticals corrected | grep of .rs only |
| modified | crates/axon-memory/src/CLAUDE.md | `record.rs` role; omitted modules | record.rs owns Clock/age, not MemoryRecord |
| modified | crates/axon-adapters/src/CLAUDE.md | add memory/upload families; SourceAcquisition attribution | files + axon-api::source::stage |
| modified | crates/axon-authz/src/CLAUDE.md | 5 "planned" files are live | caller/decision/policy/visibility/affinity exist |
| modified | crates/axon-api/src/CLAUDE.md | drop `contract.rs`; add schema_registry/migration | files exist/absent |
| modified | crates/axon-cli/src/CLAUDE.md | `testing.rs` row → `_tests.rs` sidecar reality | no testing.rs |
| modified | crates/axon-ledger/src/CLAUDE.md | `cleanup_debt.rs` is not a marker; add listing/validation | 202-line file |
| modified | crates/axon-parse/src/CLAUDE.md | add omitted builtins/markdown/tool_schema/validate/vertical | files exist |
| modified | crates/axon-observe/src/CLAUDE.md | add redaction/source_metrics rows | files exist |
| modified | crates/axon-route/src/CLAUDE.md | add 6 omitted module rows | files exist |
| modified | Justfile | `stop` pkill pattern → `axon.*(mcp\|jobs worker)` | removed per-family workers |

## Beads Activity
No bead activity observed — no `bd` commands were run this session. The work was a
docs-reconciliation task; no tracker state was created or changed.

## Repository Maintenance
- **Plans**: Not touched. No plan under `docs/plans/` was completed by this
  session (the injected active plan lives in a different repo, `axon_rust`). No
  moves to `docs/plans/complete/`.
- **Beads**: Checked — no bead activity this session (evidence: no `bd` invocation).
- **Worktrees/branches**: Inspected via injected context. Left untouched:
  `backup/fix-web-source-pre-rebase` (backup ref, unclear to reclaim),
  `marketplace-no-mcp` worktree at `/home/jmagar/workspace/_no_mcp_worktrees/axon`
  (protected long-lived branch), and the feature branch itself. None were safe or
  in-scope to clean.
- **Stale docs**: This session *was* the stale-docs pass for CLAUDE.md files —
  18 fixed and committed. Non-CLAUDE.md docs were out of scope.
- **Transparency**: The pre-existing dirty `.github/workflows/*` files and untracked
  `.github/actions/` were deliberately excluded from every commit (not this
  session's work).

## Tools and Skills Used
- **Skills**: `claude-md-management:revise-claude-md` (entry), then
  `claude-md-management:claude-md-improver` (the audit workflow followed).
- **Shell (Bash)**: git inspection, `find`/`grep`/`ls` cross-checks of crate
  layouts, `python3` to parse the DB schema JSON. Note: one zsh associative-array
  snippet failed (`bad substitution`) and a `grep` scoped to a crate dir was
  polluted by matching the CLAUDE.md doc — both caught and redone correctly.
- **File tools**: Read/Edit/Write for the targeted doc edits.
- **Subagents**: 4 parallel `general-purpose` read-only auditors over the remaining
  ~40 files; all findings re-verified before applying.
- **Session MCP**: `spawn_task` then `dismiss_task` for the Justfile bug (spun off,
  then handled inline and withdrawn).

## Commands Executed
| command | result |
|---|---|
| `ls crates/` | 23 crates; `axon-crawl/ingest/vector` absent (confirms clean break) |
| `python3 … database-schema.json` | schema_version 1, 29 tables, 7 migrations — all match root doc |
| `grep -n context: docker-compose*.yaml` | 4 compose files; 2 undocumented |
| `grep -rnE 'axon-crawl\|axon-ingest\|axon-vector' <all CLAUDE.md>` | clean after fixes |
| `git commit -q -F -` (path-scoped) | `ec77de941`; pre-commit hooks green (symlinks OK, xtask-check pass) |
| `git pull --rebase && git push` | pushed; local == origin at the branch tip |

## Errors Encountered
- zsh `bad substitution` on a bash associative-array loop — re-expressed as a plain
  per-file grep loop.
- A crate-scoped `grep` matched type names inside the CLAUDE.md doc itself, falsely
  "confirming" axon-core types existed — re-run with `--include='*.rs'` and
  `CLAUDE.md` excluded, which confirmed the agent's finding (types absent).

## Behavior Changes (Before/After)
| area | before | after |
|---|---|---|
| CLAUDE.md guidance | pointed at deleted crates/files, nonexistent types, wrong compose/deploy path | reconciled to current code; external-qdrant deploy documented |
| `just stop` | pkilled removed per-family worker names, missed real workers | matches `axon jobs worker` + `axon mcp` |

## Verification Evidence
| command | expected | actual | status |
|---|---|---|---|
| DB schema JSON parse | 29 tables / v1 / 7 migrations | 29 / 1 / 7 | pass |
| compose port/image grep | doc's ports/tags match compose | exact match | pass |
| deleted-crate grep post-edit | no matches | clean | pass |
| pre-commit hooks | green | monolith N/A, symlinks OK, xtask-check pass | pass |
| `git rev-parse HEAD` vs `origin/<branch>` | equal | equal (`b28df92c7`) | pass |

## Risks and Rollback
- Low risk: docs-only plus one non-destructive `Justfile` recipe. Rollback is
  `git revert ec77de941`. Some added module-map descriptions were inferred then
  cross-checked against module doc-comments; any residual imprecision is cosmetic.

## Decisions Not Taken
- Did not exhaustively list every omitted module in every map (e.g. axon-jobs,
  axon-document) — those maps are intentionally high-level and made no false
  claims; adding rows would bloat them without fixing an error.
- Did not use a Workflow (multi-agent orchestration) — the user asked for
  thoroughness, not explicit opt-in; plain read-only subagents sufficed.

## References
- PR #456 (feature branch context): https://github.com/dinglebear-ai/axon/pull/456
- Commit `ec77de941` — docs(claude): reconcile CLAUDE.md guides with current code

## Open Questions
- The injected transcript UUID (`3786d3bf-…`) differs from the runtime session id
  (`326f91d3-…`); the metadata block uses the injected value as instructed.

## Next Steps
- No follow-up work is outstanding for this session. The CLAUDE.md constellation
  is reconciled and pushed.
- Unrelated, pre-existing: dirty `.github/workflows/*` + untracked `.github/actions/`
  belong to other in-flight CI work (PR #456 area) and are not this session's to
  land.
- This session log will be landed on `main` independently per the save-to-md
  contract.
