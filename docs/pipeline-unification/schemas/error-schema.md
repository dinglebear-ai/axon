# Error Schema Contract
Last Modified: 2026-06-30

## Contract

`axon-error` owns error schemas: codes, stages, severity, visibility, retry
policy, degradation policy, and provider cooling context.

## Generated Artifacts

```text
docs/reference/api/errors.schema.json
docs/reference/api/errors.md
```

Generator:

```bash
cargo xtask schemas errors
cargo xtask schemas errors --check
```

## Source Inputs

The error schema generator reads:

```text
crates/axon-error/src/**
crates/axon-api/src/envelope.rs
crates/axon-web/src/error*.rs
crates/axon-mcp/src/error*.rs
crates/axon-cli/src/error*.rs
docs/pipeline-unification/runtime/error-handling.md
```

The generated artifact records these paths in `x-axon.source_inputs`.

## Required Schemas

- `ApiError`
- `ErrorEnvelope`
- `ErrorCode`
- `ErrorStage`
- `RetryPolicy`
- `DegradationPolicy`
- `ProviderCooling`
- `ErrorContext`

## ApiError Shape

```json
{
  "type": "object",
  "required": [
    "code",
    "message",
    "stage",
    "retryable",
    "severity",
    "visibility",
    "details"
  ],
  "properties": {
    "code": { "type": "string", "pattern": "^[a-z0-9_.]+$" },
    "message": { "type": "string" },
    "stage": { "$ref": "#/$defs/ErrorStage" },
    "retryable": { "type": "boolean" },
    "severity": { "$ref": "#/$defs/ErrorSeverity" },
    "visibility": { "$ref": "#/$defs/Visibility" },
    "details": { "type": "object", "additionalProperties": true },
    "job_id": { "type": "string" },
    "source_id": { "type": "string" },
    "source_item_key": { "type": "string" },
    "document_id": { "type": "string" },
    "provider_id": { "type": "string" },
    "retry_after_ms": { "type": "integer", "minimum": 0 },
    "cooldown_until": { "type": "string", "format": "date-time" }
  },
  "additionalProperties": false
}
```

## Error Registry Shape

`docs/reference/api/errors.md` is generated from a machine registry where every
error code has:

- `code`
- `stage`
- `severity`
- `retryable_default`
- `visibility_default`
- `message_template`
- `details_schema`
- `owner_crate`
- `test_fixture`

## Required Error Stage Values

```text
parsing
validation
resolving
routing
authorizing
planning
leasing
discovering
diffing
fetching
rendering
normalizing
parsing_content
graphing
preparing
embedding
upserting
publishing
cleaning
retrieving
synthesizing
observing
storage
provider
transport
internal
```

## Required Error Code Families

The registry must include at least one concrete code for every family:

| Family | Examples |
|---|---|
| `command.*` | `command.unknown`, `command.invalid_args` |
| `action.*` | `action.unknown`, `action.invalid_subaction` |
| `route.*` | `route.not_found`, `route.method_not_allowed` |
| `auth.*` | `auth.missing`, `auth.forbidden`, `auth.scope_required` |
| `source.resolve.*` | `source.resolve.unsupported`, `source.resolve.ambiguous` |
| `source.acquire.*` | `source.acquire.fetch_failed`, `source.acquire.not_found` |
| `ledger.*` | `ledger.lease_conflict`, `ledger.publish_failed` |
| `parser.*` | `parser.unsupported`, `parser.malformed` |
| `graph.*` | `graph.write_failed`, `graph.conflict` |
| `embedding.*` | `embedding.provider_unavailable`, `embedding.batch_failed` |
| `vector.*` | `vector.upsert_failed`, `vector.delete_failed` |
| `artifact.*` | `artifact.write_failed`, `artifact.not_found` |
| `provider.*` | `provider.unavailable`, `provider.rate_limited`, `provider.cooling` |
| `redaction.*` | `redaction.failed`, `redaction.secret_detected` |
| `prune.*` | `prune.plan_failed`, `prune.exec_failed` |

## Generated Markdown Registry

`docs/reference/api/errors.md` contains:

- error code
- message template
- stage
- retryable default
- severity
- visibility
- HTTP status mapping
- MCP behavior
- CLI rendering
- owner crate
- test fixture path

## Rules

- every error has stable `code`
- every error has `stage`, `retryable`, `severity`, and `visibility`
- details are structured and redacted
- retryable errors include retry scope where available
- provider cooling errors include cooldown metadata

## Drift Checks

Fail when:

- error code appears in code but not registry
- registry code has no constructor/test
- transport invents private error shape
- removed action/route/command has special compatibility error
- examples in error-handling docs fail validation

## Validation Fixtures

Required fixtures:

```text
crates/axon-error/tests/fixtures/schema/provider_unavailable.valid.json
crates/axon-error/tests/fixtures/schema/validation_error.valid.json
crates/axon-error/tests/fixtures/schema/redaction_failure.valid.json
crates/axon-error/tests/fixtures/schema/missing_code.invalid.json
crates/axon-error/tests/fixtures/schema/bad_stage.invalid.json
```

## Acceptance Criteria

- every error constructor maps to a registered error code
- every registered error code has fixture coverage
- every transport renders the same error envelope fields
- provider cooling errors include retry/cooling metadata
- redaction failures are fatal or failed according to error policy
- no removed surface gets a bespoke compatibility error
