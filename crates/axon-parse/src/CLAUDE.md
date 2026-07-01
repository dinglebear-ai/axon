# axon-parse Agent Instructions

This file is the agent-facing contract for the `axon-parse` crate docs.

## When Editing

- Keep parser registry, source parse facts, and graph candidates here.
- Add dedicated parser coverage for manifests, schemas, sessions, tools, skills,
  agents, env examples, Docker Compose, REST APIs, and code where supported.
- Do not add acquisition, graph persistence, chunking output, or vector writes.
- Update `../../../docs/pipeline-unification/crates/axon-parse/README.md`, `../../../docs/pipeline-unification/sources/parsing-contract.md`,
  `../../../docs/pipeline-unification/sources/source-graph.md`, and schema docs together.

## Review Checklist

- Parser facts have evidence spans when available.
- Unsupported content degrades cleanly.
- Graph candidates are evidence-backed.
