# axon-authz Agent Instructions

This file is the agent-facing contract for the `axon-authz` crate docs.

## When Editing

- Keep caller context, scopes, execution affinity, visibility, and policy
  decisions here.
- Do not add OAuth middleware, bearer-token parsing, source fetching, or
  redaction detectors.
- Update `README.md`, `../../runtime/auth-contract.md`, and
  `../../runtime/security-contract.md` together for policy changes.
- Ensure jobs can persist enough caller context to re-check policy later.

## Review Checklist

- Denials include stable machine-readable reasons.
- Ambiguous security decisions fail closed.
- Transports authenticate; this crate authorizes.
