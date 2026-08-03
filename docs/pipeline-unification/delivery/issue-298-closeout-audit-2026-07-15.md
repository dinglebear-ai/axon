# Issue 298 Closeout Audit
Last Modified: 2026-07-15

## Verdict

Issue #298 is implementation-complete after this closeout branch lands.

The large implementation wave is merged on `main`, and this branch resolves
the final reconciliation findings from the closeout audit:

- `axon dedupe` and `axon purge` now fail as reserved removed command tokens
  instead of falling through as bare sources.
- CLI help/schema metadata now describes `source` as all-source indexing, not
  local-path-only indexing.
- The final documentation tree exists and `cargo xtask docs check` is green.
- Active runtime/status/reset/stat surfaces no longer bridge through old
  crawl/embed/ingest job families. They use canonical durable `source` /
  `extract` job kinds.
- The terminal jobs migration drops old family job storage and the generated
  database schema artifacts no longer expose those tables.

## Scope of This Audit

A 6-agent review dated 2026-07-24 found this audit accurate for what it
measured, but narrower in scope than "implementation-complete" implies without
qualification. This section states that scope plainly.

The gates below prove three things:

- **File existence** — every path in the Final Docs Tree
  (`delivery/documentation-contract.md`) is present.
- **Link resolution** — relative markdown links across the repo resolve to a
  real file.
- **Removed-surface/removed-DTO-token absence, within `docs/reference/**`
  only** — generated reference docs do not mention removed commands, actions,
  routes, or DTO fields.

The gates below do **not** prove:

- That any file **contains** correct, complete, or non-stub content. The
  audit's own note in Final Reconciliation #1 already flags this: "The new
  final-tree docs are intentionally first-pass pages. They clear the tree and
  link contracts; deeper page expansion can continue without blocking the
  existence/link gate." That caveat applies to the whole audit, not only to
  the final-tree docs check, and is restated here so it is not read as a
  footnote.
- Anything about `docs/architecture/**` or `docs/guides/**`. Neither directory
  is a target of `check-doc-links`, `check-doc-contracts`, the docs inventory
  check, or `check-crate-contracts`. Both directories can be stale relative to
  the live runtime without failing any gate in this audit.
- The **executional orchestrator topology** — how `axon-services` actually
  drives job execution at runtime. Every gate here is declarative-surface
  (CLI/MCP/REST schema, DTO shape, JobKind enum, database epoch, doc tree
  shape). None of them exercise or assert anything about the runtime
  execution path that carries a job from `queued` to `completed`.

