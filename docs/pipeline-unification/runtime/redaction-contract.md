# Redaction Contract
Last Modified: 2026-08-21

## Contract

Redaction is a shared runtime boundary owned by `axon-core` and enforced by
security/auth surfaces. Redaction happens before content or metadata leaves a
trust boundary: logs, job events, artifacts, vector payloads, graph evidence,
memory records, CLI JSON, MCP responses, REST responses, and traces.

Redaction failure fails closed.

## Public Boundary

```rust
pub trait Redactor: Send + Sync {
    fn redact_text(&self, input: &str, context: &RedactionContext) -> String;
    fn redact_json(
        &self,
        input: serde_json::Value,
        context: &RedactionContext,
    ) -> (serde_json::Value, RedactionReport);
    fn classify_field(&self, field: &str, value: &serde_json::Value) -> Visibility;
}

pub struct RedactionContext {
    pub visibility_ceiling: Visibility,
    pub surface: RedactionSurface,
    pub source_kind: Option<SourceKind>,
    pub allow_internal_paths: bool,
}
```

**C1-V01 (2026-07-09 audit) — resolved for `axon-web`, reopened against
`axon-mcp`, 2026-07-24 review:** The chicken-and-egg dependency this note
originally described is gone. `axon-authz` now exposes a real
`VisibilityPolicy` with `ceiling_for` (`crates/axon-authz/src/visibility.rs`,
~lines 20-66), and `axon-web` calls it instead of hardcoding a ceiling, at
`handlers/sources.rs:228`, `handlers/source_watch.rs:312`,
`handlers/memory_routes.rs:58`, and `handlers/async_jobs.rs:97`.

The gap is not closed everywhere, though: `axon-mcp` still hardcodes
`visibility_ceiling: Visibility::Internal` for every caller, at
`crates/axon-mcp/src/server.rs:378-398` and
`crates/axon-mcp/src/server/tasks.rs:218`. Those two sites do not call
`VisibilityPolicy`, so MCP callers currently get the same
does-not-reflect-real-caller-policy behavior `axon-web` used to have. C1-V01
is reopened against `axon-mcp` specifically; wiring those two sites to
`VisibilityPolicy::ceiling_for` is a separate, tracked follow-up, not part of
this doc pass.

```rust

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
| bearer tokens | case-insensitive Authorization/Bearer detection when the value is a known token family or credibly opaque token; short documentation examples are preserved |
| API keys | semantic key-name detection for credential-bearing names; descriptive names such as `token_count` or `password_policy` are not sufficient evidence |
| OAuth client secrets | credential-shaped key context plus concrete value evidence; entropy is secondary, never context-free |
| cookies | `cookie`/`set-cookie` values are sensitive when session/auth/CSRF/token semantics or credible opaque credential values are present; ordinary preference cookies are preserved |
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
- use entropy checks only as a secondary signal with credential-shaped key/path context
- treat security vocabulary as context, not proof; descriptive and pagination fields remain non-secret unless their values independently carry credential evidence
- redact identifiable secret and host-path spans in place on public/log free-text surfaces; replace a complete value only when residual secret material cannot be safely isolated
- signed URLs redact credentials, signatures, security tokens, and auth keys while retaining benign signing dates/expiry and pagination metadata
- operational egress is deliberately more conservative than retrievable bodies: concrete Authorization values are scrubbed even when short, while generic documentation can preserve illustrative examples
- retrievable `chunk_text` uses high-confidence detection before vectorization; concrete secret-bearing chunks are omitted, while low-confidence tutorial syntax remains searchable
- normalize field/query names across snake_case, kebab-case, and camelCase, with explicit descriptive and pagination exemptions
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

If the redaction engine cannot safely inspect or transform a public payload, the
write is blocked and the job becomes degraded or failed according to stage policy.

Vector payload construction is fail-closed for the affected content, not the
entire unrelated corpus. A chunk positively classified as containing a forbidden
secret-bearing body value is never embedded or upserted. The vectorizer omits that
chunk, continues with safe siblings, records the actual point count, and emits an
attributable `source.vectorize.redaction_skipped_chunks` warning. Structural
payload violations, detector failures, panics, oversized uninspectable public
writes, or any condition where write safety cannot be proven still block the
affected write or stage. Secret-bearing source content is never made indexable by
replacing retrievable `chunk_text` with a redacted body.

## Testing Requirements

- each detector has positive/negative fixtures
- redaction is applied before vector writes
- redaction is applied before job event visibility
- unknown metadata defaults non-public
- redaction-engine failure blocks public payload writes
- a secret-bearing vector chunk is omitted before embedding/upsert, safe siblings continue, and the omission is surfaced through an attributable warning and actual point count
- negative fixtures cover security vocabulary, pagination tokens, preference cookies, short credential examples, high-entropy identifiers, benign query parameters, and ordinary URL/email prose
- same input/context produces deterministic output
