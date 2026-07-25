# Auth Contract
Last Modified: 2026-07-24

## Contract

`axon-authz` owns caller identity, scope checks, source execution permissions,
and visibility decisions. Security policy decides whether an operation is safe;
auth decides whether the caller may request it.

Auth is required across REST, MCP, CLI trusted contexts, jobs spawned by
authenticated requests, and background watches.

## Caller Model

```rust
pub struct CallerContext {
    pub caller_id: Option<String>,
    pub transport: TransportKind,
    pub trusted_local: bool,
    pub scopes: Vec<AuthScope>,
    pub auth_mode: AuthMode,
    pub token_id: Option<String>,
    pub display_name: Option<String>,
}

pub enum AuthScope {
    Read,
    Write,
    Admin,
    Execute,
    Local,
}
```

## Scope Rules

| Operation | Required Scope |
|---|---|
| query/retrieve/status/capabilities | `axon:read` |
| source jobs, watch create/update, memory write | `axon:write` |
| prune/reset/provider config/destructive deletes | `axon:admin` |
| CLI/MCP tool execution source | `axon:execute` |
| local filesystem source | `axon:local` |

`axon:write` does not imply `axon:admin`, `axon:execute`, or `axon:local`.

### `axon:read` / `axon:write` compatibility widening (deliberate)

For the two broad operation classes above (`axon:read` routes and `axon:write`
routes — not `axon:admin`/`axon:execute`/`axon:local`), holding **either**
broad Axon scope satisfies a required broad Axon scope. Newly issued OAuth
tokens default to `AXON_FULL_ACCESS_SCOPE` (`"axon:read axon:write"`, both
scopes together), and the static-bearer/OAuth default-scope configuration
mirrors that (see `docs/pipeline-unification/runtime/security-contract.md`'s
"Contract" paragraph, root `CLAUDE.md`'s "MCP Security Env" section, and
`axon_authz::scope_satisfies`'s doc comment for the implementation). This is a
compatibility affordance, not an oversight: it exists so a route that gets
reclassified between the broad groups does not invalidate every previously
issued token. It does **not** extend to the fine-grained `axon:admin` /
`axon:execute` / `axon:local` scopes, which still require the caller to hold
that exact scope — see the paragraph above.

### Strict scope elevation (`mutates_if`)

A small number of routes are nominally `axon:read` for schema/docs purposes
but actually mutate state unconditionally today — `/v1/search` and
`/v1/research` (and the equivalent MCP `search`/`research` actions) always
enqueue a bounded Source job per result, with no request-level opt-out. These
routes enforce an in-handler conditional elevation to `axon:write`
(`require_mutates_if_write_scope` in `axon-web`'s
`server/handlers/exploration.rs`; `mutates_if_upgrade` /
`check_scope_explicit` in `axon-mcp`'s `server/authz.rs`, applied at both the
synchronous `call_tool` dispatch path and the deferred task-tool path in
`server/tasks.rs`).

Because the compatibility widening above already treats `axon:read` as
satisfying `axon:write` for ordinary broad routes, this elevation check
deliberately does **not** use the same broad `scope_satisfies` matcher —
doing so would make the elevation a silent no-op (CWE-863): a caller holding
only `axon:read` would already "satisfy" the elevated `axon:write`
requirement before the elevation is even consulted. Instead, elevation checks
use `axon_authz::has_explicit_scope`, which requires the caller to hold the
literal `axon:write` scope string with no broad-scope widening in either
direction. Any future `mutates_if`-style elevation must use
`has_explicit_scope`, not `scope_satisfies`, for the same reason.

## Trusted CLI Context

Local CLI may be trusted when running as the local user and not through a remote
transport. Trusted CLI may receive implicit local permissions only when config
allows it. REST and MCP never infer local trust from network location alone.

## Job Propagation

Every job stores an auth snapshot:

- caller id when known
- transport kind
- granted scopes
- visibility ceiling
- request time
- policy version

Workers enforce the snapshot. A job must not gain broader permission because
server config changed after enqueue.

## Visibility

Auth controls how much state a caller can see:

| Visibility | Read Scope | Notes |
|---|---|---|
| public | read | safe metadata and redacted text |
| internal | write/admin/local depending source | local paths, provider internals |
| sensitive | admin only or never | secrets are still redacted |
| redacted | any | explicit placeholder only |

## Transport Requirements

REST:

- bearer/static token and OAuth modes map to `CallerContext`
- all write/admin routes require auth unless loopback trusted-dev mode is active
- OpenAPI documents required scopes

MCP:

- tool input cannot self-declare scopes
- MCP auth wrapper constructs `CallerContext`
- tool execution sources require `axon:execute`

CLI:

- local commands construct trusted or untrusted caller context explicitly
- `--json` output obeys the same visibility filtering

## Testing Requirements

- every route/action/command has scope tests
- job auth snapshot cannot escalate
- read-only caller cannot see sensitive fields
- write caller cannot prune/reset
- execute/local are independent from write/admin
- fake authz supports allow, deny, and visibility filtering
