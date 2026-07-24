# Runtime Security

Last Modified: 2026-07-19

Security policy spans SSRF defense, local-path trust, secret/tool-execution
policy, artifact access scoping, and destructive-operation safeguards. See
[auth.md](auth.md) for scopes and [redaction.md](redaction.md) for the
redactor boundary.

> Contract source:
> [`docs/pipeline-unification/runtime/security-contract.md`](../../pipeline-unification/runtime/security-contract.md).

## Policy boundaries

| Boundary | Owns |
|---|---|
| `SecurityPolicy` | allow/deny, SSRF, local path, execution policy |
| `CredentialProvider` | secret lookup, scoped credentials |
| `Redactor` | public/internal/sensitive scrubbing |
| `Authz` | transport caller scopes (admin/write/read) |
| `ArtifactStore` | traversal-safe reads/writes |
| `ToolSandbox` | CLI/MCP execution limits |

## SSRF policy (default deny)

Blocked unless explicitly allowed:

- loopback / localhost names
- RFC1918 / private ranges
- link-local and metadata-service ranges
- Unix sockets via URL schemes
- `file:` URLs from web inputs
- DNS rebinding after resolution

Allowed exceptions are config-driven and visible in `axon doctor`. Every
fetched URL records: requested URL, canonical URL, resolved IP class, redirect
chain, policy decision, redacted headers.

## Local path policy

Requires `axon:local` scope or trusted CLI context. Env opt-in:
`AXON_SOURCE_LOCAL_ALLOWED_ROOTS` (comma-separated allowed roots).

Rules:

- Public identity never exposes raw absolute local paths.
- Symlinks are resolved before read when policy requires containment.
- Ignored/binary files follow adapter policy.
- Secret-looking files are excluded unless explicitly allowed.
- **Denylisted by default:** `.env`, private keys, token stores, browser
  profiles, SSH/cloud config dirs, Codex/Gemini/OAuth homes, credential dirs.
- Local artifacts are stored by artifact id, not raw path.

**Fail-closed REST rule:** local-path source requests over REST are fail-closed
unless under loopback affinity, a configured allowed root, or a prepared upload.

## Secret + tool-execution policy

Secrets may live in: `.env`, process env, `CredentialProvider`/keyring,
transport auth headers, provider SDK internals.

Secrets must **not** live in: `config.toml`, vector payloads, SourceGraph
public evidence, public artifacts, source/prepared documents, job events
visible to read-only callers, generated docs/examples.

Tool-execution sources:

- Default to `--no-exec` unless caller opts in.
- Commands allowlisted by adapter/policy.
- Arguments stored separately from shell strings; no shell expansion unless the
  source is explicitly a shell-script source.
- Timeout, output-byte-cap, and environment allowlist required.
- stdout/stderr treated as untrusted source content; outputs redacted before
  embedding.
- Failures indexed only as metadata unless policy allows error-output embedding.

## Artifact access scoping

Reads/writes must canonicalize paths, reject traversal, reject symlink root
escape, set safe content type/disposition, record content hash + byte count,
classify visibility, and enforce retention. WARC/screenshots/large outputs/raw
tool responses use `ArtifactStore`, not vector payloads.

## Public payload policy

Before writing vector payloads / graph evidence / memory rows / public status:
classify fields, redact sensitive values, drop unknown sensitive extras, fail
closed on redaction errors, record `redaction_status`. Adapter `extra` metadata
is **not** automatically public.

## Destructive operations

`prune exec` and `reset exec` require `axon:admin` scope and explicit
confirmation (`--confirm`). `PruneExecutor::execute()` is the single
chokepoint for destructive deletes — see [pruning.md](pruning.md).

## Audit events

Auth denied, SSRF denied, local-path denied, tool-execution denied, redaction
failure, secret detected+dropped, artifact traversal attempt, destructive prune
approved/executed, credential missing/degraded. Each includes `job_id`, caller
identity, source id, policy id/version, and a redacted reason.

## Review focus

Every new adapter must define trust boundaries, credential handling, and
execution behavior **before** it is exposed through source requests.

If the security policy changes, update this file and
[`crates/axon-authz/src/CLAUDE.md`](../../../crates/axon-authz/src/CLAUDE.md)
in the same PR.
