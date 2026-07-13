# Workflows

This page tracks repository workflow behavior relevant to this update.

## CI (`.github/workflows/ci.yml`)

CI helper-tool bootstrap changed to `jdx/mise-action` for consistent, versioned installs:

- `taplo-fmt` checks now install `cargo:taplo-cli`.
- The main test job now installs `cargo:cargo-nextest` through mise.
- `security` now installs `cargo:cargo-audit@0.22.1` and `aqua:EmbarkStudios/cargo-deny@0.19.0` through mise.
- `mcp-smoke` and related jobs install `node npm:mcporter@0.7.3` via mise.

## OpenWiki update workflow (`.github/workflows/openwiki-update.yml`)

The workflow now:

- runs `openwiki code --update --print` on schedule and manual dispatch
- requires OpenRouter + LangSmith env vars for run-time tracing (`OPENROUTER_API_KEY`, `OPENWIKI_MODEL_ID`, `LANGSMITH_API_KEY`, `LANGCHAIN_PROJECT`, `LANGCHAIN_TRACING_V2`)
- updates `openwiki` plus control files in generated PR payload (`AGENTS.md`, `CLAUDE.md`, `.github/workflows/openwiki-update.yml`)
- uses pinned `peter-evans/create-pull-request` SHA

## Why this changed

- Single tool installation surface for CI helpers.
- Better workflow PR payload guarantees for wiki-control-docs alignment.