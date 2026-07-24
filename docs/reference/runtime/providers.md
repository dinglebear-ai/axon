# Providers

Last Modified: 2026-07-19

External services and the stores that act like them are modeled as provider
boundaries with capability, health, reservation, and cooling semantics. Provider
throughput is scheduled globally so bulk source embedding does not starve
interactive `ask`/`query`/`retrieve`.

> Contract source:
> [`docs/pipeline-unification/runtime/provider-contract.md`](../../pipeline-unification/runtime/provider-contract.md).
> Capability schema: [`provider-capabilities.schema.json`](provider-capabilities.schema.json).

## Boundaries

| Boundary | Implementations |
|---|---|
| `EmbeddingProvider` | TEI, OpenAI-compatible, fake |
| `VectorStore` | Qdrant, fake |
| `LlmProvider` | Gemini headless, OpenAI-compat, Codex app-server, fake |
| `SearchProvider` | SearXNG, Tavily, fake |
| `FetchProvider` | reqwest, git, registry clients, fake |
| `RenderProvider` | Chrome/CDP, fake |
| `ArtifactStore` | filesystem, fake |
| `LedgerStore` / `GraphStore` / `MemoryStore` / `JobStore` | SQLite + in-memory fakes |

## Capability document

`provider_id` (stable instance id), `provider_kind` (boundary class),
`implementation` (`qdrant`/`tei`/`gemini-headless`), `version`, `models`,
`dimensions`, `limits` (concurrency/batch/bytes/token/timeout), `features`,
`health`, `cooldown_until`, `last_error` (redacted).

**Status:** `ready` / `degraded` / `cooling` / `unavailable` / `disabled`.

## Reservations

`reservation_id`, `job_id`, `stage_id`, `provider_kind`, `priority`, `units`,
`estimated_inputs`, `deadline`, `granted_at`, `expires_at`, `state`.

Priority ∈ `interactive` / `normal` / `background` / `maintenance`. State
lifecycle: `requested → queued → granted → active → released` (+ `canceled` /
`expired` / `failed`). Providers reject calls without reservations except health
checks. Stale active reservations are reclaimed only after heartbeat grace +
lease expiry.

## Cooling

Repeated retryable failures enter cooldown: cooldown blocks **new** reservations
but preserves already-active requests until their timeout/cancellation; safe
cleanup/finalization may continue; a health probe can end cooldown early.

Cooling fields: `failure_count`, `cooldown_reason` (`rate_limit`/`timeout`/
`unavailable`/`resource_exhausted`/…), `cooldown_started_at`, `cooldown_until`,
`probe_after`, `last_success_at`, `last_error`.

## Global throughput scheduler

Capacity classes: `embedding`, `vector_write`, `vector_read`, `llm`, `fetch`,
`render`, `parse`, `graph_write`, `artifact_write`.

**Interactive vs bulk (embedding lanes):**

| Lane | Used by | Behavior |
|---|---|---|
| `interactive` | `ask`/`query`/`retrieve` query embeddings | reserved minimum capacity |
| `foreground` | `axon <source> --wait` | higher than background, yields between batches |
| `background` | watches, scheduled refresh, bulk backfill | bounded + coalesced |
| `maintenance` | reset, reindex, repair | lowest |

Embedding bottleneck rule: source jobs must reserve `embedding` capacity before
creating embedding batches; `ask`/`query`/`retrieve` use a separate
low-latency pool and must **not** wait behind unbounded bulk source embedding.
Vector-write reservations are separate from embedding reservations so Qdrant
optimizer pressure cannot hide behind embedding throughput. LLM calls use `llm`
reservations and cannot consume embedding capacity.

## Embedding-specific capabilities

`model`, `dimensions`, `max_batch_inputs`, `max_batch_tokens`,
`max_in_flight_inputs`, `max_concurrent_requests`, `query_instruction`,
`document_instruction`, `supports_openai_compat`, `supports_native_tei`.

Tune under `[providers.embedding]` (TEI batch/concurrency/in-flight/retries/
cooldowns, interactive-reserved slots) and `[providers.vector]` (Qdrant
upsert batch/write concurrency, RRF hybrid toggle, HNSW ef).

## Rules

Provider clients must have bounded timeouts, redacted errors, retry policy,
cooling behavior where applicable, and structured health diagnostics. Every
provider boundary is fakeable for tests.

If the provider surface changes, update this file,
`crates/axon-api/src/schema_registry.rs`, and regenerate
`provider-capabilities.schema.json` in the same PR.
