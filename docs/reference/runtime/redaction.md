# Redaction

Last Modified: 2026-07-19

Redaction prevents secrets and sensitive local data from leaking into logs,
events, artifacts, vector payloads, graph evidence, memory records, generated
docs, or user-visible errors. Redaction failure **fails closed**.

> Contract source:
> [`docs/pipeline-unification/runtime/redaction-contract.md`](../../pipeline-unification/runtime/redaction-contract.md).
> Owner: [`crates/axon-core/src/redact`](../../../crates/axon-core/) — the
> `Redactor` trait + `redact_secrets`. `axon-observe`'s `LogFieldSet` redacts
> `message` through `redact_secrets` at construction.

## When redaction runs

Before content/metadata leaves any trust boundary: logs, job events, artifacts,
vector payloads, graph evidence, memory records, CLI JSON, MCP responses, REST
responses, traces. If a payload cannot be safely redacted, the write is
**blocked** and the job degrades or fails per stage policy. Redaction failure
is a not-retryable-without-mutation job error.

## The `Redactor` trait

```rust
trait Redactor {
    fn redact_text(&self, s: &str, ctx: RedactionContext) -> RedactionResult<String>;
    fn redact_json(&self, v: Value, ctx: RedactionContext) -> RedactionResult<Value>;
    fn classify_field(&self, name: &str, v: &Value) -> Visibility;
}
```

`RedactionContext` carries `visibility_ceiling: Visibility`,
`surface: RedactionSurface`, `source_kind: Option<SourceKind>`,
`allow_internal_paths: bool`.

> **Known gap (C1-V01, 2026-07-09):** `axon-web` hardcodes
> `visibility_ceiling: Visibility::Internal` for every caller because no
> `VisibilityPolicy` type exists yet (tracked as C1-16 in `axon-authz`). Do not
> resolve until `axon-authz` exposes a real `VisibilityPolicy`.

## Detectors

Structured parsers (JSON/TOML/YAML/env/URL) run before regex fallback.
High-entropy is a **secondary** signal with key/path context — entropy alone
never classifies a field.

| Detector | Triggers on |
|---|---|
| Bearer tokens | `authorization: bearer <token>` (case-insensitive) |
| API keys (key-name) | `api_key`/`apikey`/`token`/`secret`/`password`/`client_secret`/`private_key` |
| OAuth client secrets | key-name + high-entropy |
| Cookies | `cookie`/`set-cookie` |
| Private keys | PEM `-----BEGIN … PRIVATE KEY-----` |
| `.env` secrets | dotenv `KEY=value` parsing |
| GitHub tokens | `ghp_`/`gho_`/`ghu_`/`ghs_`/`ghr_`/`github_pat_` |
| GitLab tokens | `glpat-` + high-entropy |
| Gitea tokens | — |
| Reddit credentials | `REDDIT_CLIENT_SECRET`/refresh/access/bearer |
| OpenAI-compatible keys | `sk-`/`sk-proj-`/configured names |
| URL userinfo | non-empty username/password authority |
| Secret query params | `?token=…`, `?api_key=…` |
| Local credential paths | Codex/Gemini/browser/SSH/cloud/provider homes under a home dir |

## Visibility classes (5)

`public`, `internal`, `sensitive`, `derived`, `redacted`. Unknown adapter
metadata defaults to `internal`. Unknown fields **never** become public just
because they are present.

## `RedactionStatus`

`clean` / `redacted` / `failed`. Every public payload write records
`redaction_status`, `redaction_version`, `visibility`, and `redacted_fields`
count.

`RedactionReport` fields: `status`, `redacted_fields: Vec<String>`,
`dropped_fields: Vec<String>`, `detectors_triggered: Vec<String>`.

## Surfaces (8)

logs/traces · job events · vector payloads (public/redacted metadata only) ·
graph evidence (public/internal by class, never secrets) · memory records
(memory-specific visibility + decay) · artifacts (visibility gates access;
sensitive not inlined) · CLI JSON (same as REST for untrusted callers) · MCP
responses (same as REST read/write scope).

## Scope

Redaction applies to config and environment values, HTTP headers and tokens,
local paths and secret-looking filenames, provider errors, and tool/MCP source
metadata. Every logging and diagnostic path must use the shared redaction
helpers instead of ad hoc string formatting for sensitive values.

If the redactor changes, update this file and
[`crates/axon-core/src/redact`](../../../crates/axon-core/) in the same PR.
