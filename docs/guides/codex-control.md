---
title: Codex app-server control in Axon Palette
created: 2026-08-28
updated: 2026-08-28
---

# Codex app-server control in Axon Palette

Axon has two separate Codex app-server uses. `AXON_LLM_BACKEND=codex-app-server`
selects the bounded synthesis provider. The Palette control plane is independent,
disabled by default, and uses a dedicated server-host `CODEX_HOME`.

## Enable the control plane

Set an absolute control home and enable only the mutation families the operator
intends to manage:

```text
AXON_CODEX_CONTROL_ENABLED=true
AXON_CODEX_CMD=/usr/local/bin/codex
AXON_CODEX_CONTROL_HOME=/var/lib/axon/codex-control
AXON_CODEX_CONTROL_ACCOUNT_WRITES=false
AXON_CODEX_CONTROL_CONFIG_WRITES=false
AXON_CODEX_CONTROL_MCP_WRITES=false
AXON_CODEX_CONTROL_PLUGIN_WRITES=false
AXON_CODEX_CONTROL_SKILL_WRITES=false
```

When `AXON_CODEX_CONTROL_ENABLED=true`, `AXON_CODEX_CMD` must be an absolute
path to the executable used by both bounded synthesis and Palette control.
Synthesis alone may use the default bare `codex` command. The setting does not
select separate binaries for those runtimes. The binary and control home must be
owned by the effective Axon server user (which is root only when Axon itself runs
as root). Their ancestor paths must not be group- or world-writable, and all
paths must pass the runtime's non-symlink checks. The binary must be executable;
symlinked binary or home paths are rejected. Axon binds approved operations to
the canonical home identity, app-server boot, active policy, method, exact
parameter digest, and optional config revision.

## Palette workflows

Open **Codex app-server** from the Palette footer. The resource tabs show the
server account summary, models, persisted/active config, MCP servers, plugins,
skills, hooks, and apps. Data belongs to the Axon server host, never the Palette
client device. Auth tokens and secret-shaped event fields are not returned.
The method-inventory tab exposes the complete pinned app-server request and
notification inventory so unsupported or deferred protocol areas remain
discoverable instead of disappearing from the control UI.

Changes use three explicit stages:

1. Prepare records an idempotent durable operation with a redacted request.
2. Approve issues a random, single-use capability for that exact digest.
3. Execute revalidates the home, boot, policy, revision, method, and parameters.

Config values are entered as JSON so booleans, numbers, strings, arrays, objects,
and `null` retain their types. The Palette batch editor accepts a JSON array of at least two
`{"keyPath": ..., "value": ...}` entries, or an object containing an `edits`,
`writes`, or `changes` array with that shape. The shared REST/MCP boundary accepts
any non-empty edits array, including a single edit. Invalid JSON and malformed writes
are shown inline before Prepare is enabled.

Lost responses after a side effect become `ambiguous`; interrupted executions
become `recovery_required` on restart and are not retried blindly. Server-initiated
approval and MCP elicitation requests appear separately and require an operator
approve or deny response.

MCP definitions are written with Codex config RPCs. Apply **Reload MCP servers**
as a separate approved operation before expecting the active MCP runtime to use
the changed definition; config writes do not automatically chain a reload. The
Palette editor accepts either one executable plus a JSON string array of
arguments, or an HTTPS URL. URL transports reject command arguments and
environment entries. Command environment entries use one `NAME=env:SECRET_NAME`
reference per line. Removal is an explicit checkbox; an empty text value does
not remove a definition. Do not send shell command strings or plaintext
credentials. Plugin, marketplace, and
standalone-skill sources using a `source` field must be HTTPS and pinned with a
64-character SHA-256 digest. Local and `file:` sources are rejected.

## REST boundary

Read routes require the normal Axon read authorization. Mutation, approval,
execution, and server-request response routes are mounted under the existing
admin authorization boundary. Family-specific feature flags and durable human
approval are additional checks, not replacements for HTTP authorization.

## Diagnostics and failure states

`axon doctor --json` reports the requested model separately from the effective
model. Codex app-server does not expose a reliable effective-model field, so the
effective model remains `null` with source `not_exposed`. Human doctor output
shows the model catalog count and capability readiness without parsing stderr.

If the Palette reports `degraded`, inspect the detail, verify the absolute Codex
binary and control home, and refresh. Event cursors are boot-scoped; a cursor
from an earlier process requires a canonical snapshot refresh.
