# Redaction Contract
Last Modified: 2026-06-30

## Contract

Redaction is a shared runtime boundary owned by `axon-core` and enforced by
security/auth surfaces. Redaction happens before content or metadata leaves a
trust boundary: logs, job events, artifacts, vector payloads, graph evidence,
memory records, CLI JSON, MCP responses, REST responses, and traces.

Redaction failure fails closed.

## Public Boundary

```rust
pub trait Redactor: Send + Sync {
    fn redact_text(&self, input: &str, context: RedactionContext) -> RedactionResult<String>;
    fn redact_json(&self, input: serde_json::Value, context: RedactionContext)
        -> RedactionResult<serde_json::Value>;
    fn classify_field(&self, field: &str, value: &serde_json::Value) -> Visibility;
}

pub struct RedactionContext {
    pub visibility_ceiling: Visibility,
    pub surface: RedactionSurface,
    pub source_kind: Option<SourceKind>,
    pub allow_internal_paths: bool,
}

pub struct RedactionReport {
    pub status: RedactionStatus,
    pub redacted_fields: Vec<String>,
    pub dropped_fields: Vec<String>,
    pub detectors_triggered: Vec<String>,
}
```

## Surfaces

| Surface | Rule |
|---|---|
| logs/traces | redact secrets and local sensitive paths |
| job events | redact by caller visibility ceiling |
| vector payloads | public/redacted metadata only |
| graph evidence | public/internal by evidence class, never secrets |
| memory records | memory-specific visibility and decay policy |
| artifacts | artifact visibility gates access; sensitive artifacts are not inlined |
| CLI JSON | same as REST for untrusted mode |
| MCP responses | same as REST read/write scope visibility |

## Detectors

Minimum detectors:

- bearer tokens
- API keys
- OAuth client secrets
- cookies
- authorization headers
- private keys
- password-bearing URLs
- `.env` key/value secrets
- GitHub/GitLab/Gitea tokens
- Reddit credentials
- OpenAI-compatible API keys
- local credential-store paths, including Codex, Gemini, browser, SSH, cloud,
  and provider SDK homes

Credential identifiers such as OAuth client ids are not cryptographic secrets,
but they are still credential metadata. Public surfaces redact them unless a
contract explicitly marks the field public.

## Metadata Classification

Every metadata field is one of:

- `public`
- `internal`
- `sensitive`
- `derived`
- `redacted`

Unknown adapter metadata defaults to `internal`. Unknown fields never become
public just because they are present in `metadata`.

## Redaction Status

Every public payload write records:

- `redaction_status`
- `redaction_version`
- `visibility`
- `redacted_fields` count

If a payload cannot be safely redacted, the write is blocked and the job becomes
degraded or failed according to stage policy.

## Testing Requirements

- each detector has positive/negative fixtures
- redaction is applied before vector writes
- redaction is applied before job event visibility
- unknown metadata defaults non-public
- failure blocks public payload writes
- same input/context produces deterministic output
