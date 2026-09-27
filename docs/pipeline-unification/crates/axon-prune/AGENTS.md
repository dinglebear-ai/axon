# axon-prune Agent Instructions

This file is the agent-facing contract for the `axon-prune` crate docs.

## When Editing

- Keep cleanup debt execution, prune planning, old generation cleanup, orphan
  cleanup, dedupe, dry-run plans, safety checks, and receipts here.
- Do not add ledger ownership, source acquisition, embedding, or transport
  rendering.
- Update `README.md`, `../../runtime/pruning-contract.md`, and storage/ledger
  docs together.
- Assume empty DB clean-break semantics unless told otherwise.

## Review Checklist

- Dry-run and execute plans target the same items.
- Cleanup is idempotent.
- Receipts include counts, skipped reasons, and source/generation ids.
