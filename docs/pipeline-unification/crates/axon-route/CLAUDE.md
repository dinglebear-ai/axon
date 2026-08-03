# axon-route Agent Instructions

This file is the agent-facing contract for the `axon-route` crate docs.

## When Editing

- Keep source resolution, canonical URI, source id, scope validation, aliasing,
  authority records, and adapter matching here.
- Do not add acquisition/fetching, parsing, ledger writes, or vector behavior.
- Update `README.md`, `../../sources/url-normalization.md`, and
  `../../sources/adapter-scopes.md` together.
- Treat bare domains, aliases, official docs, repos, packages, and scopes as
  explicit routing cases.

## Review Checklist

- Resolution is deterministic without network access when possible.
- Ambiguous scopes produce actionable errors.
- Adapter routing is capability-based.
