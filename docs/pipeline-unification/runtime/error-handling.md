# Error Handling Contract
Last Modified: 2026-06-30

## Contract

`axon-error` owns the typed error taxonomy. `axon-api` owns transport-neutral
error envelopes/projections. CLI, MCP, REST, jobs, logs, stores, providers, and
domain crates all use `axon-error` rather than creating local error taxonomies.

This is the target error contract. Current REST/MCP/CLI errors are narrower and
not yet one shared typed taxonomy.

Errors, warnings, degradation, retry, cooling, removed-command behavior, and
redaction failures use one structured model across CLI, MCP, REST, jobs,
progress events, logs, SourceLedger, SourceGraph, DocumentStatus, and provider
status.

Errors are not prose. Every error has a stable code, stage, retry policy,
severity, visibility, and redacted details.

## Design Rules

- Validate before side effects when possible.
- Construct errors through `axon-error` helpers so codes, stages, retry policy,
  severity, and visibility stay consistent.
- Fail fast on invalid action/command/scope/auth.
- Degrade only when the missing capability is optional or a fallback preserves
  the contract.
- Never publish a generation that failed required steps.
- Never write public/vector output after redaction failure.
- Store item-level failures without failing the whole source when policy allows.
- Record cleanup failures as cleanup debt.
- Removed commands/actions/routes are absent from public schemas and never
  dispatch. If a stale removed spelling reaches a parser/router, it is treated
  as an unknown command/action/route and performs no side effects.
- Provider failures enter cooling/backoff before hot loops.

## Crate Ownership

| Crate | Owns |
|---|---|
| `axon-error` | `ApiError`, stable codes, error stage/severity/retry/degradation/cooling taxonomy |
| `axon-api` | serializable `ErrorEnvelope` and schema projection |
| `axon-observe` | error event/log/tracing projection |
| `axon-services` | use-case error mapping at orchestration boundaries |
| transports | rendering only; no private error taxonomy |

## Current Implementation Snapshot

Implemented today:

- REST errors currently use a body shaped around `kind`, `message`, and
  optional `diagnostics`.
- MCP errors use RMCP error data and the current MCP envelope; some internal
  errors may carry structured diagnostics, but not the full target shape.
- CLI errors are still command-specific in many places.
- Code-search records cleanup debt when generation-fenced vector cleanup fails.

Planned by this contract:

- Every transport returns or renders the target `ApiError` shape with stable
  `code`, `stage`, `retryable`, `severity`, `visibility`, redacted `details`,
  and correlation ids.
- Removed commands/actions/routes are absent from public schemas and cannot
  dispatch; stale callers receive unknown-command/action/route errors, not
  compatibility aliases.
- Provider cooling/backoff is standardized and visible in progress/status.

## Error Shape

```json
{
  "code": "provider.unavailable",
  "message": "Embedding provider is unavailable.",
  "stage": "embedding",
  "retryable": true,
  "severity": "failed",
  "visibility": "public",
  "details": {
    "provider": "tei",
    "cooldown_until": "2026-06-30T20:25:00Z"
  },
  "source_id": "src_...",
  "source_item_key": "src/lib.rs",
  "document_id": "doc_...",
  "job_id": "job_..."
}
```

Required fields:

| Field | Meaning |
|---|---|
| `code` | Stable machine code. |
| `message` | Redacted human-readable message. |
| `stage` | Pipeline/transport stage. |
| `retryable` | Whether retry may succeed. |
| `severity` | `info`, `warning`, `degraded`, `failed`, `fatal`. |
| `visibility` | `public`, `internal`, `sensitive`. |
| `details` | Redacted structured context. |

Optional fields:

| Field | Meaning |
|---|---|
| `job_id` | Job correlation id. |
| `source_id` | Source id. |
| `source_item_key` | Item/file/page key. |
| `document_id` | Document id. |
| `chunk_id` | Chunk id. |
| `provider_id` | Provider that failed. |
| `retry_after_ms` | Suggested retry delay. |
| `cooldown_until` | Provider/job cooling timestamp. |

## Stage Values

| Stage | Examples |
|---|---|
| `parsing` | CLI/MCP/REST request parse, removed command/action |
| `validation` | missing fields, bad types, unsupported flags |
| `resolving` | source resolution, canonical URI, authority |
| `routing` | adapter/scope/provider selection |
| `authorizing` | auth, credentials, execution policy |
| `planning` | source plan, prune plan |
| `leasing` | job/watch/source lease |
| `discovering` | manifest/map discovery |
| `diffing` | manifest diff |
| `fetching` | HTTP/git/package/local/MCP/CLI fetch |
| `rendering` | browser/CDP/render provider |
| `normalizing` | SourceDocument creation |
| `parsing_content` | parser facts/chunk parser |
| `graphing` | graph writes/merge/conflict |
| `preparing` | chunking/PreparedDocument |
| `embedding` | embedding provider/batch |
| `upserting` | VectorStore writes |
| `publishing` | generation publish |
| `cleaning` | cleanup/prune/dedupe |
| `retrieving` | query/retrieve context |
| `synthesizing` | LLM synthesis |
| `observing` | progress/log/status emit |

## Error Categories

