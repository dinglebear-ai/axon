---
type: "Reference"
title: "Testing Guidance"
description: "Practical validation guidance for the `e7d34a6b` update: CI gates, wrapper behavior, and documentation-generation checks."
---

# Testing Guidance

## Source-level verification

This update mostly changes CI/workflow/documentation plumbing and error-chain behavior in startup. Focus tests accordingly:

- `cargo build --bin axon` (or equivalent wrapper path via `scripts`)
- wrapper behavior smoke: confirm expected compile wrapper path (helper/delegate or `sccache` fallback) in local runs.
- targeted CI/job-path checks in `.github/workflows/ci.yml` if changing gating conditions.

## Relevant CI gates

The primary active gates in this range include:

- re-enabled `test`, `clippy`, `release-smoke` paths under path predicates,
- stronger `verify required jobs` terminal check,
- schema/docs contract checks in `mcp-schema-doc-sync`.

## Generated docs validation

- run `python3 scripts/generate_action_docs.py` after action/parity source changes,
- confirm generated `Surfaces` blocks and `docs/reference/actions/README.md` are synchronized,
- validate parity source (`docs/reference/api-parity.md`) when behavior claims change.

## OpenWiki update checks

- If `openwiki-update` automation changes, verify preflight endpoint success and PR payload includes expected control files (`openwiki`, `AGENTS.md`, `CLAUDE.md`, `.github/workflows/openwiki-update.yml`).

## Notes

Avoid broad new test additions unless runtime or parser/core behavior changed substantially. Keep changes scoped to the surfaces touched by this commit set.
