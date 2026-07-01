# axon-error Agent Instructions

This file is the agent-facing contract for the `axon-error` crate docs. Keep it
short, operational, and distinct from `../../../docs/pipeline-unification/crates/axon-error/README.md`.

## When Editing

- Preserve `axon-error` as the lowest shared error boundary.
- Keep error taxonomy, retry, cooling, degradation, severity, and redacted
  context here.
- Do not add transport rendering, provider clients, stores, or job scheduling.
- Update `../../../docs/pipeline-unification/crates/axon-error/README.md`, `../../../docs/pipeline-unification/runtime/error-handling.md`, and
  `../../../docs/pipeline-unification/schemas/error-schema.md` together when error shapes change.
- Add/adjust schema fixtures for every new `ErrorCode`, stage, or policy.

## Review Checklist

- No dependency on higher crates.
- Every error has machine-readable stage, severity, retry, and degradation data.
- Display/debug output is redaction-safe.
