# axon-core Agent Instructions

This file is the agent-facing contract for the `axon-core` crate docs.

## When Editing

- Keep only reusable runtime primitives here: config, paths, ids, time,
  redaction primitives, HTTP/path safety, artifacts, diagnostics, and test
  utilities.
- Reject domain-specific helpers unless at least two crates need them and they
  do not create layering pressure.
- Do not add orchestration, acquisition, parsing, embedding, vector, job, or
  transport behavior.
- Update `README.md`, `../../foundation/shared-utilities-contract.md`,
  configuration docs, and schema docs when primitives change.

## Review Checklist

- No kitchen-sink drift.
- No dependency on higher orchestration or transport crates.
- Secrets are redacted in debug/display paths.
