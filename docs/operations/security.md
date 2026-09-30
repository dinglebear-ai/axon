---
title: "Security Model"
created: 2026-02-25
updated: 2026-09-29
---

# Security Model

Last reviewed: 2026-09-29

## Scope and threat model

Axon handles untrusted source text, URLs, file paths, provider responses, and
remote client requests. Its security boundaries include HTTP authentication,
operation authorization, source containment, provider/tool execution, durable
visibility, and response/log redaction. A trusted local installation is not
a license to expose the same operations anonymously over HTTP.

Production is native Axon under systemd in Incus or bare-metal Linux with
external providers. Follow [deployment](deployment.md) and verified local
installation notes. This guide describes controls and required checks; it
is not a claim that every possible provider/browser path is invulnerable.

## SSRF and URL validation

[`axon-core/src/http/ssrf.rs`](../../crates/axon-core/src/http/ssrf.rs) owns
URL policy, IP-range checks, resolver protection, and crawl blacklist support.
[`http/client.rs`](../../crates/axon-core/src/http/client.rs) installs the
shared HTTP safety behavior, including redirect validation. The old root
`src/core`, `src/crawl`, and `src/ingest` paths are not current owners.

The public-source policy rejects non-HTTP schemes and disallowed loopback,
private, link-local, reserved hostname/IP forms, including mapped IPv6.
Parse-time validation alone cannot prevent DNS rebinding. The production
resolver checks resolved addresses for supported shared-client paths;
redirect targets must also pass policy. Do not construct an unguarded
HTTP client to bypass a rejected source.

Web acquisition additionally crosses Spider and browser/CDP boundaries.
Inspect [adapter security](../../crates/axon-adapters/src/acquisition_security.rs),
[source security](../../crates/axon-services/src/source/security.rs), and
the chosen acquisition implementation when adding a path. A guarded reqwest
client does not prove that every browser subresource or upstream tool uses
that same resolver. Browser providers therefore remain privileged services
inside an explicitly controlled network boundary.

Test-only loopback allowances and mock-client lifecycle helpers are not
production configuration. Preserve cfg gates and test both denied production
inputs and authorized fixtures. Include redirects, alternate IP spellings,
DNS rebinding, and interrupted/partial acquisition in relevant changes.

## Filesystem and tool execution

Local sources are resolved on the executing host against approved roots and
canonical paths. `AXON_SOURCE_LOCAL_ALLOWED_ROOTS` replaces the rejected
legacy MCP embed-root name. A client-provided path, discovery result, or
metadata descriptor does not grant filesystem or execution permission.

CLI/MCP tool acquisition requires its own authorization, allowlists, timeout,
output caps, cancellation, and redaction. Use the existing source adapter
contracts rather than a provider-specific shell pipeline. Temporary data
needs cleanup on success, failure, and cancellation; failed adapter release
is tracked as cleanup debt for controlled retry.

Do not claim a parsed configuration field enforces a limit without checking
its runtime consumer. In particular, the current session adapter reads whole
selected transcript files and does not consume the old session-byte-limit
variable. See [session ingestion](../guides/ingest/sessions.md),
[local sources](../guides/local-sources.md), and
[CLI tool sources](../guides/cli-tool-sources.md) and
[MCP tool sources](../guides/mcp-tool-sources.md).

## HTTP authentication and operation authorization

Shared HTTP authorization is owned by
[`axon-authz`](../../crates/axon-authz/src/), with the MCP compatibility
re-export in [`axon-mcp/src/auth.rs`](../../crates/axon-mcp/src/auth.rs).
HTTP API and MCP use the same caller-derived policy; stdio uses the local
process boundary, not an HTTP token exchange.

| Mode | Required boundary |
|---|---|
| Loopback-only development without a token | Deliberately local listener; not anonymous remote access |
| Static bearer | Configured `AXON_HTTP_TOKEN`; supported bearer/API-key header handling |
| OAuth | Configured public origin, identity/client/secret/redirect settings, validated caller and scopes |
| Stdio | Client-owned child process and executing OS account, with operation/source policies still applicable |

A non-loopback listener without configured authentication must refuse startup.
Whitespace-only tokens are not valid protection. Configure TLS at the
intended origin/proxy and check the effective listener rather than relying
on the presence of a token in an unused file.

`axon:write` and operation-specific authorization/elevation checks are
separate from descriptive tool safety hints. Some nominally read-oriented
operations such as external search/research can acquire/index sources and
require explicit write elevation. Unknown selectors must fail before side
effects. Do not copy a static read/write table without verifying the live
action metadata and handler checks.

Source, job, artifact, and task visibility must preserve caller identity
through shared service dispatch. The primary MCP action enum is not the
full authorization surface: system/watch request types, atomic projections,
resources, auxiliary dashboard, and tasks also require coverage. See
[MCP authentication](auth/mcp-auth.md) and
[the MCP tool contract](../reference/mcp/tool-contract.md).