See [Superseded By](#superseded-by) below for what closes this gap.

## Live Evidence

Baseline checked from `main` at:

```text
46d99ac8a203508a1746c4ccb852c843218ff138
```

GitHub state at audit time:

- Issue #298: open, 27 comments.
- Open PRs: none.
- Latest checked workflow set on `main`: success before this audit branch.
- Worktree state before this audit branch: clean on `main`.

Core gates after the audit fixes:

| Gate | Result |
|---|---|
| `cargo xtask check` | pass |
| `cargo xtask check-crate-contracts` | pass, 22 crate contracts [^1] |
| `cargo xtask schemas generate --check` | pass |
| `cargo xtask presentation check` | pass |
| `cargo xtask check-api-parity` | pass |
| `cargo xtask check-openapi-drift` | pass |
| `cargo xtask check-android-api-contract` | pass |
| `cargo xtask check-release-versions --head HEAD --mode main --json` | pass |
| `cargo xtask docs check` | pass [^2] |
| `cargo test -p axon-api -p axon-jobs -p axon-services -p axon-cli -p axon-mcp -p axon-web -p xtask --no-run` | pass |
| `cargo test -p xtask database_defs -- --nocapture` | pass |
| `cargo test -p axon-jobs migrations -- --nocapture` | pass |

[^1]: At audit time this was 22 of **23** product crates — `axon-extract` had
    no `docs/pipeline-unification/crates/axon-extract/` contract to check
    against, so it was silently excluded from the 22, not passing a check
    against its own contract. A separate, concurrent task is adding that
    contract; once it lands, this gate covers 23 of 23.
[^2]: This gate is existence/link/token-only (see
    [Scope of This Audit](#scope-of-this-audit)). It does not include
    `cargo xtask docs generate --check`, which did not run at audit time — the
    later `plans/finish-unification-metaplan.md` (2026-07-16) records it as
    "BLOCKED by unrelated in-flight `axon-adapters` errors" that do not
    compile.

Targeted behavior probes after this audit fix:

```text
dedupe_rc=8
`axon dedupe` has been removed from the unified source surface. Use `axon prune plan collection:<name>` or `axon prune exec collection:<name> --confirm`.

purge_rc=8
`axon purge` has been removed from the unified source surface. Use `axon prune plan <target>` or `axon prune exec <target> --confirm`.
```

```text
axon source --help
Index a source through the unified pipeline
```

## Completed Closeout Checks

Removed crates:

| Path | Status |
|---|---|
| `crates/axon-vector` | absent |
| `crates/axon-crawl` | absent |
| `crates/axon-ingest` | absent |
| `crates/axon-code-index` | absent |
| `crates/axon-source-ledger` | absent |
| `crates/axon-extract` | present intentionally; restored vertical extractor crate |

Workspace shape:

- Cargo workspace members: 25 including root binary and `xtask`.
- Product dependency graph check: 23 crates, acyclic, snapshot in sync.

Crawl/source shape:

- `crates/axon-services/src/crawl.rs` builds `SourceRequest` jobs with site
  scope; the removed public `crawl` command/action surface does not own a
  separate crawl execution path.
- `crates/axon-adapters/src/web/site_discovery.rs` owns site/docs discovery
  through the web adapter. It still uses the relocated web engine internally to
  enumerate URLs, but the crawl-to-disk service pre-pass is no longer the public
  pipeline path.
- Web adapter acquisition, manifest diffing, preparation, embedding, and
  publishing now flow through the source pipeline.

Vertical extractor shape:

- `crates/axon-extract` is present and intentional. It is no longer the missing
  old-crate gap; it is the restored vertical extractor crate.
- The closeout risk is not "extractors missing" from the tree anymore. Future
  review should focus on extractor coverage and adapter behavior, not crate
  resurrection.

## Final Reconciliation

### 1. Final documentation tree

```text
check-doc-links (repo-wide): 511 markdown file(s), no broken relative links.
check-doc-contracts: 122 markdown file(s), no removed-surface references.
docs inventory: all 110 file(s) from the Final Docs Tree exist.
docs check: all checks passed.
```

The new final-tree docs are intentionally first-pass pages. They clear the tree
and link contracts; deeper page expansion can continue without blocking the
existence/link gate.

### 2. Durable job-family closeout

Resolved on the closeout follow-up branch:

- `axon_api::source::JobKind` exposes canonical final variants only.
- CLI/MCP/web/status/reset/stat surfaces route lifecycle reads and commands
  through the durable job model.
- The `axon-services` SQLite runtime reads `ServiceJob` rows from the unified
  store instead of bridge modules.
- The `axon-jobs` old backend/ops/query/store-inventory modules and their
  orphan tests are removed.
- Migration `0026_remove_legacy_job_families.sql` rebuilds `jobs` with the
  final kind constraint and drops old family job storage.
- Generated runtime database schema JSON/markdown is free of old family job
  tables.

## Stale Citations (Outcome-Held, Not Errors)

Two file citations elsewhere in this audit now point at a different path than
the one recorded, because later commits moved or squashed the underlying code
without changing the described outcome. Both are confirmed correct in effect;
neither is a factual error in what the audit claimed happened.

- **`crates/axon-services/src/crawl.rs` (Crawl/source shape, above)** — this
  file was deleted in `5960cf2c7`. Its live analogue is
  `crates/axon-services/src/search_crawl.rs`, which builds `SourceRequest`
  jobs with site scope the same way the cited file did; the claimed behavior
  still holds under the new filename.
- **`0026_remove_legacy_job_families.sql` (Durable job-family closeout,
  above)** — this migration file no longer exists as a standalone file. It was
  squashed into `crates/axon-jobs/src/migrations/0001_canonical_jobs.sql`,
  which is the correct outcome for a clean-break epoch (one canonical
  migration, not a trail of superseded ones) and still drops old family job
  storage as described.

## Superseded By

This audit's "implementation-complete" verdict measured the declarative
surface only (see [Scope of This Audit](#scope-of-this-audit) above). A later
document, one day newer, measured further and found the executional core
incomplete:

- [`../plans/finish-unification-metaplan.md`](../plans/finish-unification-metaplan.md)
  (Last Modified: 2026-07-16) carries 26 unchecked boxes against Phase 6-12 of
  issue #298, including "Tier 5 cutover tests pass — Open, cutover blockers
  remain" (~lines 193, 206), "no known contract gaps remain — Open" (~line
  219), "docs match generated artifacts — Open" (~line 217), and "PR checklist
  is complete — Open" (~line 218). It states directly, at ~line 54: "Full
  all-source fixture completeness cannot be treated as deferrable if this
  metaplan is used to close #298. Keep #298 open until required fixture/test
  contracts pass, or explicitly narrow the issue scope before closure." It
  also calls `axon-extract` "TRANSITIONAL" (~line 180), which rebuts this
  audit's "present intentionally" framing of that crate's restoration.
- A 6-agent review dated 2026-07-24 found five Critical findings in the
  executional core that this audit's declarative-surface gates cannot detect:
  1. Three parallel pipeline implementations coexist in `axon-services`.
  2. Local sources create a second, orphaned, unrecoverable job.
  3. The claim loop stalls all job kinds behind source work.
  4. Parked source jobs are reclaimed as stale while still alive.
  5. Non-Negotiable #7 (provider throughput must be scheduled globally) is
     violated.

Neither document links back to this audit, and this audit does not link
forward to either. Read together, the corrected verdict is:

**Issue #298 is implementation-complete for the declarative surface**
(CLI/MCP/REST surface shape, `JobKind`, database epoch, adapters, and the
security seam) **but not for the executional core or documentation
completion**, both of which are tracked separately in
`plans/finish-unification-metaplan.md` and the 2026-07-24 review. Do not read
this audit's "#298 is implementation-complete" verdict as covering runtime
execution correctness or full documentation completion.

## Closeout Sequence

1. Land this closeout branch.
2. Post the final green gate summary to issue #298.
3. Sync any stale issue-body checklist items and close the issue.
