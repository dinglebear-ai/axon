# No-MCP Plugin Variant

`marketplace-no-mcp` is a long-lived alternate branch for installs that do not
want bundled MCP server registrations.

`main` is the full/default plugin source. The no-MCP branch keeps the same
plugin assets, hooks, commands, and skills while removing bundled MCP server
registrations for users who rely on a separate gateway, prefer CLI-only usage,
or want skills to use their fallback paths.

The branch is synchronized by
`.github/workflows/sync-marketplace-no-mcp.yml` after pushes to `main` and on a
daily schedule. Drift is checked by
`.github/workflows/check-no-mcp-drift.yml` and can be checked locally with:

```bash
plugins/scripts/check-no-mcp-drift --compare-ref
```

Humans should not casually merge, delete, or retire the branch. Direct writes
are release-maintenance work and must be followed by the drift check.
