# Provider Contract
Last Modified: 2026-06-30

## Contract

This is the target provider boundary. Current providers are concrete dispatch
paths and clients in the existing crates: TEI/OpenAI-compatible embedding,
Qdrant vector operations, Gemini/OpenAI-compatible/Codex LLM synthesis,
SearXNG/Tavily search, reqwest/git/registry fetch, and Chrome/CDP rendering.
They do not yet share one capability registry or reservation model.

Providers are bounded capabilities with health, limits, reservations, errors,
cooling, and fakes. Provider code performs one class of work; it does not own
global job scheduling or cross-pipeline fairness.

This contract exists especially to avoid overloading `EmbeddingProvider` during
bulk indexing, watch refreshes, and interactive retrieval.

## Design Rules

- Every provider reports capabilities and current health.
- Every provider has structured errors.
- Every provider has a fake for tests.
- The scheduler reserves capacity before provider calls.
- Interactive work has priority lanes.
- Provider cooldown is global across jobs.
- Provider config is explicit and small.
- Provider implementation does not own transport rendering, ledger publishing,
  or vector payload policy unless that is its boundary.

## Provider Classes

| Provider | Boundary | Examples |
|---|---|---|
| `EmbeddingProvider` | text/code/query embeddings | TEI, OpenAI-compatible, fake |
| `VectorStore` | vector write/read/delete | Qdrant, fake |
| `LlmProvider` | synthesis/extraction/judging/chat | Gemini CLI, OpenAI-compatible, Codex app-server, fake |
| `SearchProvider` | external search | SearXNG, Tavily, fake |
| `FetchProvider` | HTTP/git/registry/social fetch | reqwest, git, registry clients, fake |
| `RenderProvider` | browser render/screenshots/endpoints | Chrome/CDP, fake |
| `ArtifactStore` | large/binary output storage | filesystem, object store later, fake |
| `LedgerStore` | source lifecycle state | SQLite, fake |
| `GraphStore` | graph nodes/edges/evidence | SQLite, fake |
| `MemoryStore` | memory lifecycle | SQLite/vector hybrid, fake |
| `JobStore` | jobs/events/heartbeats | SQLite, fake |

## Capability Document

Every provider reports:

| Field | Meaning |
|---|---|
| `provider_id` | Stable provider instance id. |
| `provider_kind` | Boundary class. |
| `implementation` | `qdrant`, `tei`, `gemini-headless`, etc. |
| `version` | Provider/client version when available. |
| `models` | Supported models where relevant. |
| `dimensions` | Embedding dimensions where relevant. |
| `limits` | Concurrency, batch, bytes, token, timeout limits. |
| `features` | Sparse vectors, streaming, screenshots, etc. |
| `health` | `healthy`, `degraded`, `unavailable`, `unknown`. |
| `cooldown_until` | If cooled. |
| `last_error` | Redacted structured error. |

## Reservation Model

Provider calls require reservations from the job scheduler.

Reservation fields:

| Field | Meaning |
|---|---|
| `reservation_id` | Unique id. |
| `job_id` | Owning job. |
| `stage_id` | Owning stage. |
| `provider_kind` | Capacity class. |
| `priority` | `interactive`, `normal`, `background`, `maintenance`. |
| `units` | Abstract capacity units. |
| `estimated_inputs` | Items/tokens/bytes when known. |
| `deadline` | Optional latest start/finish. |
| `granted_at` | Grant time. |
| `expires_at` | Lease expiry. |
| `state` | `requested`, `queued`, `granted`, `active`, `released`, `expired`, `canceled`, or `failed`. |

Providers reject calls without reservations except for health checks and tiny
local fake tests.

Reservation lifecycle:

```text
requested -> queued -> granted -> active -> released
requested -> queued -> canceled
granted -> expired
active -> failed
active -> expired
```

Recovery rules:

- queued reservations are released when the owning job is canceled or expires
- granted reservations expire if a stage never starts before `expires_at`
- active reservations heartbeat through the owning job heartbeat
- stale active reservations are reclaimed only after heartbeat grace and lease
  expiry
