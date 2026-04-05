# MCP Server Documentation -- Axon

Documentation for the Axon MCP server (`axon mcp`).

## Files

| File | Description |
|------|-------------|
| [TOOLS.md](TOOLS.md) | Tool actions, subactions, parameters, and response format |
| [ENV.md](ENV.md) | MCP-specific environment variables |
| [TRANSPORT.md](TRANSPORT.md) | stdio, HTTP, and streamable-http transport configuration |
| [DEPLOY.md](DEPLOY.md) | Deployment patterns -- local dev, Docker, lite mode |
| [CONNECT.md](CONNECT.md) | Connect from Claude Code, Codex CLI, Gemini CLI |
| [DEV.md](DEV.md) | MCP development workflow and adding new actions |
| [PATTERNS.md](PATTERNS.md) | Code patterns -- dispatch, artifacts, error handling |

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

- [../CONFIG.md](../CONFIG.md) -- full environment variable reference
- [../stack/ARCH.md](../stack/ARCH.md) -- trimodal architecture overview
- [../repo/REPO.md](../repo/REPO.md) -- repository structure
- [../MCP.md](../MCP.md) -- MCP runtime internals
- [../MCP-TOOL-SCHEMA.md](../MCP-TOOL-SCHEMA.md) -- wire contract (source of truth)
