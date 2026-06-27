# Crate Ownership & the Service Boundary

Last Modified: 2026-06-26

**Canonical rule for where logic, contracts, and orchestration live in the Axon
workspace.** This supersedes the older "everything goes through `axon-services`"
framing. If code and this doc disagree, fix the code.

## The principle

> **Own the contract where the data lives. Reserve `axon-services` for
> composition and the job runtime. Let it be a thin *facade* (re-export), not a
> mandatory reimplementation hop.**

There are two separable concerns that "service layer" used to conflate:

1. **The contract boundary** — transports (CLI/MCP/REST/palette) must call a
   typed, transport-neutral entry point, never reach into a domain crate's
   internal modules (`axon_vector::ops::*`, `axon_ingest::<internal>`, …). This
   is non-negotiable.
2. **The aggregation crate** — whether *every* such entry point must live in one
   crate (`axon-services`). It must **not**. A single mega-crate forces
   pass-through ceremony and duplicate DTOs (the bug that motivated this doc:
   `PurgeResult` duplicated `QdrantDeleteByUrlResult`, and the CLI reached past
   the boundary into `axon_vector::ops::qdrant`).

## Where things go

The crate layering decides what *can* live where:

```
axon-api · axon-authz                         ← transport-neutral DTOs, scope checks
   ↓
axon-core                                     ← config, http, content, llm
   ↓
axon-crawl · axon-vector · axon-ingest        ← DOMAIN crates: own their logic
axon-extract · axon-code-index                   + a typed public service entry
   ↓
axon-jobs                                     ← job lifecycle + runtime
   ↓
axon-services                                 ← composition + ServiceContext + facade
   ↓
axon-mcp · axon-web · axon-cli (+ palette)    ← transports: thin shims over the boundary
```

| Kind of operation | Lives in | Why |
|---|---|---|
| **Contract DTO** (`*Result`) | `axon-api` | Transports already depend on it; no transport→domain-crate fan-out. (Precedent: `ServiceJob`, `IngestSource`, job DTOs already live here.) |
| **Single-domain logic** (no job runtime, one domain) — purge, dedupe, stats, query, classify | the **domain crate** (`axon-vector`, `axon-ingest`, …) as a typed `pub` entry | The crate that owns the data owns its API. |
| **Job-lifecycle ops** (need `ctx.jobs`) | `axon-services` | Domain crates are *below* `axon-jobs`; they physically can't depend on the runtime. |
| **Cross-domain orchestration** — scrape→embed, `ask` (retrieve+rank+LLM), the ingest pipeline | `axon-services` | Genuinely composes ≥2 domain crates. |
| **Cross-cutting policy** — scope mapping (`action_api`), partial-failure (`require_success`), preflight checks | `axon-services` | Knows about all actions / multiple domains. |
| **Transport facade** (`pub use` / thin error-adapting wrapper) | `axon-services` | Keeps one import surface for transports even when the impl lives in a domain crate. **This is a feature, not a smell.** |

## Decision procedure (use this when adding an operation)

1. Does it compose **≥2 domain crates**, or need the **job runtime** (`ctx.jobs`)?
   → It lives in `axon-services`.
2. Otherwise it's **single-domain** → the **logic** lives in the owning domain
   crate, the **DTO** lives in `axon-api`, and `axon-services` *may* re-export it
   so transports keep one import.
3. A transport **never** imports a domain crate's internal `::ops::` /
   `::<internal>::` paths. It calls the domain crate's public entry or the
   `axon-services` facade.

## Worked example — `purge`

| Layer | Holds |
|---|---|
| `axon-api::purge::PurgeResult` | the contract DTO |
| `axon-vector::purge` | the delete logic (`pub use ops::qdrant::qdrant_delete_by_url`) |
| `axon-services::system::purge` | a 3-line facade: calls `axon_vector::purge`, adapts the error to `Box<dyn Error>` |
| CLI / MCP / REST / palette | thin shims calling `services::system::purge` |

`dedupe` should follow the same shape (its `DedupeResult` is a candidate to move
to `axon-api`; logic stays in `axon-vector`).

## Migration policy — no forced churn

Apply this rule to **new** code and when you're **already editing** an
operation. Do **not** sweep the whole tree to relocate working code. The
existing reaches into domain internals are tracked by
`cargo xtask check-layering` (a seeded allowlist that prevents *new* violations
while documenting current debt).

## Enforcement

- `cargo xtask check-layering` — fails when a transport crate (`axon-cli`,
  `axon-web`, `axon-mcp`) imports a domain crate's internal module outside the
  allowlist. Run in CI; add new legitimate exceptions to the allowlist
  consciously (each is debt to pay down, not a free pass).
- Code review: a new `pub struct *Result` in `axon-services` for a single-domain
  op is a red flag — it probably belongs in `axon-api`.
