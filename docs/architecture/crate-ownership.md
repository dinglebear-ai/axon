---
title: "Crate Ownership & the Service Boundary"
created: 2026-06-27
updated: 2026-07-30
---

# Crate Ownership & the Service Boundary

Last Modified: 2026-07-26

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
   internal modules (`axon_prune::executor::*`, `axon_vectors::qdrant::*`, …).
   This is non-negotiable.
2. **The aggregation crate** — whether *every* such entry point must live in one
   crate (`axon-services`). It must **not**. A single mega-crate forces
   pass-through ceremony and duplicate DTOs (the bug that motivated this doc:
   a service-layer result type duplicating a domain-crate result type, with a
   transport reaching past the boundary into a domain crate's internal `ops`
   module).

## Where things go

The crate layering decides what *can* live where. The current workspace has 23
crates; the authoritative, kept-current diagram and per-crate table live in
[`crate-structure.md`](crate-structure.md) — do not duplicate that diagram
here, since a second copy is exactly what let this doc rot out of sync with
the real crate list. In short: cross-cutting contract crates
(`axon-error`, `axon-api`, `axon-authz`, `axon-core`, `axon-observe`) sit below
the domain crates (acquisition, ledger, graph, memory, document, embedding,
vectors, retrieval, llm, prune, …), which sit below `axon-jobs`, which sits
below the `axon-services` facade, which sits below the transports
(`axon-cli`, `axon-mcp`, `axon-web`).

| Kind of operation | Lives in | Why |
|---|---|---|
| **Contract DTO** (`*Result`) | `axon-api` | Transports already depend on it; no transport→domain-crate fan-out. (Precedent: `ServiceJob`, `PruneResult`, and other job/source DTOs already live here.) |
| **Single-domain logic** (no job runtime, one domain) — prune plan/execute, dedupe, stats, query, classify | the **domain crate** that owns the data (e.g. `axon-prune`, `axon-vectors`) as a typed `pub` entry | The crate that owns the data owns its API. |
| **Job-lifecycle ops** (need `ctx.jobs`) | `axon-services` | Domain crates are *below* `axon-jobs`; they physically can't depend on the runtime. |
| **Cross-domain orchestration** — acquire→prepare→embed→publish, `ask` (retrieve+rank+LLM), the source pipeline | `axon-services` | Genuinely composes ≥2 domain crates. |
| **Cross-cutting policy** — scope mapping (`action_api`), partial-failure (`require_success`), preflight checks | `axon-services` | Knows about all actions / multiple domains. |
| **Transport facade** (`pub use` / thin error-adapting wrapper) | `axon-services` | Keeps one import surface for transports even when the impl lives in a domain crate. **This is a feature, not a smell.** |

## Decision procedure (use this when adding an operation)

1. Does it compose **≥2 domain crates**, or need the **job runtime** (`ctx.jobs`)?
   → It lives in `axon-services`.
2. Otherwise it's **single-domain** → the **logic** lives in the owning domain
   crate, the **DTO** lives in `axon-api`, and `axon-services` *may* re-export it
   so transports keep one import.
3. A transport **never** imports a domain crate's internal `::ops::` /
   `::executor::` / other private-module paths. It calls the domain crate's
   public entry or the `axon-services` facade.

## Worked example — `prune`

`prune` is a live command (`axon prune plan` / `axon prune exec --confirm`)
and a good template because it already follows the rule end to end:

| Layer | Holds |
|---|---|
| `axon-api::source::prune::{PruneRequest, PrunePlan, PruneResult, PruneSelector, ...}` | the contract DTOs |
| `axon-prune` (`plan.rs`, `executor.rs`, `dedupe.rs`, `orphan.rs`, `debt.rs`, `generation.rs`, `safety.rs`, `receipt.rs`) | the plan/execute/receipt logic plus the destructive store-delete boundary (`PruneExecutor`, `PruneTarget`) |
| `axon-services::prune` | transport-neutral entrypoint (`prune_plan` / `prune_execute`) that resolves a `PruneRequest` via `axon_prune::PrunePlanner`, then — only on explicit execute — runs it through `PruneExecutor`, threading caller-derived `PruneAuthz` |
| CLI / MCP / REST | thin shims calling `services::prune`, never reaching into `axon_prune::executor::*` or `axon_vectors::qdrant::*` directly |

`axon-prune` also re-exports the `axon-api` DTOs it produces/consumes
(`pub use axon_api::source::prune::{PruneCounts, PruneEstimate, PrunePlan, ...}`
in `crates/axon-prune/src/lib.rs`), so callers can `use axon_prune::PruneResult`
without a direct `axon-api` import — the same "facade, not forced hop" pattern
`axon-services` uses one layer up.

## Migration policy — no forced churn

Apply this rule to **new** code and when you're **already editing** an
operation. Do **not** sweep the whole tree to relocate working code. The
existing reaches into domain internals are tracked by
`cargo xtask check-layering`. Its exact exception ledger records the owning
Bead and expected occurrence count for each remaining violation, so new reaches
and unreviewed count drift fail closed.

## Enforcement

- `cargo xtask check-layering` — fails when a transport crate (`axon-cli`,
  `axon-web`, `axon-mcp`) imports forbidden domain/provider surfaces, or when a
  transport manifest declares a forbidden dependency. The fail-closed AST gate
  audits all 23 live crates, resolves dependency aliases and production module
  reachability, and permits only exact path/rule/count exceptions tied to open
  Beads. Run it in CI; every removed exception is a verified debt reduction,
  and new exceptions require explicit ownership rather than a broad allowlist.
- Code review: a new `pub struct *Result` in `axon-services` for a single-domain
  op is a red flag — it probably belongs in `axon-api`.
