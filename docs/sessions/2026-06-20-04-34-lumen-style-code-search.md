# Lumen-Style Code Search

Date: 2026-06-20
Branch: `codex/lumen-style-code-search`

## Summary

Implemented first-class local code search for Axon with CLI and MCP surfaces only.
The shipped path indexes one local Git project at a time, keeps absolute roots in
SQLite only, writes Qdrant payloads with relative paths and `local_project_key`,
and marks returned snippets as `untrusted_local_code`.

## Scope

- SQLite-backed local code index state, manifest diffing, pending
  sentinels, generation state, and single-flight freshness path.
- Batched changed-file embedding and generation-fenced local-code deletes.
- Local-project Qdrant filters, payload indexes, and code-search retrieval
  ranking that accepts small source chunks.
- `axon code-search` and MCP `action=code_search` as write-scoped surfaces.
- Deferred REST/OpenAPI, donor seeding, global code search, background refresh,
  and UI work.

## Verification

- `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test code_index::tests -- --nocapture`
- `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test code_search_score_policy -- --nocapture`
- `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test build_candidates_trace_records_low_signal_drops -- --nocapture`
- `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test code_search_result_marks_snippets_untrusted -- --nocapture`
- `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test mcp_schema_includes_code_search -- --nocapture`
- `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test cli_help_contract_includes_code_search -- --nocapture`
- `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test curated_command_sections_cover_current_clap_surface -- --nocapture`
- `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo test env_config_boundary_matrix_is_current -- --nocapture`
- `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 ./scripts/axon doctor`
- Live smoke with temporary Git repo and collection `code_search_smoke_1781940510`:
  indexed `alpha`, committed `beta`, returned `lib.rs` with `beta`, and did not
  expose the temporary absolute root in JSON output.
- `cargo fmt --all --check`
- `git diff --check`
- `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 timeout 900s cargo clippy --all-targets --all-features`
- `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 timeout 900s cargo test`

## Notes

The repo's generated web output is not present in this worktree, so verification
used `AXON_ALLOW_FALLBACK_WEB_ASSETS=1`; this produced the expected fallback web
panel warning and did not affect code-search behavior.
