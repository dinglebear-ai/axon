# Security Contract
Last Modified: 2026-06-30

## Contract

Source unification increases the number of things Axon can touch: URLs, local
paths, Git repos, registries, session exports, CLI tools, MCP tools, browser
rendering, screenshots, artifacts, memories, and graph links. Security policy is
a first-class pipeline boundary, not transport glue.

## Design Rules

- Validate authorization and execution policy before side effects.
- Secrets are never stored in vector payloads, graph evidence, public artifacts,
  CLI JSON, MCP responses, REST responses, or unredacted logs.
- Local filesystem access is opt-in by source class and scope.
- Tool execution is opt-in and isolated.
- URL fetching obeys SSRF policy before network access.
- Browser rendering obeys the same URL/host policy as HTTP fetching.
- Artifacts are path-safe and cannot escape the artifact root.
- Redaction failure fails closed.
- Public metadata must be explicitly classified.

## Policy Boundaries

| Boundary | Owns |
|---|---|
| `SecurityPolicy` | allow/deny decisions, SSRF, local path, execution policy |
| `CredentialProvider` | secret lookup and scoped credentials |
| `Redactor` | public/internal/sensitive field scrubbing |
| `Authz` | transport caller scopes and admin/write/read rights |
| `ArtifactStore` | traversal-safe artifact reads/writes |
| `ToolSandbox` | CLI/MCP execution limits |

## Authorization Model

Scopes:

| Scope | Allows |
|---|---|
| `axon:read` | query, retrieve, status, capabilities, safe metadata |
| `axon:write` | source jobs, watch creation, memory writes, extraction |
| `axon:admin` | prune, reset, provider config, destructive cleanup |
| `axon:execute` | CLI/MCP tool execution sources |
| `axon:local` | local filesystem sources |

`axon:write` does not imply `axon:admin`, `axon:execute`, or unrestricted local
path access.

## SSRF Policy

All network sources pass SSRF checks before fetch/render.

Default deny:

- loopback and localhost names unless explicitly allowed
- RFC1918/private ranges unless explicitly allowed
- link-local and metadata service ranges
- Unix sockets through URL schemes
- file URLs from web inputs
- DNS rebinding after resolution

Allowed exceptions are config-driven and visible in `doctor`.

Every fetched URL records:

- requested URL
- canonical URL
- resolved IP class
- redirect chain
- policy decision
- redacted headers

## Local Path Policy

Local path sources require `axon:local` or trusted CLI context.

Rules:

- public identity never exposes raw absolute local paths
- symlinks are resolved before read when policy requires containment
- ignored files and binary files follow adapter policy
- secret-looking files are excluded unless explicitly allowed
- `.env`, private keys, token stores, browser profiles, SSH/cloud config dirs,
  Codex/Gemini/OAuth homes, and credential dirs are denylisted by default
- local artifacts are stored by artifact id, not raw path

## Tool Execution Policy

CLI tool and MCP tool sources are powerful. They require explicit execution
permission.

Rules:

- tool execution sources default to `--no-exec` unless caller opts in
- commands are allowlisted by adapter or policy
- arguments are stored separately from shell strings
- no shell expansion unless the source is explicitly a shell script source
- timeout, output byte cap, and environment allowlist are required
- stdout/stderr are treated as untrusted source content
- outputs are redacted before embedding
- failures are indexed only as metadata unless policy allows error-output
  embedding

## Secret Handling

Secrets may live in:

- `.env`
- process environment
- credential provider/keyring
- transport auth headers
- provider SDK internals

Secrets must not live in:

- `config.toml`
- vector payloads
- SourceGraph public evidence
- public artifacts
- source documents
- prepared documents
- job events visible to read-only callers
- generated docs/examples

Redaction must cover:

- bearer tokens
- API keys
- OAuth client secrets
- cookies
- authorization headers
- private keys
- database URLs with passwords
- local credential paths, provider homes, browser profiles, SSH/cloud config,
  and token-store paths when classified sensitive

## Artifact Safety

Artifact reads/writes must:

- canonicalize paths
- reject traversal
- reject symlink root escape
- set safe content type/disposition
- record content hash and byte count
- classify visibility
- enforce retention policy

WARC, screenshots, large outputs, and raw tool responses use ArtifactStore
rather than vector payloads.

## Public Payload Policy

Before writing vector payloads, graph evidence, memory rows, or public status:

- classify fields
- redact sensitive values
- drop unknown sensitive extras
- fail closed on redaction errors
- record `redaction_status`

`extra` metadata from adapters is not automatically public.

## Audit Events

Security-relevant events:

- auth denied
- SSRF denied
- local path denied
- tool execution denied
- redaction failure
- secret detected and dropped
- artifact traversal attempt
- destructive prune approved/executed
- credential missing/degraded

Audit events include `job_id`, caller identity when known, source id when known,
policy id/version, and redacted reason.

## Testing Requirements

Security tests must cover:

- private IP URL denied
- redirect to private IP denied
- local path symlink escape denied
- `.env` excluded from local source by default
- bearer token redacted from logs/events/payloads
- artifact traversal rejected
- CLI tool execution denied without permission
- MCP tool execution denied without permission
- redaction failure blocks vector write
