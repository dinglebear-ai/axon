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
| `cargo xtask docs check` | pass |
| `cargo test -p axon-api -p axon-jobs -p axon-services -p axon-cli -p axon-mcp -p axon-web -p xtask --no-run` | pass |
| `cargo test -p xtask database_defs -- --nocapture` | pass |
| `cargo test -p axon-jobs migrations -- --nocapture` | pass |

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

## Closeout Sequence

1. Land this closeout branch.
2. Post the final green gate summary to issue #298.
3. Sync any stale issue-body checklist items and close the issue.
