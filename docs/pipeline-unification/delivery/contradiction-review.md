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

### `refresh`/`fresh` Removal Was Missing From `command-contract.md`

Problem:

`surfaces/command-contract.md`'s "Removed Commands" enumeration omitted
`axon refresh` and `axon fresh`, while `delivery/surface-removal-contract.md`,
`foundation/source-pipeline.md` (`refresh existing` crosswalk row), and
`plans/finish-unification-metaplan.md` all already treated both as removed
commands with documented canonical replacements
(`axon <source> --refresh` and `axon watch ...` / source freshness config,
respectively). A Phase 10 drift review initially read this gap as "refresh/fresh
are in scope to keep," but the weight of evidence across the other three
contract docs says they are genuinely removed — `command-contract.md` was
simply stale.

Resolution:

`surfaces/command-contract.md`'s Removed Commands list now includes
`axon refresh [filter]` and `axon fresh <sub>`, matching the other three docs.
No scope change: refresh/fresh removal was already the intended target state.

This does **not** mean `watch` (current URL-diff scheduler, or its future
source-request-backed replacement) already reproduces `refresh`'s bulk
re-enqueue-by-origin semantics or `fresh`'s CLI-created recurring-schedule
semantics today. `crates/axon-services/src/refresh.rs` (facet-and-re-enqueue by
`seed_url`/`source_type`) and `crates/axon-services/src/freshness/` (SQLite
`FreshnessDef`/`FreshnessRun` lease-based scheduler dispatching
scrape/crawl/embed/ingest) are both real, load-bearing, and have no drop-in
replacement in `crates/axon-jobs/src/watch.rs` today. Removing them without
first building the target `watch <source>` / `SourceLedger`-backed freshness
lifecycle (see `foundation/source-pipeline.md`, `surfaces/command-contract.md`
Watch Commands section) would be a real functionality regression, not a
same-day-safe deletion — treat the actual removal as scoped, sequenced work
under the Phase 10/11 cutover, not a quick surface-drift fix.

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

## Design Decisions — Resolved In Landed Code (verified 2026-07-10, HEAD `5a4558cc7`)

The four decisions below carried `Decision needed` when this doc was written.
All four now match their recommended default in landed code; kept here as a
record rather than an open question.

### Retrieval Query Embedding Boundary — resolved

`axon-retrieval::RetrievalEngine` is generic over `E: EmbeddingProvider`
(`crates/axon-retrieval/src/boundary.rs`), not a concrete TEI/OpenAI client.
Tests seed it with `FakeEmbeddingProvider`
(`crates/axon-retrieval/src/boundary_tests.rs`), confirming the trait-only
dependency.

### Job Runner Injection Shape — resolved

`axon-jobs`'s `Cargo.toml` carries no dependency on `axon-services`; runner
functions/closures are injected from the composition layer, matching the
recommended default.

### `ArtifactStore` Home — resolved

`ArtifactStore` remains in `crates/axon-core/src/boundary.rs`, matching the
recommended default (no object-store/process-boundary need has emerged to
justify promoting it to its own crate).

### `extract` Naming — resolved

`axon extract` remains the top-level structured-data LLM command; internal
pipeline stage names (`acquire`, `parse`, `enrich`, `prepare`) are used for
the source-pipeline stages per `source-pipeline.md`'s Stage Registry, matching
the recommended default.

## No Current Blockers

No contradiction found that blocks writing the detailed implementation plan.
The biggest caution is implementation order: split contracts and fakes before
moving high-risk `axon-vector` behavior so retrieval latency/correctness does
not regress invisibly.
