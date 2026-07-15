# Issue 298 Closeout Audit
Last Modified: 2026-07-15

## Verdict

Issue #298 is not ready to close yet.

The large implementation wave is merged on `main`, and the core unification
gates are green. This audit also fixed two closeout drifts found during review:

- `axon dedupe` and `axon purge` now fail as reserved removed command tokens
  instead of falling through as bare sources.
- CLI help/schema metadata now describes `source` as all-source indexing, not
  local-path-only indexing.

Remaining blocker: remove legacy job-family compatibility bridges. The policy
decision is now explicit: old source-family job tables and backend job kinds are
removal targets, not compatibility surfaces.

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
| `cargo xtask check-crate-contracts` | pass, 22 crate contracts |
| `cargo xtask schemas generate --check` | pass |
| `cargo xtask presentation check` | pass |
| `cargo xtask check-api-parity` | pass |
| `cargo xtask check-openapi-drift` | pass |
| `cargo xtask check-android-api-contract` | pass |
| `cargo xtask check-release-versions --head HEAD --mode main --json` | pass |
| `cargo xtask docs check` | fail |

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

## Remaining Blockers

### 1. Final documentation tree is complete

Resolved on the closeout follow-up branch:

```text
check-doc-links (repo-wide): 511 markdown file(s), no broken relative links.
check-doc-contracts: 122 markdown file(s), no removed-surface references.
docs inventory: all 110 file(s) from the Final Docs Tree exist.
docs check: all checks passed.
```

The new final-tree docs are intentionally first-pass pages. They clear the tree
and link contracts; deeper page expansion can continue without blocking the
existence/link gate.

### 2. Legacy job-family bridges must be removed

The old source-family crates are gone, and real source execution appears routed
through unified source jobs. However, compatibility/status/reset code still
references legacy family tables and job kinds:

- `axon_crawl_jobs`
- `axon_embed_jobs`
- `axon_extract_jobs`
- `axon_ingest_jobs`
- `JobKind::Crawl`
- `JobKind::Embed`
- `JobKind::Extract`
- `JobKind::Ingest`

Representative live references are in:

- `crates/axon-jobs/src/workers.rs`
- `crates/axon-jobs/src/store_inventory.rs`
- `crates/axon-jobs/src/migrations/0001_create_tables.sql`
- `crates/axon-services/src/embed_tests.rs`
- `crates/axon-services/src/extract_tests.rs`
- `crates/axon-services/src/reset_tests.rs`

These are not acceptable compatibility bridges in the final state. Remove the
old tables/kinds/bridges and migrate all remaining status/reset/stat behavior
to the unified durable job model.

## Closeout Sequence

1. Land this audit branch so removed cleanup tokens and generated CLI schema
   metadata are corrected.
2. Remove legacy family job tables/backend kinds/bridge modules from active
   runtime and generated contracts.
3. Re-run the full closeout gate set.
4. Post the final green gate summary to issue #298, sync any stale issue-body
   checklist items, and close the issue.
