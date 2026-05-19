# Session: Export Backup and Restore Hardening

## 1. Session overview
- Reviewed and tightened the export/backup implementation to support rebuild-oriented backups instead of exporting full crawled URL inventories.
- Added `export verify`, schema/integrity verification, seed-tracking migration SQL, restore documentation, and export contract documentation.
- Verified code compiles and targeted tests pass.
- Applied the new seed-tracking migration to the live Postgres database and verified tables/indexes exist.

## 2. Timeline of major activities
- Reviewed export scope and confirmed the desired contract: crawl seed URLs only for crawl-derived content; keep other seed inputs for scrape/search/research/extract/ingest.
- Reworked export behavior and schema around seed-only output with optional history inclusion.
- Added CLI/config support for `axon export verify <file>` and implemented export verification logic.
- Added golden schema coverage, a seed-only no-fanout test, and restore/export docs.
- Applied `migrations/003_export_seed_tracking.sql` to the live database and verified presence of tables/indexes.

## 3. Key findings with references
- Export now defaults to seed-only and only includes historical sections when `include_history` is enabled: [crates/services/export.rs](/home/jmagar/workspace/axon_rust/crates/services/export.rs):51, [crates/services/export.rs](/home/jmagar/workspace/axon_rust/crates/services/export.rs):96
- Export schema version is `3` and requires integrity, metadata, settings snapshot, rebuild seeds, watches, and refresh schedules: [crates/services/export.rs](/home/jmagar/workspace/axon_rust/crates/services/export.rs):17, [crates/services/export.rs](/home/jmagar/workspace/axon_rust/crates/services/export.rs):83
- `axon export verify` validates required top-level keys, schema version, and integrity hashes/counts before restore: [crates/cli/commands/export.rs](/home/jmagar/workspace/axon_rust/crates/cli/commands/export.rs):7, [crates/services/export.rs](/home/jmagar/workspace/axon_rust/crates/services/export.rs):138, [crates/services/export.rs](/home/jmagar/workspace/axon_rust/crates/services/export.rs):143
- Search/research query tracking is recorded before external search execution: [crates/services/search.rs](/home/jmagar/workspace/axon_rust/crates/services/search.rs):30, [crates/services/search.rs](/home/jmagar/workspace/axon_rust/crates/services/search.rs):80
- Scrape seed tracking is recorded after a successful scrape payload is built: [crates/services/scrape.rs](/home/jmagar/workspace/axon_rust/crates/services/scrape.rs):66, [crates/services/scrape.rs](/home/jmagar/workspace/axon_rust/crates/services/scrape.rs):72

## 4. Technical decisions and rationale
- Kept export rebuild-focused: crawl fanout URLs are excluded because rebuild only needs crawl seeds plus non-crawl seed inputs.
- Added `include_history` instead of mixing rebuild seeds with historical payloads by default to keep backups compact and deterministic.
- Added integrity counts and hashes so a backup can be validated before restore and audited after replay.
- Added migration SQL for `axon_query_history` and `axon_scrape_seeds` because relying on lazy table creation is weaker for fresh databases.
- Used a golden fixture for schema v3 to make field drift visible in CI.

## 5. Files modified/created and purpose
- [crates/services/export.rs](/home/jmagar/workspace/axon_rust/crates/services/export.rs): export manifest assembly, seed-only behavior, verification logic, integrity logic, tests.
- [crates/cli/commands/export.rs](/home/jmagar/workspace/axon_rust/crates/cli/commands/export.rs): CLI execution for export and `export verify`.
- [crates/core/config/cli.rs](/home/jmagar/workspace/axon_rust/crates/core/config/cli.rs): `export verify` subcommand definition.
- [crates/core/config/parse/build_config.rs](/home/jmagar/workspace/axon_rust/crates/core/config/parse/build_config.rs): config wiring for `include_history` and verify input path.
- [crates/core/config/types/config.rs](/home/jmagar/workspace/axon_rust/crates/core/config/types/config.rs): config field for verify input path.
- [crates/core/config/types/config_impls.rs](/home/jmagar/workspace/axon_rust/crates/core/config/types/config_impls.rs): default/debug support for new export config.
- [crates/services/types/export.rs](/home/jmagar/workspace/axon_rust/crates/services/types/export.rs): export schema and verify report types.
- [migrations/003_export_seed_tracking.sql](/home/jmagar/workspace/axon_rust/migrations/003_export_seed_tracking.sql): migration for query/scrape seed tracking tables.
- [docs/EXPORT.md](/home/jmagar/workspace/axon_rust/docs/EXPORT.md): export contract documentation.
- [docs/RESTORE.md](/home/jmagar/workspace/axon_rust/docs/RESTORE.md): restore workflow documentation.
- [docs/README.md](/home/jmagar/workspace/axon_rust/docs/README.md): docs index links for export/restore.
- [docs/commands/export.md](/home/jmagar/workspace/axon_rust/docs/commands/export.md): command documentation including verify.
- [tests/export_schema_v3_golden.rs](/home/jmagar/workspace/axon_rust/tests/export_schema_v3_golden.rs): schema drift guard.
- [tests/fixtures/export_schema_v3.golden.json](/home/jmagar/workspace/axon_rust/tests/fixtures/export_schema_v3.golden.json): golden export fixture.