| Prefix | Meaning | Retry |
|---|---|---|
| `command.*` | CLI command parse/removed/validation | no |
| `action.*` | MCP action parse/removed/validation | no |
| `route.*` | REST route removed/validation | no |
| `auth.*` | authz/authn/credential failure | depends |
| `source.resolve.*` | source resolution/canonicalization | depends |
| `source.scope.*` | unsupported scope/adapter option | no |
| `source.acquire.*` | fetch/discover/acquisition failure | yes |
| `ledger.*` | ledger transaction/lease/generation failure | yes |
| `parser.*` | parse/chunk/fact extraction failure | degrade/depends |
| `graph.*` | graph write/merge failure | degrade/depends |
| `embedding.*` | embedding provider/batch failure | yes |
| `vector.*` | VectorStore write/search/delete failure | yes |
| `artifact.*` | artifact read/write/retention failure | yes |
| `provider.*` | provider health/capability/rate-limit | yes |
| `redaction.*` | secret/sensitive data handling failure | no/fatal |
| `output.*` | response too large/write failure | depends |
| `prune.*` | cleanup/purge/dedupe failure | yes |

## Severity Semantics

| Severity | Terminal | Meaning |
|---|---:|---|
| `info` | no | informational event |
| `warning` | no | non-fatal issue, full behavior preserved |
| `degraded` | maybe | behavior reduced but acceptable by policy |
| `failed` | yes for item/job depending scope | required work failed |
| `fatal` | yes | cannot continue safely |

## Degraded vs Failed

Degrade when:

- optional graph extraction fails
- optional brand/screenshot/artifact capture fails
- parser falls back but chunks remain citable
- one optional provider has a configured fallback
- some source items fail but source policy allows partial publish
- cleanup after publish fails and cleanup debt is recorded

Fail when:

- request cannot be parsed or validated
- source cannot be resolved
- requested adapter/scope is unsupported
- auth/credential policy denies access
- redaction fails before public/vector output
- required provider has no fallback
- embedding fails for required publish
- generation cannot be published safely
- ledger transaction integrity is unknown

## Retry and Cooling

Retry policy fields:

| Field | Meaning |
|---|---|
| `retryable` | whether retry may succeed |
| `retry_after_ms` | minimum delay |
| `attempt` | current attempt number |
| `max_attempts` | configured max |
| `backoff_ms` | next backoff |
| `cooldown_until` | provider/source cooling window |
| `retry_scope` | item, document, phase, job, provider |

Provider cooling:

- starts after repeated provider failures or rate limits
- emits progress/status event
- prevents tight retry loops
- is visible in provider status/capabilities
- can be bypassed only by explicit admin/debug option

Retry rules:

- parse/validation/unsupported scope errors are not retryable
- transient network/provider failures are retryable
- redaction failures are not retryable without changed input/policy
- cleanup failures are retryable and recorded as cleanup debt
- idempotency keys prevent duplicate job side effects

## Removed Surfaces

Removed commands, actions, and routes are absent from the final parser, schema,
OpenAPI document, help output, and generated clients. They do not have a
special public error shape. If a stale caller sends one anyway, it receives the
normal unknown-command, unknown-action, or not-found error and no side effects
occur.

## Source Item Errors

Item-level failures are attached to ledger/source item state.

Required fields:

- `source_id`
- `source_item_key`
- `generation`
- `status`
- `error_code`
- `error_stage`
- `retryable`
- `attempt`
- redacted details

Item errors do not automatically fail the whole job. The source policy decides
whether partial publish is allowed.

## Redaction Failures

Redaction is a safety boundary.

Rules:

- If sensitive content might enter public output or vectors and cannot be
  confidently redacted, fail before write.
- Do not log the offending content.
- Store only redaction class, count, location type, and policy id.
- Public error should say content could not be safely processed.
- Internal error may include hashes and classifier details, not raw secret.

## Local Path Safety

Local paths are sensitive by default.

Rules:

- public errors should use `local_project_key` and relative path
- internal logs may include absolute path only under local/admin policy
- vector payloads use relative path/path hash
- auth errors must not reveal private path existence to remote callers

## Transport Mapping

| Transport | Error Surface |
|---|---|
| CLI human | concise redacted message, remediation, exit code |
| CLI JSON | strict error envelope |
| MCP | MCP envelope with structured error |
| REST | HTTP status plus error envelope |
| jobs/events | progress event with `severity=error/fatal` |
| logs | structured log with `error_code` |
| metrics | `axon_errors_total` with bounded labels |

HTTP mapping:

| HTTP | Error Class |
|---:|---|
| 400 | validation/parse |
| 401 | unauthenticated |
| 403 | unauthorized |
| 404 | source/document/artifact not visible/found |
| 409 | lease/conflict/idempotency mismatch |
| 410 | removed route |
| 422 | unsupported source/scope/semantic validation |
| 429 | rate limit/cooling |
| 500 | internal invariant failure |
| 502 | provider upstream failure |
| 503 | dependency unavailable |

## Validation Checklist

Implementation is incomplete until:

- every public error uses the structured shape
- every error has stable code/stage/severity/retryable
- removed surfaces are absent from public schemas and do not dispatch
- provider failures cool down instead of retry looping
- redaction failures fail safely
- cleanup failures create cleanup debt
- item-level errors are stored on SourceLedger items
- transport mappings are deterministic
- logs/metrics include error codes
- public errors contain no secrets
