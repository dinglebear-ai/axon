# Task 9 Report: Legacy Crawl Removal, Migration, And Generated Surface Cleanup

## Summary

Retired the live legacy Crawl job surface while preserving migration-only handling for old `JobKind::Crawl` rows.

Normal crawl/site acquisition now remains SourceRequest-backed: the runner registry has no Crawl runner, `crawl_start_with_context` enqueues `JobKind::Source`, and old active Crawl rows are converted to terminal failures with `legacy.crawl.removed` guidance instead of being recovered/requeued.

## Changes

- `crates/axon-services/src/runtime/job_runners/crawl_runner.rs`
  - Removed the unused legacy crawl-to-disk runner.
- `crates/axon-services/src/runtime/sqlite/crawl_bridge.rs`
  - Converted recovery into migration dead-lettering for active legacy Crawl rows.
  - Marks old rows failed with `legacy.crawl.removed` and replacement guidance.
  - Keeps list/status/cancel/cleanup/clear available only as a bridge for existing rows.
- `crates/axon-api/src/source/enums.rs`
  - Kept `JobKind::Crawl` deserializable for old rows.
  - Added public-surface helpers so generated schemas can exclude migration-only `crawl`, `embed`, and `ingest`.
- `crates/axon-mcp/src/server/tool_schema.rs`
  - Prunes migration-only job kinds from the live MCP tool schema.
- `xtask/src/schemas/*`
  - Prunes migration-only job kinds from generated public schema artifacts and canonical enum checks.
- `crates/axon-web/src/server/handlers/jobs.rs`
  - Removed stale `/v1/crawl*` OpenAPI annotations from generic job helpers.
  - Updated comments to describe Crawl as migration-only bridge state.
- `crates/axon-services/src/legacy_crawl_unreachable_tests.rs`
  - Added regression coverage for no Crawl runner registration, dead-letter recovery, and Source-backed crawl start.
- `crates/axon-mcp/tests/fixtures/schema/removed_crawl.invalid.json`
  - Added an invalid removed-action fixture.
- `docs/reference/**`, `crates/axon-mcp/tests/golden/tool-schema.json`, and `xtask/tests/fixtures/schemas/**`
  - Regenerated public schema/docs snapshots after the JobKind public-surface change.

## Notes

- The repository uses in-crate test modules for this surface, so the plan's `--test legacy_crawl_unreachable` command was mapped to `cargo test -p axon-services legacy_crawl_unreachable -- --nocapture`.
- Public schemas now omit `crawl`, `embed`, and `ingest` from JobKind enum projections. Runtime still accepts those variants for migration readers and old rows.
- The monolith policy reports warning-sized functions elsewhere on the branch, but no failures.

## Verification

- `cargo check -p axon-services`
- `cargo check -p axon-mcp`
- `cargo test -p axon-services legacy_crawl_unreachable -- --nocapture`
- `cargo test -p axon-services crawl_start_with_context -- --nocapture`
- `cargo test -p axon-mcp tool_schema -- --nocapture`
- `cargo test -p xtask canonical_enums_match_axon_api_schemars_output -- --nocapture`
- `cargo xtask schemas generate --update-fixtures`
- `cargo xtask schemas generate --check`
- `cargo test -p axon-web legacy_indexing_routes_are_absent_and_sources_present -- --nocapture`
- `cargo fmt`
- `cargo fmt --check`
- `git diff --check`
- `cargo xtask check-layering`
- `python scripts/enforce_monoliths.py --base origin/main --head HEAD`
