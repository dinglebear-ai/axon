# Source Map

## Files directly changed since previous documentation snapshot

- `.github/workflows/ci.yml` (tooling install changes for `taplo`, `cargo-nextest`, `cargo-audit`, `cargo-deny`, `mcporter`)
- `.github/workflows/openwiki-update.yml` (trigger/env/schema + PR payload changes)
- `CLAUDE.md` (OpenWiki docs section marker)
- `Justfile` (helper targets and install hints)
- `README.md` (related-server links addition)
- `scripts/cargo-rustc-wrapper` (helper-aware wrapper + cache helper detection)
- `docs/reference/actions/*.md` (generated surfaces block refresh)
- `docs/reference/actions/README.md` (generated index surface matrix references)

## Key paths for runtime behavior

- Compile/tooling: `scripts/cargo-rustc-wrapper`
- Workflow automation: `.github/workflows/ci.yml`, `.github/workflows/openwiki-update.yml`
- Generated documentation source of truth: `scripts/generate_action_docs.py` and `docs/reference/api-parity.md`
- Manual docs anchors: `CLAUDE.md`, `README.md`, action pages under `docs/reference/actions/`.

## Follow-up for deeper archaeology

If a question requires current implementation details (for example, why a REST route is present for an action), read:

- `docs/reference/api-parity.md`
- `apps/web/openapi/axon.json` and `docs/reference/mcp/tool-schema.md` (for client/server contract evidence)
- CI guard jobs (especially generated parity and route tests in `.github/workflows/ci.yml`)