## Host and origin policy

Host validation and browser-origin policy are owned by
[`axon-web/src/security.rs`](../../crates/axon-web/src/security.rs),
[routing security](../../crates/axon-web/src/server/routing_security.rs), and
[MCP CORS](../../crates/axon-mcp/src/cors.rs). Allowed origins must be explicit
for the intended client deployment.

CORS is a browser constraint, not authentication: a non-browser caller can
omit Origin. Conversely, a successful authenticated command-line request
does not prove that a browser origin or preflight is allowed. Test allowed
and denied Host/Origin values, preflight headers, authentication, and real
MCP initialization against the changed listener. Do not use a wildcard proxy
rule to conceal a failing allowed-origin check.

## Web panel authorization

The web panel is an administrative surface. Its password/session mechanism
is separate from API/MCP bearer or OAuth identity. Do not substitute an
API token for the panel unlock credential.

[`axon-web/src/auth.rs`](../../crates/axon-web/src/auth.rs) creates a random
panel password file under the Axon home with restrictive creation semantics
on Unix. Startup should report the protected path, not the password. New-file
creation uses exclusive creation and `O_NOFOLLOW`; do not extrapolate that
into a claim that every later existing-file read has the same checks. Verify
existing file ownership, permissions, and location during deployment.

The login handler returns the panel credential as the token in the current
implementation. Treat it as a bearer secret, not a separately revocable
short-lived session. Keep it out of URLs, logs, screenshots, shared client
configuration, and browser storage exports.

Use the actual panel/router source for the available routes. The former
fixed list and claim that no download/WebSocket/output route could exist
were not a maintained security contract. Command operations still require
their parser, service policy, and administrative authorization; do not infer
arbitrary-shell permission from a command-shaped UI.

## Secrets, configuration, and logs

`~/.axon/.env` is a conventional environment source, **not the only secret
store**. Service managers, explicit environment files, process variables,
provider credentials, OAuth state, client credentials, and the panel password
can live separately. Inventory the effective installation and use the
approved secret-management process for each.

Use TOML for non-secret tuning. Unknown-field rejection catches typos but
is not a generic secret detector: a credential pasted into an accepted
string/header field does not become safe to commit. Keep secrets out of
repository examples and generated artifacts. Preserve unrelated keys rather
than replacing a configured file with a template.

[`src/main.rs`](../../src/main.rs) owns environment-file loading. An invalid
explicit `AXON_ENV_FILE` can fall through to later locations; canonical-file
permission/symlink failure has different handling. Check startup diagnostics
and effective values; do not assume a requested file was loaded successfully.

Config debug formatting, provider error bodies, URL redaction, command output,
and persisted metadata each need their own redaction tests. Structured
results go to stdout; diagnostic logs go to stderr and correlate through
`axon-observe`. Never flatten a failure into a successful empty response.
Report safe cause, stage/entity/job IDs, retryability, known side effects,
and recovery instructions without leaking secrets.

## Retained content, caches, and backups

Session and tool acquisition redact and normalize before publication, and
retrieval must honor committed generations and visibility. Do not equate
redaction with encryption or guarantee that every input secret pattern is
recognized. Review [redaction](../reference/runtime/redaction.md),
[metadata](../reference/sources/metadata-payload.md), and provider failure
paths before adding new fields.

The optional full-document ask cache retains source text in process memory.
Its daemon startup core-dump guard does not encrypt memory or protect a
compromised process. SQLite also contains an embedding-vector cache, while
Qdrant owns published retrieval vectors. Backups need equivalent access
controls and consistent state boundaries; follow
[backup and restore](../reference/operations/backup-restore.md).

## Network exposure

Tracked Compose provider mappings are loopback-bound and checked by
`scripts/check_compose_port_bindings.py`. Containers still listen on their
internal interfaces. A custom network, port override, proxy, or tunnel can
change reachability; inspect actual published ports and policy.

Chrome CDP/management is a privileged unauthenticated control interface,
not a public API to expose directly. Qdrant and embedding providers also
need an explicit trusted network/authentication boundary. Use deliberate
authenticated cross-host access rather than removing loopback bindings
to make a failing connection work.

## Verification and maintenance

For a security-relevant change, test denial as well as success, including
invalid inputs, missing/wrong scope, origin/host restrictions, source roots,
redaction, output limits, and cancellation. Verify discovery and one safe
call through the real configured transport after reload/reconnect.

Do not perform reset, broad process kills, firewall changes, or credential
rotation as a documentation smoke test. Destructive cleanup needs the
reviewed [prune](../reference/runtime/pruning.md) or
[reset](../reference/operations/reset.md) plan and explicit target approval.
Preserve partial-success and unknown-commit evidence before retrying.
