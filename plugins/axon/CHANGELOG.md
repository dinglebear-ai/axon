# Changelog

## Unreleased

### Added
- Rebuilt the plugin skill surface around 25 plain-name Axon skills under `skills/`: one `using-axon` guide, eight core command/action skills, and sixteen outcome-focused workflow skills.
- Added OpenAI skill metadata at `agents/openai.yaml` for every shipped skill.
- Added shared workflow references under `references/`, including capture recipes, workflow authoring guidance, and output templates.
- Documented the `memory.remember`, `memory.search`, `memory.show`, `memory.link`, `memory.supersede`, and `memory.context` agent-memory actions in the `using-axon` skill.
- Added a defensive SessionStart hook that recalls compact `memory.context` for the current git project when Axon memory is available.

### Changed
- Rebranded the imported Firecrawl-style skills as Axon-native workflows, removing the `axon-` folder prefix now that the skills live inside the Axon plugin namespace.
- Moved the runtime RAG synthesis prompt from a user-facing skill path to `references/rag-synthesize/` so it can be embedded by Axon without appearing as an invocable plugin skill.
- Updated the plugin README to describe the current HTTP MCP configuration, minimal user config, skills layout, and reference files.

### Fixed
- Added skill hygiene coverage so missing `SKILL.md`, missing `agents/openai.yaml`, stale `axon-` folder prefixes, or misplaced reference-only skills fail in tests.
- Clarified that Axon download/offline-capture guidance is a composed `scrape`/`crawl --output-dir`/`screenshot` workflow, not a single first-class offline-site mirroring command.

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
