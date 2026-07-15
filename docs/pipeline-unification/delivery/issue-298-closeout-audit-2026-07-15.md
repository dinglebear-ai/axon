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

Remaining blockers are documentation closeout and a final decision on legacy
job-family compatibility bridges.

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

### 1. Final documentation tree is incomplete

`cargo xtask docs check` currently fails:

```text
check-doc-links (repo-wide): 1008 broken link(s)
docs inventory: 48 file(s) from the Final Docs Tree in docs/pipeline-unification/delivery/documentation-contract.md do not exist yet
```

The missing final-doc inventory includes architecture, guide, runtime,
source-reference, and surface-reference pages such as:

- `docs/architecture/source-pipeline.md`
- `docs/guides/quickstart.md`
- `docs/guides/web-crawls.md`
- `docs/reference/cli/overview.md`
- `docs/reference/runtime/observability.md`
- `docs/reference/sources/source-graph.md`
- `docs/reference/surfaces/web.md`

This is the hard closeout blocker because the final docs contract has a real
green/red gate and it is still red.

### 2. Legacy job-family bridges need an explicit closeout decision

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

This may be acceptable as migration/reset/status compatibility for old rows,
but it needs an explicit decision before #298 is closed:

- Accept and document these as legacy-data compatibility bridges, or
- Remove the old tables/kinds/bridges and migrate all remaining status/reset
  behavior to the unified durable job model.

## Closeout Sequence

1. Land this audit branch so removed cleanup tokens and generated CLI schema
   metadata are corrected.
2. Complete the final docs inventory and broken-link cleanup until
   `cargo xtask docs check` passes.
3. Decide and document the legacy job-family bridge policy.
4. Re-run the full closeout gate set.
5. Post the final green gate summary to issue #298, sync any stale issue-body
   checklist items, and close the issue.