## 6. Critical commands executed and outcomes
- `cargo check -q` | completed successfully after export refactor updates.
- `cargo fmt && cargo check -q` | formatting and compile verification succeeded.
- `cargo test -q export_schema_v3_matches_golden_fixture` | passed.
- `cargo test -q seed_only_export_contains_only_seed_urls_not_crawl_fanout_urls` | passed.
- `cargo test -q verify_manifest_detects_integrity_drift` | passed.
- `cargo test -q parse_export_verify_subcommand` | passed.
- `cargo run --quiet --bin axon -- --json export verify tests/fixtures/export_schema_v3.golden.json` | returned `valid: true`.
- `docker exec -i axon-postgres psql -v ON_ERROR_STOP=1 -U axon -d axon < migrations/003_export_seed_tracking.sql` | migration applied; existing relations were skipped where already present.
- `docker exec -i axon-postgres psql -U axon -d axon -Atc ...` | confirmed `axon_query_history`, `axon_scrape_seeds`, and both indexes exist.

## 7. Behavior changes (before/after)
- Before: export behavior in the working branch had drifted historically and had previously emitted oversized URL inventories; there was no explicit `export verify` command.
- After: export is seed-only by default, history is opt-in, integrity is verified, and backup validity can be checked with `axon export verify <file>`.
- Before: fresh DBs could rely on runtime lazy creation of query/scrape tracking tables.
- After: migration SQL exists and was applied to the live DB for those tables.

## 8. Verification evidence (`command | expected | actual | status`)
- `cargo check -q | compile succeeds | succeeded | PASS`
- `cargo test -q export_schema_v3_matches_golden_fixture | golden fixture matches schema v3 | passed | PASS`
- `cargo test -q seed_only_export_contains_only_seed_urls_not_crawl_fanout_urls | no crawl fanout URLs in seed-only export | passed | PASS`
- `cargo test -q verify_manifest_detects_integrity_drift | verifier catches mismatched integrity | passed | PASS`
- `cargo run --quiet --bin axon -- --json export verify tests/fixtures/export_schema_v3.golden.json | valid=true | valid=true | PASS`
- `docker exec ... < migrations/003_export_seed_tracking.sql | tables/indexes available | applied without error | PASS`
- `./scripts/axon status --json | Axon status responds | returned JSON status payload | PASS`
- `./scripts/axon embed "docs/sessions/2026-03-20-export-backup-restore-hardening.md" --json | embed job queues | job_id=fabea4ff-1345-483c-9da7-26e30a439ca7 status=pending; final re-embed job_id=056b1c9a-bb9a-4278-b7ae-db21afdcd863 status=pending | PASS`
- `./scripts/axon embed status "056b1c9a-bb9a-4278-b7ae-db21afdcd863" --json | completed status returns source metadata | status=completed, metrics.collection=cortex, metrics.source=rust, no url field present | PASS_WITH_CONTRACT_DRIFT`
- `./scripts/axon retrieve "rust" --collection "cortex" | retrieve embedded session content | "No content found for URL: rust" | FAIL`

## 9. Source IDs + collections touched (embed/retrieve source IDs, collections, outcomes)
- Saved session markdown path: `docs/sessions/2026-03-20-export-backup-restore-hardening.md`
- Final embed job ID: `056b1c9a-bb9a-4278-b7ae-db21afdcd863`
- Embed status outcome: `completed`
- Collection observed from embed status: `cortex`
- Source-like field observed from embed status: `metrics.source = rust`
- Retrieve attempt used: source ID `rust`, collection `cortex`
- Retrieve outcome: failed with `No content found for URL: rust`
- Net Axon indexing outcome for this session record: embed success, retrieve verification failed

## 10. Risks and rollback
- Export/restore is still one-way hardening in this session; no `axon restore` command exists yet.
- The session used a dirty working tree; many unrelated repo changes were present and were not modified or reverted.
- Rollback for this session’s code changes is by reverting the touched export/config/docs/test/migration files.
- Rollback for the DB migration is manual SQL drop/revert of `axon_query_history`, `axon_scrape_seeds`, and their indexes if explicitly desired.

## 11. Decisions not taken
- Did not implement `axon restore` in this session.
- Did not introduce a global migration runner; only added the migration file and applied it manually to the live DB.
- Did not overwrite any existing session log path; selected a new dated filename.

## 12. Open questions
- Whether runtime startup should automatically execute numbered SQL migrations instead of relying on manual application remains unresolved in this session.
- Whether `axon restore` should support dry-run and integrity-gated replay remains unresolved in this session.
- Whether additional restore-time validation beyond current integrity checks is needed remains unresolved in this session.

## 13. Next steps
- Implement `axon restore` with dry-run and integrity-gated replay.
- Decide on automated migration execution at startup or via a dedicated admin command.
- Add restore-path verification tests once restore behavior exists.
