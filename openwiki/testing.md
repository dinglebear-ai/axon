# Testing Guidance

## Source-level command updates

This update touched primarily workflow/docs/tooling surfaces, not core runtime algorithms. Testing remains focused on:

- Workflow smoke checks and regeneration checks
- Existing source-level verification for API parity, doc contracts, and compile gates

## Quick validation matrix

- `cargo xtask` gates from CI:
  - `check-doc-contracts`
  - `check-api-parity` (through relevant CI steps)
  - `check-dep-graph`
  - `check-redaction-logs`
- OpenWiki docs sanity:
  - `python3 scripts/generate_action_docs.py --check` (if used in your local flow)
- Wrapper behavior:
  - run representative `cargo build --bin axon` and confirm expected binaries still appear in standard paths.

## Helpful commands

- `python3 scripts/generate_action_docs.py`
- `just taplo-check`
- `just taplo-fmt`
- `just test` / `just test-fast` (or your branch-specific command sets)
- `cargo xtask` checks configured in `.github/workflows/ci.yml`

## Notes

Because this run is documentation/tooling-dominant, avoid adding broad additional test suites unless core runtime behavior changed (for example, service contracts, handlers, or CLI semantics).
