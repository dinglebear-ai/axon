# axon-services — Agent Guide

`axon-services` owns **transport-neutral orchestration** — the use-case / facade
layer. It composes the domain crates (routing, adapters, ledger, parse, graph,
document, embedding, vector, retrieval, LLM, memory, prune, jobs, authz, observe)
behind typed service entrypoints, owns `ServiceContext`, and converts `axon-api`
requests into domain boundary calls. Full contract (owns / API / deps / tests):
[../../../docs/pipeline-unification/crates/axon-services/README.md](../../../docs/pipeline-unification/crates/axon-services/README.md)
· behavior spec:
[../../../docs/pipeline-unification/foundation/types/service-contract.md](../../../docs/pipeline-unification/foundation/types/service-contract.md).

## Status — live crate, cutover at Phase 10
This is the composition layer / re-export facade over the current domain crates,
and works today. At the **Phase 10 surface cutover** the service registry
converges on the end-state action set (source/map/extract/ask/query/retrieve/
search/memory/graph/jobs/providers/config/status/prune) so every CLI, MCP, and
REST action maps to exactly one service entrypoint. It is a **facade, not a
mandatory reimplementation hop** — single-domain logic stays in its domain crate;
only cross-domain or job-runtime orchestration lives *in* this crate.

## Module map
Current groups from `crates/axon-services/src/` (target consolidations noted):
| Area | Owns |
|---|---|
| `lib.rs` · `context.rs` · `runtime/` | crate root + `ServiceContext` / dependency container + job runtime wiring |
| `scrape.rs` · `crawl.rs` · `crawl_sync/` · `ingest/` · `map.rs` | source acquisition paths → converge under `source.rs`/`map.rs` |
| `ask.rs`* · `query.rs` · `search/` · `summarize.rs` · `document.rs` | retrieval/RAG + query/search entrypoints (`ask.rs`/`query.rs`/`retrieve.rs`) |
| `extract.rs` · `brand.rs` · `endpoints/` · `diff.rs` · `screenshot.rs` | structured extraction + derived-content actions |
| `memory/` · `embed.rs` · `refresh.rs` · `freshness/` · `watch.rs` | memory, embedding, freshness/watch use-cases |
| `jobs.rs` · `migrate.rs` · `system/` · `config.rs` · `action_api/` · `client_contract/` · `transport.rs` · `types/` | job/system/config services + shared action/result assembly |

## Boundary — keep OUT of this crate
- Transport-specific parsing/rendering, stdout/stderr output, HTTP route or MCP tool registration.
- Domain internals that belong in lower crates; duplicate DTOs instead of `axon-api`.
- Provider clients or stores outside injected boundaries.

## Dependencies
- **Allowed:** all lower domain and provider-boundary crates, plus `axon-api`/`axon-error`/`axon-core`/`axon-authz`/`axon-observe`/`axon-jobs`.
- **Forbidden:** `axon-cli`, `axon-mcp`, `axon-web`; stdout/stderr rendering; HTTP/MCP registration. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- Every transport action has exactly one service entrypoint; adding a source/action changes service registration once, not per transport.
- Source pipeline stage order matches `foundation/source-pipeline.md`; stage results are explicit and observable.
- Errors, progress, document status, and cleanup debt are emitted consistently (via `axon-error`/`axon-observe`).
- No service writes around injected stores/providers.

## DTO ownership
Every service function returns an **`axon-api`** result DTO — no raw JSON printing
or stdout side-effects. Transports call these service entrypoints and the
`axon-api` DTOs, never a domain crate's `::ops::*` or internals; this crate is the
one hop allowed to reach into domain crates.

## Keep in sync when shapes change
`README.md` (crate contract) · `foundation/types/service-contract.md` ·
`foundation/source-pipeline.md` · the CLI/MCP/REST surface contracts · the
request/result DTOs in `axon-api`.
