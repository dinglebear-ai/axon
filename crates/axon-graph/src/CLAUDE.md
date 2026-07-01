# axon-graph

This crate is part of the issue #298 pipeline-unification target structure.

## Ownership

- Owns the target boundaries documented in `docs/pipeline-unification/crates/axon-graph/README.md`.
- Contains marker modules only in PR0.
- Must not own runtime behavior until the implementation PR that moves that boundary also moves its contract tests.

## PR0 Rules

- Do not import from runtime crates.
- Do not change public CLI, MCP, REST, job, vector, crawl, embed, ingest, ask, memory, or watch behavior from this crate.
- Keep this crate compileable with workspace defaults and no external dependencies unless a later PR moves real behavior here.

## Modules

- `store`
- `sqlite`
- `migration`
- `node`
- `edge`
- `evidence`
- `candidate`
- `authority`
- `merge`
- `query`
- `testing`
