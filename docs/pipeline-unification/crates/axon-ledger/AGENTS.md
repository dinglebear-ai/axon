# axon-ledger Agent Instructions

This file is the agent-facing contract for the `axon-ledger` crate docs.

## When Editing

- Keep durable source accounting here: source records, manifests, diffs,
  generations, document status, leases, and cleanup debt.
- Do not add source fetching, parsing, embedding, vector writes, or cleanup
  execution.
- Update `README.md`, `../../runtime/ledger-contract.md`, and
  `../../schemas/database-schema.md` together.
- Assume an empty database for the clean-break implementation unless the user
  explicitly asks for migration support.

## Review Checklist

- Generation commit/publish is transactional.
- Failed generations never become visible search state.
- Cleanup debt is durable and idempotent.
