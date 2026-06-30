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

| Detector | Required Pattern/Library Behavior |
|---|---|
| bearer tokens | case-insensitive `authorization: bearer <token>` header/value detection |
| API keys | key-name detector for `api_key`, `apikey`, `token`, `secret`, `password`, `client_secret`, `private_key` in JSON/TOML/YAML/env |
| OAuth client secrets | key-name detector plus high-entropy value classification |
| cookies | `cookie`/`set-cookie` header and semicolon-delimited cookie value detection |
| private keys | PEM blocks beginning `-----BEGIN ... PRIVATE KEY-----` |
| password URLs | URL parser detection of non-empty username/password authority parts |
| `.env` secrets | dotenv-style `KEY=value` parsing with secret-key classification |
| GitHub tokens | `ghp_`, `gho_`, `ghu_`, `ghs_`, `ghr_`, and fine-grained `github_pat_` prefixes |
| GitLab tokens | `glpat-` and deploy-token style high-entropy values when key context matches GitLab |
| Gitea tokens | token key context plus high-entropy value classification |
| Reddit credentials | `REDDIT_CLIENT_SECRET`, refresh/access token fields, and OAuth bearer fields |
| OpenAI-compatible keys | `sk-`, `sk-proj-`, and configured OpenAI-compatible key names |
| local credential paths | path detector for Codex, Gemini, browser profiles, SSH, cloud config, provider SDK homes, and token stores under a home directory |

Implementation libraries:

- use structured parsers for JSON/TOML/YAML/env/url inputs before regex fallback
- use compiled `regex`/`regex-set` style detectors for token patterns
- use entropy checks only as a secondary signal with key/path context
- never classify a field as public solely because no detector matched it

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
