# 1. Session overview
- Hardened Rust build resource controls (8GB memory cap, jobs limit, wrapper resolution) and validated service-migration test health.
- Repaired failing tests introduced by symbol moves and env-coupled test setup.
- Confirmed targeted migration regressions and full library test suite pass under deterministic threading.

# 2. Timeline of major activities
- Inspected working tree and executed `cargo check` (pass).
- Ran targeted migration tests; initial run failed during compile due missing imports in scrape migration tests.
- Patched scrape migration tests, reran targeted tests, then ran full `cargo test --lib`; addressed two failing tests.
- Patched env-sensitive config and artifact-response tests; reran targeted failures and full library suite with `--test-threads=1` (pass).
- Verified sccache presence/stats and confirmed wrapper/symlink state for cargo/rustc.

# 3. Key findings with path:line references
- Scrape migration tests referenced moved helpers and needed explicit imports from crawl layer: `crates/cli/commands/scrape/scrape_migration_tests.rs:13`.
- Web-origin allowlist test was coupled to env-provided core URLs; made deterministic by passing CLI flags: `crates/core/config/parse/build_config.rs:604`.
- MCP artifact response tests were vulnerable to shared env state for artifact root; isolated with per-test temp root + lock: `crates/mcp/server/artifacts/respond.rs:131`.
- Cargo wrapper enforces build-job default + cgroup/prlimit memory controls at 8GB: `/home/jmagar/.local/bin/cargo:80`, `/home/jmagar/.local/bin/cargo:91`, `/home/jmagar/.local/bin/cargo:99`.
- Repo-local Rust build defaults constrain parallelism and test/dev debug/codegen pressure: `.cargo/config.toml:1`.

# 4. Technical decisions and rationale
- Imported `select_output` and `build_scrape_website` from crawl module instead of reintroducing duplicate wrappers; keeps migration tests aligned with current architecture.
- Converted one env-dependent config test to CLI-arg service URLs to avoid cross-test env mutation races.
- Used async-aware lock (`tokio::sync::Mutex`) in async MCP tests to serialize env mutation in test runtime.
- Used deterministic full-suite verification (`cargo test --lib -- --test-threads=1`) after observing non-deterministic failures in parallel run.

# 5. Files modified/created and purpose
- `crates/cli/commands/scrape/scrape_migration_tests.rs`: fixed imports and timeout assertion typing for migrated helper location.
- `crates/core/config/parse/build_config.rs`: stabilized allowlist parsing test by passing required core URLs via CLI flags.
- `crates/mcp/server/artifacts/respond.rs`: isolated artifact env root per test to prevent shared-state failures.
- `.cargo/config.toml` (created): set sane repo defaults for Rust build/test memory pressure (`jobs=4`, lower debug/codegen pressure).

# 6. Critical commands executed and outcomes
- `git status --short` | confirmed dirty tree context and modified targets.
- `cargo check` | passed (`Finished dev profile`).
- `cargo test migrated_cli_commands_do_not_import_raw_business_logic_layers --lib` | passed after test fixes.
- `cargo test no_sync_service_modes_remain_on_subprocess_fallback --lib` | passed.
- `cargo test map_crawl_job_result_preserves_output_files --lib` | passed.
- `cargo test --lib` | initially failed (env/artifact/qdrant-runtime-flake context), then succeeded in deterministic run with `--test-threads=1`.
- `command -v sccache && sccache --version` | found `/usr/bin/sccache`, version `0.10.0`.
- `sccache --show-stats` | reported compile activity and zero compilation failures in current stats snapshot.
- `axon status` | showed queue state and pending embed jobs.
- `axon embed "docs/sessions/2026-03-11-memory-cap-migration-verification.md" --json` | returned queued job id `649c90ef-03a1-4790-bb58-2ca39db36f67`.
- `axon embed status "649c90ef-03a1-4790-bb58-2ca39db36f67" --json` | completed with `chunks_embedded=4`, `collection=cortex`, `source=rust`.
- `axon retrieve "docs/sessions/2026-03-11-memory-cap-migration-verification.md" --collection "cortex"` | returned indexed content (`Chunks: 4`).

