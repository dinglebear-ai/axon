# Runtime Auth

Last Modified: 2026-07-19

`axon-authz` owns caller identity, scope checks, execution-affinity policy, and
visibility decisions. Transports authenticate; this crate authorizes (no
duplication of policy). It does **not** own OAuth/bearer HTTP middleware, MCP
transport handshake, source fetching, SSRF client implementation, or redaction
detectors.

> Contract source:
> [`docs/pipeline-unification/runtime/auth-contract.md`](../../pipeline-unification/runtime/auth-contract.md).
> Implementation: [`crates/axon-authz/src/`](../../../crates/axon-authz/src/).

## `CallerContext`

```rust
struct CallerContext {
    caller_id: Option<String>,
    transport: TransportKind,
    trusted_local: bool,
    scopes: Vec<AuthScope>,
    auth_mode: AuthMode,
    token_id: Option<String>,
    display_name: Option<String>,
}
```

## Scopes

`AuthScope` variants: `Read`, `Write`, `Admin`, `Execute`, `Local`.

**String literals (do not alter — baked into issued OAuth tokens; changing
invalidates every existing token):** `axon:read`, `axon:write`, `axon:admin`,
`axon:execute`, `axon:local` (plus `AXON_FULL_ACCESS_SCOPE`).

| Operation | Required scope |
|---|---|
| query / retrieve / status / capabilities | `axon:read` |
| source jobs / watch create-update / memory write | `axon:write` |
| prune exec / reset exec / provider config / destructive deletes | `axon:admin` |
| CLI/MCP tool-execution sources | `axon:execute` |
| local filesystem sources | `axon:local` |

`axon:write` does **not** imply `axon:admin`/`axon:execute`/`axon:local`.
Newly issued OAuth tokens default to both `axon:read` and `axon:write` (either
Axon scope is accepted for read/write routes for compatibility).

## Execution visibility

| Class | Who sees it |
|---|---|
| `public` | read scope (safe metadata + redacted text) |
| `internal` | write/admin/local depending on source (local paths, provider internals) |
| `sensitive` | admin only, or never (secrets still redacted) |
| `redacted` | any scope (explicit placeholder only) |

## Loopback vs non-loopback

REST and MCP **never** infer local trust from network location alone. The local
CLI may be trusted when running as the local user, not through a remote
transport; the trusted CLI receives implicit local permissions only when config
allows. All REST/MCP write/admin routes require auth unless loopback
trusted-dev mode is active.

- Loopback bind (`127.0.0.1`/`::1`) — tokenless allowed, or `AXON_HTTP_TOKEN`,
  or OAuth.
- Non-loopback bind — `AXON_HTTP_TOKEN` (bearer / `x-api-key`) **or** OAuth
  (`AXON_AUTH_MODE=oauth`).

OAuth email allowlisting is the access boundary: allowed OAuth users receive
full Axon server access; `AXON_AUTH_ADMIN_EMAIL` grants admin scope.

## Job auth propagation

Every job stores an immutable `auth_snapshot`: caller id, transport kind,
granted scopes, visibility ceiling, request time, policy version. Workers
enforce the snapshot — a job must **not** gain broader permission because server
config changed after enqueue. Scope-satisfaction is deterministic and
closed-by-default; ambiguous decisions fail closed; denied decisions carry
stable machine-readable reasons. `AuthSnapshot::trusted_system` is used for
system-triggered work (e.g. automatic cleanup-debt drain).

## Principles

- HTTP access requires configured auth outside loopback development.
- Local CLI calls are locally trusted but still go through source safety checks.
- Source execution rechecks auth at execution time.
- Destructive operations require write/admin policy and explicit confirmation.

## Source auth

Local paths, tool sources, MCP sources, and private network targets require
policy checks before acquisition **and** before execution. See
[security.md](security.md) for the SSRF/local-path/tool-execution policy
details.

If the auth surface changes, update this file and
[`crates/axon-authz/src/CLAUDE.md`](../../../crates/axon-authz/src/CLAUDE.md)
in the same PR.
