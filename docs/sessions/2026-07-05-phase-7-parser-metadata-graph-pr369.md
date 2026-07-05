# Phase 7 Parser Metadata Graph PR #369

Date: 2026-07-05

Worktree: `/home/jmagar/workspace/axon/.worktrees/phase-7-parser-metadata-graph`

Branch: `codex/phase-7-parser-metadata-graph`

Base: `codex/phase-4-source-resolver-router` at `75daf47c1de6d98beac6bbf0bb4e866ffdc99125`

PR: https://github.com/jmagar/axon/pull/369

## Summary

Executed Phase 7 parser/metadata/graph plan on top of Phase 4. The branch adds parser families for Docker/Compose, env examples, and observed tool output; carries parse facts and graph candidates into prepared documents; validates graph evidence source ranges; extends vector payload source families; and adds graph lineage fixtures and generated schema updates.

Review remediation added URI redaction for tool external resources, hardened parser-family and graph-candidate tests, added Compose `env_file`/`secrets`/`depends_on` extraction, added vector payload source-input provenance for `payload_families.rs`, split monolith-sensitive parser/preparer code, and refreshed the generated vector payload snapshot.

## Verification

- `cargo test -p axon-parse --no-fail-fast` passed, 37 tests.
- `cargo test -p axon-document --no-fail-fast` passed, 27 tests.
- `cargo test -p axon-graph --no-fail-fast` passed, 52 tests.
- `cargo test -p axon-extract --lib --no-fail-fast` passed, 153 tests.
- `cargo test -p axon-vectors payload --no-fail-fast` passed, 35 tests, 84 filtered.
- `cargo test -p axon-llm provider --no-fail-fast` passed, 7 tests, 128 filtered.
- `cargo check -p axon-services --lib` passed with existing warnings.
- `cargo test -p axon-services tool_policy --no-fail-fast` passed, 2 tests, 585 filtered.
- `cargo xtask schemas vector-payload --check` passed.
- `git diff --check` passed.
- Contract scans for legacy `source_item`/`declares`/`source_line` and forbidden `candidate_with_edge`/`tool_execution_policy` returned no matches.
- Pre-commit passed: structural checks, monolith, rustfmt.
- Pre-push passed: structural checks.

## Review Loop

- Independent review agent found tool URI redaction, Compose coverage, graph evidence range validation, and vertical artifact wiring concerns.
- Simplification pass 1 flagged dead/unwired tool policy, metadata-bound range validation, repeated Compose line scans, duplicate range construction, and duplicate secret/local helpers.
- Simplification pass 2 flagged vacuous graph-candidate tests and oversized lineage fixture shape.
- Simplification pass 3 flagged missing vector payload provenance for `payload_families.rs`.
- PR-toolkit-equivalent review flagged generated schema source-input drift.
- External PR comments were fetched with `gh-fetch-comments --pr 369 --output /tmp/phase7-pr-comments.json --no-beads`; only quota/skip notices were present.

## PR Status

PR #369 head is `474ef214f3d71d870e5f26ef3cfbb5a8c856a0c2` before this session-note commit. Base remains `codex/phase-4-source-resolver-router`. GitHub reports `UNSTABLE` because Cubic review is pending; CodeRabbit and GitGuardian passed, Claude review skipped, and no actionable review comments were present.

## Residual Risks

- `ScrapedDoc::parse_artifacts` is implemented and tested for GitHub repo verticals, but production web-source indexing does not yet carry the original `ScrapedDoc` through to `PrepareSourceDocumentRequest`; wiring that bridge is larger than the review-fix scope.
- Compose service dependency edges use the closed `derived_from` edge kind because the current graph registry has no dedicated service-to-service dependency edge.
- Existing unrelated warnings remain in `axon-services`, `axon-cli`, `xtask`, and one `axon-vectors` test import.
