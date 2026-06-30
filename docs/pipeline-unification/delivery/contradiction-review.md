# Contradiction Review
Last Modified: 2026-06-30

## Contract

This file captures the contradiction sweep across the target contracts and the
current implementation sweep. It should be empty of unresolved blockers before
the detailed implementation plan is written.

## Reviewed Inputs

- [../README.md](../README.md)
- [../foundation/source-pipeline.md](../foundation/source-pipeline.md)
- [../foundation/crate-structure.md](../foundation/crate-structure.md)
- [../foundation/repo-structure.md](../foundation/repo-structure.md)
- [../foundation/boundary-map.md](../foundation/boundary-map.md)
- [../crates/README.md](../crates/README.md)
- [current-implementation-sweep.md](current-implementation-sweep.md)

## Resolved Issues

### `axon-jobs` Depending On `axon-services`

Problem:

`axon-jobs` originally allowed `axon-services` at worker-runner composition
points, which would make the target graph vulnerable to a cycle because
`axon-services` needs job runtime access.

Resolution:

`axon-jobs` may not depend on `axon-services`. The composition layer must inject
worker functions, closures, or small traits into the job runtime. Jobs own
durable scheduling and worker mechanics; services own orchestration.

### Repo Structure Module Drift

Problem:

`foundation/repo-structure.md` had stale shorthand module lists that did not
match the new per-crate READMEs.

Resolution:

The repo structure doc now states that the per-crate README files are canonical
and mirrors their module lists.

### Current Ingest Coverage Was Understated

Problem:

Some target docs used GitHub/Reddit/YouTube as examples and could imply those
were the only existing ingest sources.

Resolution:

The current implementation sweep records GitLab, Gitea/Forgejo, generic Git,
RSS/Atom/JSON feeds, and sessions as current first-class implementation paths
that must be represented in adapters/scopes/new-source contracts.

### Current Job Runtime Strengths Were Understated

Problem:

The target job contract mentioned heartbeats and progress, but the current
runtime also has watchdog recovery, panic guard, starvation detection,
cancellation tokens, and bounded channels.

Resolution:

The implementation checklist and current sweep call these out as behaviors to
preserve when moving to one durable job model.

## Remaining Design Decisions For Implementation Plan

### Retrieval Query Embedding Boundary

Decision needed:

Should `axon-retrieval` call `EmbeddingProvider` directly for query embeddings,
or should `axon-services` produce query embeddings and pass vectors into
`RetrievalEngine`?

Recommended default:

Let `axon-retrieval` depend on the `EmbeddingProvider` trait, not the concrete
provider, because retrieval planning needs to decide single-query vs dual-query
embedding behavior. It must not depend on TEI/OpenAI concrete clients.

### Job Runner Injection Shape

Decision needed:

How should `axon-services` and `axon-jobs` compose without cycles?

Recommended default:

Define runner traits or boxed async runner functions in `axon-api` or
`axon-jobs`, and have the top-level bootstrap crate or `axon-services` register
them with `JobRuntime`. The job crate stores/runs jobs; it does not import the
service crate.

### `ArtifactStore` Home

Decision needed:

Should `ArtifactStore` remain in `axon-core` or become its own crate?

Recommended default:

Keep artifact primitives in `axon-core` until there is more than filesystem
storage or object-store support is implemented. Promote later only if the
boundary crosses process/network/security concerns materially.

### `extract` Naming

Decision needed:

Structured LLM extraction remains top-level, while extraction/acquisition inside
the source pipeline should not be called an indexing category.

Recommended default:

Keep `axon extract` as the explicit structured-data LLM command/action. Use
`acquire`, `parse`, `enrich`, and `prepare` for internal pipeline stages.

## No Current Blockers

No contradiction found that blocks writing the detailed implementation plan.
The biggest caution is implementation order: split contracts and fakes before
moving high-risk `axon-vector` behavior so retrieval latency/correctness does
not regress invisibly.
