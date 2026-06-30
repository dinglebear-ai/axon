# Provider Capability Schema Contract
Last Modified: 2026-06-30

## Contract

Provider capability schemas define how providers report features, health,
limits, cooling, reservations, and degradation behavior.

## Generated Artifacts

```text
docs/reference/runtime/provider-capabilities.schema.json
docs/reference/runtime/provider-capabilities.md
```

Generator:

```bash
cargo xtask schemas providers
cargo xtask schemas providers --check
```

## Source Inputs

The provider capability schema generator reads:

```text
crates/axon-embedding/src/provider*.rs
crates/axon-llm/src/provider*.rs
crates/axon-vectors/src/provider*.rs
crates/axon-adapters/src/provider*.rs
crates/axon-core/src/provider*.rs
crates/axon-jobs/src/rate*.rs
crates/axon-api/src/provider.rs
docs/pipeline-unification/runtime/provider-contract.md
```

The generated artifact records these paths in `x-axon.source_inputs`.

## Provider Families

- `LlmProvider`
- `EmbeddingProvider`
- `VectorStore`
- `SearchProvider`
- `FetchProvider`
- `RenderProvider`
- `NetworkCaptureProvider`
- `CredentialProvider`
- `RateLimiter`
- `HealthProbe`
- `SecurityPolicy`
- durable stores that expose health/capability

## Required Fields

Every capability includes:

- `provider_id`
- `provider_kind`
- `implementation`
- `version`
- `health`
- `limits`
- `features`
- `cooldown_until`
- `last_error` redacted
- `reservation_policy`
- `cost_class`
- `degraded_modes`
- `fake_overrides_supported`

## Provider Capability Shape

```json
{
  "type": "object",
  "required": [
    "provider_id",
    "provider_kind",
    "implementation",
    "version",
    "health",
    "limits",
    "features",
    "reservation_policy",
    "degraded_modes"
  ],
  "properties": {
    "provider_id": { "type": "string" },
    "provider_kind": { "$ref": "#/$defs/ProviderKind" },
    "implementation": { "type": "string" },
    "version": { "type": "string" },
    "health": { "$ref": "#/$defs/HealthStatus" },
    "limits": { "$ref": "#/$defs/ProviderLimits" },
    "features": {
      "type": "array",
      "items": { "type": "string" }
    },
    "cooldown_until": { "type": ["string", "null"], "format": "date-time" },
    "last_error": { "$ref": "#/$defs/ApiError" },
    "reservation_policy": { "$ref": "#/$defs/ReservationPolicy" },
    "cost_class": { "$ref": "#/$defs/ProviderCostClass" },
    "degraded_modes": {
      "type": "array",
      "items": { "$ref": "#/$defs/DegradedMode" }
    },
    "fake_overrides_supported": { "type": "boolean" }
  },
  "additionalProperties": false
}
```

## Provider Limits Shape

```json
{
  "type": "object",
  "properties": {
    "max_concurrency": { "type": "integer", "minimum": 1 },
    "max_batch_size": { "type": "integer", "minimum": 1 },
    "max_input_bytes": { "type": "integer", "minimum": 1 },
    "timeout_ms": { "type": "integer", "minimum": 1 },
    "rate_limit_per_minute": { "type": ["integer", "null"], "minimum": 1 },
    "max_queue_depth": { "type": ["integer", "null"], "minimum": 1 },
    "interactive_reserved_concurrency": { "type": ["integer", "null"], "minimum": 0 },
    "background_max_concurrency": { "type": ["integer", "null"], "minimum": 0 },
    "maintenance_max_concurrency": { "type": ["integer", "null"], "minimum": 0 }
  },
  "additionalProperties": false
}

```

## Reservation Policy Shape

```json
{
  "type": "object",
  "required": [
    "supports_reservations",
    "queue_policy",
    "interactive_reserve",
    "cooldown_after_failures",
    "cooldown_secs"
  ],
  "properties": {
    "supports_reservations": { "type": "boolean" },
    "queue_policy": {
      "type": "string",
      "enum": ["fifo", "priority", "fair_by_job", "drop_when_full"]
    },
    "interactive_reserve": { "type": "integer", "minimum": 0 },
    "cooldown_after_failures": { "type": "integer", "minimum": 1 },
    "cooldown_secs": { "type": "integer", "minimum": 1 },
    "retry_backoff_ms": { "type": "integer", "minimum": 0 }
  },
  "additionalProperties": false
}
```

Schedulers must be able to decide whether to admit, queue, reserve, or cool a
provider call from capability fields alone.

## Family-Specific Required Fields

| Provider | Extra Required Fields |
|---|---|
| `EmbeddingProvider` | `model_id`, `dimensions`, `max_input_tokens`, `instruction_support` |
| `LlmProvider` | `model_id`, `context_window`, `streaming`, `json_schema` |
| `VectorStore` | `dense`, `sparse`, `hybrid`, `payload_filters`, `delete_by_filter` |
| `FetchProvider` | `schemes`, `redirect_policy`, `header_policy` |
| `RenderProvider` | `render_modes`, `browser_pool_limits`, `script_support` |
| `CredentialProvider` | `auth_schemes`, `redaction_policy` |

