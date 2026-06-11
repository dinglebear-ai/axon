# Changelog

## Unreleased

### Added
- Documented the `memory.remember`, `memory.search`, `memory.show`, `memory.link`, `memory.supersede`, and `memory.context` agent-memory actions in the `using-axon` skill.
- Added a defensive SessionStart hook that recalls compact `memory.context` for the current git project when Axon memory is available.

## [1.5.4] - 2026-05-06

### Added
- `.mcp.json` wiring the `axon` MCP server (stdio transport via `axon mcp`).
- `userConfig` block in `.claude-plugin/plugin.json` exposing Qdrant URL, TEI URL, collection, LLM endpoint/model/API key, Tavily API key, and Chrome remote URL — substituted into the MCP server env via `${user_config.*}`.

### Fixed
- Plugin description now correctly reports 16 skills (previously listed 15).

## [1.5.3] - 2026-05-05

### Changed
- Address PR #67 review feedback.

## [1.5.2] - 2026-05-05

### Changed
- Internal version bump tracking the axon repo `pp5.10` series.

## [1.1.0] - 2026-05-03

### Added
- Initial plugin scaffold.