- provider cooldown cancels queued background reservations but preserves
  already-active requests until their request timeout or cancellation fires
- scheduler metrics expose queue depth and wait time by provider kind, priority,
  and job kind

## EmbeddingProvider Contract

`EmbeddingProvider` owns:

- model selection within configured provider
- document/query embedding request formatting
- batch request execution
- provider-specific retryable error classification
- dimensions and pooling metadata

`EmbeddingProvider` does not own:

- global concurrency
- cross-job fairness
- watch throttling
- vector payload fields
- Qdrant writes
- source ledger publishing
- retry loops across jobs

Embedding capabilities:

| Field | Meaning |
|---|---|
| `model` | Active embedding model. |
| `dimensions` | Output dimensions. |
| `max_batch_inputs` | Max inputs per request. |
| `max_batch_tokens` | Max tokens per request when known. |
| `max_in_flight_inputs` | Scheduler cap across jobs. |
| `max_concurrent_requests` | Scheduler cap across jobs. |
| `query_instruction` | Whether query text receives an instruction prefix. |
| `document_instruction` | Whether document text receives an instruction prefix. |
| `supports_openai_compat` | OpenAI embeddings compatibility. |
| `supports_native_tei` | Native TEI `/embed` support. |

Embedding priority lanes:

| Lane | Used By | Rule |
|---|---|---|
| `interactive` | `ask`, `query`, `retrieve` query embeddings | Reserved minimum capacity. |
| `foreground` | `axon <source> --wait` | Higher priority than background, yields between batches. |
| `background` | watches, scheduled refresh, bulk backfill | Bounded and coalesced. |
| `maintenance` | reset, reindex, repair | Lowest priority. |

## VectorStore Contract

`VectorStore` owns:

- collection capability reporting
- upsert/delete/search
- filter schema/index compatibility
- named/unnamed vector handling
- sparse/dense/hybrid support
- write/read/delete error classification

It does not own source freshness, generation publish, or document preparation.

Vector write reservations are separate from embedding reservations so Qdrant
optimizer pressure cannot be hidden behind embedding throughput.

## LlmProvider Contract

`LlmProvider` owns:

- completion/chat/extraction/judging request execution
- streaming deltas
- model/context capability reporting
- JSON/schema validation support signals
- backend-specific auth/runtime setup

LLM calls use `llm` reservations and cannot consume embedding capacity.

## Health and Cooling

Health states:

| State | Meaning |
|---|---|
| `healthy` | Recent probe succeeded. |
| `degraded` | Optional feature unavailable or slow. |
| `unavailable` | Required calls failing. |
| `cooling` | Temporarily blocked after repeated failures. |
| `unknown` | Not probed. |

Cooling rules:

- repeated retryable provider failures enter cooldown
- cooldown blocks new reservations
- existing safe cleanup/finalization may continue
- health probe can end cooldown early
- cooldown status is visible in jobs/status/doctor/capabilities

Cooling state fields:

| Field | Meaning |
|---|---|
| `failure_count` | Consecutive failures counted toward cooling. |
| `cooldown_reason` | `rate_limit`, `timeout`, `unavailable`, `resource_exhausted`, etc. |
| `cooldown_started_at` | First time provider entered current cooldown. |
| `cooldown_until` | Earliest time background reservations may resume. |
| `probe_after` | Earliest health probe time. |
| `last_success_at` | Last successful provider call. |
| `last_error` | Redacted structured provider error. |

## Configuration Surface

Provider config should expose useful knobs only:

- endpoint URL and secrets in `.env`
- model names in `config.toml` unless secret/runtime-specific
- concurrency, batch size, timeout, retry, cooldown in `config.toml`
- compose-only GPU/image/port variables in `.env` only when Docker needs them

No provider should require a large kitchen-sink env file to boot.

## Testing Requirements

Provider tests must cover:

- capability document
- healthy probe
- unavailable probe
- timeout
- rate limit
- retryable vs fatal error
- cooldown
- reservation required
- fake deterministic output
- interactive lane not starved by background lane
