# axon-memory Agent Instructions

This file is the agent-facing contract for the `axon-memory` crate docs.

## When Editing

- Keep durable memory lifecycle, recall, decay, reinforcement, review, context,
  and graph links here.
- Do not turn memory into a generic source adapter or vector-store owner. The
  one sanctioned projection is the narrow `memory` adapter in `axon-adapters`
  (`memory://mem_<id>`, exactly one record per request), which reads through
  the neutral `MemorySourceProvider` boundary; persistence and lifecycle stay
  in this crate.
- Update `README.md`, `../../runtime/memory-contract.md`,
  `../../sources/source-graph.md`, and metadata/schema docs together.
- Preserve memory links to sessions, repos, issues, artifacts, tools, skills,
  and agents.

## Review Checklist

- Supersession preserves history.
- Decay/review policies are explicit and testable.
- Recall output is bounded and cited.