# 7. Behavior changes (before/after)
- Before: scrape migration tests failed to compile due unresolved helper symbols in module scope.
- After: scrape migration tests compile and execute using current crawl-layer helper locations.
- Before: allowlist config test could fail when shared env state did not provide required core URLs.
- After: allowlist config test is deterministic using explicit CLI URL flags.
- Before: MCP artifact response tests could fail with temp file write error when artifact env path was invalid from shared state.
- After: MCP artifact response tests isolate artifact root per test and clean env on exit.

# 8. Verification evidence (`command | expected | actual | status`)
- `cargo check | compiles | Finished dev profile | PASS`
- `cargo test migrated_cli_commands_do_not_import_raw_business_logic_layers --lib | test passes | ok (1 passed) | PASS`
- `cargo test no_sync_service_modes_remain_on_subprocess_fallback --lib | test passes | ok (1 passed) | PASS`
- `cargo test map_crawl_job_result_preserves_output_files --lib | test passes | ok (1 passed) | PASS`
- `cargo test explicit_inline_mode_returns_inline_data --lib | test passes | ok (1 passed) | PASS`
- `cargo test qdrant_scroll_pages_visits_all_inserted_points --lib | test passes | ok (1 passed) | PASS`
- `cargo test --lib -- --test-threads=1 | suite passes | 1149 passed; 0 failed; 5 ignored | PASS`
- `cargo build -q | build succeeds | exit code 0 | PASS`
- `sccache --show-stats | reports compile stats without failures | Compilation failures: 0 | PASS`
- `axon embed "docs/sessions/2026-03-11-memory-cap-migration-verification.md" --json | queued embed job | {"job_id":"649c90ef-03a1-4790-bb58-2ca39db36f67","source":"rust","status":"pending"} | PASS`
- `axon embed status "649c90ef-03a1-4790-bb58-2ca39db36f67" --json | completed embed with metadata | status=completed, result_json={chunks_embedded:4,collection:cortex,source:rust} | PASS`
- `axon retrieve "rust" --collection "cortex" | retrieve by status source value | No content found for URL: rust | FAIL`
- `axon retrieve "docs/sessions/2026-03-11-memory-cap-migration-verification.md" --collection "cortex" | retrieve indexed session doc | Retrieve Result ... Chunks: 4 | PASS`

# 9. Source IDs + collections touched (embed/retrieve source IDs, collections, outcomes)
- Embed job id: `649c90ef-03a1-4790-bb58-2ca39db36f67`.
- Embed status output fields observed: `result_json.source=rust`, `result_json.collection=cortex`, `result_json.chunks_embedded=4`.
- Retrieve attempt using status `source` value (`rust`) failed (`No content found for URL: rust`).
- Retrieve attempt using indexed document path (`docs/sessions/2026-03-11-memory-cap-migration-verification.md`) with `--collection cortex` succeeded (`Chunks: 4`).

# 10. Risks and rollback
- Risk: `cargo test --lib` without deterministic threading can still exhibit transient failures in some integration-adjacent tests.
- Risk: local wrapper changes in `/home/jmagar/.local/bin` are host-scoped, not repo-scoped.
- Rollback (repo): `git checkout -- crates/cli/commands/scrape/scrape_migration_tests.rs crates/core/config/parse/build_config.rs crates/mcp/server/artifacts/respond.rs .cargo/config.toml` (if needed).
- Rollback (host wrappers): restore prior `/home/jmagar/.local/bin/cargo` and `/home/jmagar/.local/bin/rustc` from backup/history.

# 11. Decisions not taken
- Did not reintroduce legacy CLI helper wrappers for scrape tests; kept tests aligned to migrated crawl-layer helpers.
- Did not mark qdrant scroll test ignored; instead verified it passes and used deterministic full-suite run.
- Did not remove existing dirty-tree changes outside touched files.

# 12. Open questions
- Should default CI/unit test invocation use `--test-threads=1` for stability, or should flaky tests be isolated/serialized individually?
- Should host-level wrapper policy (memory cap and jobs) be documented in repo docs for reproducibility across dev machines?

# 13. Next steps
- Continue remaining service-migration completion work (`#13`, `#30`) on top of this now-stable test baseline.
- If desired, codify deterministic test-thread policy in CI or add serial guards only where required.
