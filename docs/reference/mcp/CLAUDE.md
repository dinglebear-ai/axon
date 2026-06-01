# MCP Server Documentation -- Axon

Documentation for the Axon MCP server (`axon mcp`).

## Files

| File | Description |
|------|-------------|
| [TOOLS.md](tools.md) | Tool actions, subactions, parameters, and response format |
| [ENV.md](env.md) | MCP-specific environment variables |
| [TRANSPORT.md](transport.md) | stdio, HTTP, and streamable-http transport configuration |
| [DEPLOY.md](deploy.md) | Deployment patterns -- local dev, Docker, SQLite runtime |
| [CONNECT.md](connect.md) | Connect from Claude Code, Codex CLI, Gemini CLI |
| [DEV.md](dev.md) | MCP development workflow and adding new actions |
| [PATTERNS.md](patterns.md) | Code patterns -- dispatch, artifacts, error handling |

## Reading order

**New to the Axon MCP server:**
1. ENV.md -- understand required configuration
2. TRANSPORT.md -- choose stdio or HTTP
3. CONNECT.md -- wire up your MCP client
4. TOOLS.md -- learn the action/subaction API surface

**Adding or modifying MCP actions:**
1. PATTERNS.md -- dispatch and artifact patterns
2. DEV.md -- step-by-step workflow
3. TOOLS.md -- existing API surface

## Cross-references

- [../CONFIG.md](../../guides/configuration.md) -- full environment variable reference
- [../stack/ARCH.md](../../architecture/stack/arch.md) -- trimodal architecture overview
- [../repo/REPO.md](../../contributing/repo/repo.md) -- repository structure
- [../MCP.md](overview.md) -- MCP runtime internals
- [../MCP-TOOL-SCHEMA.md](tool-schema.md) -- wire contract (source of truth)
