# Phase 2 Schema Contracts PR 366

Date: 2026-07-04

Worktree: `/home/jmagar/workspace/axon/.worktrees/phase-2-schema-contracts`
Branch: `codex/phase-2-schema-contracts`
Base: `work/phase-1-contract-alignment`
PR: https://github.com/jmagar/axon/pull/366

## Summary

Executed the Phase 2 schema contract alignment plan in a stacked worktree on top
of Phase 1. The PR adds registry-backed schema generation for the required
families, generated artifact manifests/checks, fixture validation, and
cross-family drift checks.

Follow-up review fixes addressed:

- REST schema registry drift against the current OpenAPI route inventory.
- Per-route response metadata for route-specific statuses.
- `--update-fixtures` now refreshes snapshot fixtures instead of only warning.
- Fixture validation now fails when valid, invalid, or snapshot categories are
  empty.
- Removed-surface checks no longer treat currently routed REST purge/dedupe
  paths as Phase 2 removals; those remain Phase 10 cleanup scope.

## Verification

- `cargo test -p xtask schemas:: --no-fail-fast`
- `cargo xtask schemas generate --check`
- `cargo xtask schemas generate --check --json`
- `jq '[.[].fixtures_validated, .[].snapshots_checked] | min' /tmp/axon-schema-generator-report.json`
- `cargo xtask check-doc-contracts`
- `cargo xtask check-doc-links`
- `cargo xtask check-layering`
- `git diff --check`

## Review Notes

Local review agents found and the PR fixed:

- Watch route path mismatch in the generated REST registry.
- Coarse response metadata for sync routes such as ask.
- No-op fixture update mode.
- Missing regression guard for empty fixture/snapshot categories.
- Missing comparison between generated REST registry and current OpenAPI route
  inventory.

External PR bots did not provide actionable review: CodeRabbit skipped because
the base branch is not the default branch, and Copilot/Codex review hit quota.
