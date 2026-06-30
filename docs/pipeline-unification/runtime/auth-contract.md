# Auth Contract
Last Modified: 2026-06-30

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
