# Connect to Axon MCP

Last reviewed: 2026-09-29

Choose a local stdio process or an existing HTTP server. Do not replace an
existing client configuration with a template: add one named entry and
preserve unrelated settings. The examples below are alternatives, not a
request to register the same server twice.

## Before connecting

For stdio, use the absolute path to the intended Axon binary and verify its
version, executing user, HOME, configuration, and provider endpoints.
`axon mcp` does not open an HTTP listener by default.

For HTTP, run the intended native Axon service with `/mcp` mounted, typically
through `axon serve` or `axon serve mcp`. The default local origin is
`http://127.0.0.1:8001`, but use the actual deployment origin. Non-loopback
HTTP requires the configured bearer/OAuth policy. Panel unlock passwords
and sessions are separate from API/MCP authorization.

These instructions configure clients, not Axon provider credentials or
server-side source permissions. Review [transport](transport.md) and
[authentication](../../operations/auth/mcp-auth.md).

## Claude Code

Register local stdio or loopback HTTP with the client CLI:

```bash
claude mcp add --scope user --transport stdio axon -- /absolute/path/to/axon mcp
# Alternative:
claude mcp add --scope user --transport http axon http://127.0.0.1:8001/mcp
claude mcp list
```

The default local scope and user scope are stored in `~/.claude.json`; local
scope is keyed to the project. Shared project scope uses `.mcp.json`, not
`.claude/settings.local.json`. HTTP entries need `type: "http"`. For a
static-token project entry, keep the variable reference rather than a secret:

```json
{
  "mcpServers": {
    "axon": {
      "type": "http",
      "url": "https://axon.example.com/mcp",
      "headers": { "Authorization": "Bearer ${AXON_HTTP_TOKEN}" }
    }
  }
}
```

Supply the token in the client environment, or use the configured OAuth
login flow instead. Follow the client trust/approval prompts. Scope and
expansion behavior are documented in the
[official Claude Code MCP guide](https://code.claude.com/docs/en/mcp).

Claude Desktop chat configuration is a different client surface. Use that
application's current connector/developer setup workflow; do not copy Claude
Code settings paths into it.

## Codex CLI and IDE

Codex uses `~/.codex/config.toml`, or `.codex/config.toml` for a trusted
project. It does not use the old `mcpServers` JSON examples in `mcp.json`.
For local stdio, add this table to the existing TOML file:

```toml
[mcp_servers.axon]
command = "/absolute/path/to/axon"
args = ["mcp"]
```

For HTTP, use this table instead:

```toml
[mcp_servers.axon]
url = "https://axon.example.com/mcp"
bearer_token_env_var = "AXON_HTTP_TOKEN"
```

The variable contains the Axon HTTP token, not an OpenAI API key. For an
OAuth-configured server, omit that token setting and use the client login
flow. Verify with `codex mcp list` and `/mcp`; use `codex mcp --help` for
the installed version. The
[official MCP guide](https://learn.chatgpt.com/docs/extend/mcp) owns client
configuration, scope, and authentication options.

## Gemini CLI

Ordinary server configuration lives under `mcpServers` in
`~/.gemini/settings.json` or project `.gemini/settings.json`, not a standalone
`gemini-extension.json`. Stdio uses `command` and `args`; streamable HTTP
uses **`httpUrl`**, while `url` selects the older SSE transport.

```json
{
  "mcpServers": {
    "axon": {
      "httpUrl": "https://axon.example.com/mcp",
      "headers": { "Authorization": "Bearer ${AXON_HTTP_TOKEN}" }
    }
  }
}
```

For local stdio:

```bash
gemini mcp add --scope user axon /absolute/path/to/axon mcp
gemini mcp list
```

Choose the server's actual auth mode and complete the client trust flow.
The [official Gemini MCP guide](https://geminicli.com/docs/tools/mcp-server/)
documents environment expansion and transport-specific settings.

## Verify the runtime, not just registration

A configuration entry is not proof of a connection. Initialize a real MCP
client, discover `tools/list`, and verify the selected legacy/atomic/both
projection and auxiliary dashboard. Inspect the exact input schema before
a read-only status/help call.

Legacy mode calls the aggregate `axon` tool with `action`; atomic mode uses
the discovered leaf name without fixed action/subaction fields. Reconnect
clients after a startup-static projection change. An HTTP `/healthz` success
checks process health, not the MCP handshake or provider execution. A bare
GET of `/mcp` can have protocol-specific requirements and is not a sufficient
connection test.

For queued source work, follow the returned job to terminal state. A client
timeout does not prove cancellation, and a task/job descriptor does not prove
publication. Do not repeat a write with unknown commit status.

## Troubleshooting

Check the failing hop: local executable/configuration, remote origin/TLS,
authentication, MCP initialization/session, tool discovery, or provider/job
execution. Record the client/server versions and safe diagnostics; do not
paste secrets or entire environment files.

`axon doctor` inspects the runtime where it executes. The bundled CLI does
not forward arbitrary commands to a remote `axon serve`; remote checks must
use that server's REST/MCP surface. See [environment](env.md),
[deployment](deploy.md), and [MCP overview](overview.md).
