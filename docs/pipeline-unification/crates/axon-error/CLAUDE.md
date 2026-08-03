# axon-error Agent Instructions

This file is the agent-facing contract for the `axon-error` crate docs. Keep it
short, operational, and distinct from `README.md`.

## When Editing

- Preserve `axon-error` as the lowest shared error boundary.
- Keep error taxonomy, retry, cooling, degradation, severity, and redacted
  context here.
- Do not add transport rendering, provider clients, stores, or job scheduling.
- Update `README.md`, `../../runtime/error-handling.md`, and
  `../../schemas/error-schema.md` together when error shapes change.
- Add/adjust schema fixtures for every new `ErrorCode`, stage, or policy.

## Review Checklist

- No dependency on higher crates.
- Every error has machine-readable stage, severity, retry, and degradation data.
- Display/debug output is redaction-safe.