Family-specific schema fragments:

```json
{
  "EmbeddingProviderCapability": {
    "required": [
      "model_id",
      "dimensions",
      "max_input_tokens",
      "max_batch_tokens",
      "instruction_support",
      "sparse_output",
      "batch_limits"
    ]
  },
  "VectorStoreCapability": {
    "required": [
      "dense",
      "sparse",
      "hybrid",
      "payload_filters",
      "payload_indexes",
      "delete_by_filter",
      "collection_aliases",
      "consistency"
    ]
  },
  "LlmProviderCapability": {
    "required": [
      "model_id",
      "context_window",
      "streaming",
      "json_schema",
      "tool_use",
      "structured_output",
      "max_output_tokens"
    ]
  }
}
```

## Provider Registry Shape

Each provider crate exports:

```rust
pub struct ProviderSpec {
    pub provider_id: &'static str,
    pub provider_kind: ProviderKind,
    pub implementation: &'static str,
    pub version: &'static str,
    pub feature_flag: Option<&'static str>,
    pub capability_schema: &'static str,
    pub health_probe: &'static str,
    pub fake_available: bool,
}
```

The generated provider schema is built from all `ProviderSpec` values.

## Fake Provider Contract

Every provider family exposes a fake for tests.

Fake capabilities must allow overriding:

- health status
- latency
- limits
- queue policy
- cooldown state
- failure mode
- dimensions/model id for embeddings
- context window/model id for LLMs
- vector delete/search/upsert behavior

The provider schema generator fails when a provider family lacks a fake or when
the fake cannot produce valid capability JSON.

## Required Provider IDs

Minimum target provider ids:

| Provider ID | Kind | Implementation |
|---|---|---|
| `llm.gemini` | `llm` | Gemini CLI/headless |
| `llm.openai_compat` | `llm` | OpenAI-compatible chat |
| `llm.codex` | `llm` | Codex app-server |
| `embedding.tei` | `embedding` | TEI |
| `embedding.openai_compat` | `embedding` | OpenAI-compatible embeddings |
| `vector.qdrant` | `vector` | Qdrant |
| `search.searxng` | `search` | SearXNG |
| `search.tavily` | `search` | Tavily |
| `fetch.http` | `fetch` | HTTP client |
| `render.chrome` | `render` | Chrome/CDP |
| `credential.env` | `credential` | env/config secret refs |
| `rate_limiter.default` | `rate_limiter` | in-process limiter |
| `security.default` | `security` | SSRF/local/tool policy |

Every provider has a fake provider for tests except external-only adapters that
are represented by a fake at the family boundary.

## Health Report Shape

```json
{
  "provider_id": "embedding.tei",
  "status": "healthy",
  "checked_at": "2026-06-30T20:20:00Z",
  "latency_ms": 12,
  "message": "ready",
  "last_error": null,
  "cooldown_until": null
}
```

Health rules:

- `last_error` is redacted and schema-valid
- unhealthy providers include remediation when known
- cooling providers include `cooldown_until`
- disabled providers include reason and feature flag/config source
- provider checks have declared cost so doctor/status can avoid expensive probes

## Reservation Fields

Capabilities must expose scheduler-relevant fields:

- max concurrent requests
- max batch size
- queue policy
- interactive reserve support
- timeout
- retry/cooling policy
- cost class
- health probe cost

## Generated Capability Document

`GET /v1/providers` and MCP `providers/list` return a document shaped like:

```json
{
  "generated_at": "2026-06-30T20:20:00Z",
  "providers": [
    { "$ref": "#/$defs/ProviderCapability" }
  ],
  "families": {
    "embedding": {
      "default_provider_id": "embedding.tei",
      "available": ["embedding.tei"],
      "degraded": []
    }
  },
  "scheduler": {
    "global_in_flight_limit": 320,
    "interactive_reserved_requests": 1,
    "background_max_requests": 3
  }
}
```

This document is generated from the same registry used by runtime provider
selection.

## Drift Checks

Fail when:

- provider implementation has no capability schema
- capability schema omits health or limits
- provider status route emits fields absent from schema
- fake provider cannot override capability fields
- provider docs differ from registry
- scheduler cannot compute reservations from capability fields
- fake provider capability differs structurally from real provider capability
- provider emits unredacted secret or endpoint credential

## Validation Fixtures

Required fixtures:

```text
crates/axon-embedding/tests/fixtures/capability/tei.valid.json
crates/axon-llm/tests/fixtures/capability/gemini.valid.json
crates/axon-vectors/tests/fixtures/capability/qdrant.valid.json
crates/axon-adapters/tests/fixtures/capability/search.valid.json
crates/axon-jobs/tests/fixtures/capability/rate_limiter.valid.json
crates/axon-embedding/tests/fixtures/capability/missing_limits.invalid.json
```

## Acceptance Criteria

- every provider implementation exposes `ProviderSpec`
- every provider capability validates against the family schema
- provider health output validates against health schema
- fake providers can override every scheduler-relevant field
- provider docs are generated from the same registry as runtime capabilities
- background reservations can be computed from capability fields alone
- provider capability docs, REST status, MCP capabilities, and scheduler inputs
  all use the same schema
