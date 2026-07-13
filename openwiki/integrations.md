# Integrations

## External integrations affected indirectly

- CI tooling integration migrated from `taiki-e/install-action` and direct npm install to `jdx/mise-action` and versioned package IDs.
- OpenWiki workflow now uses OpenRouter + LangSmith env for model/tracing (`OPENROUTER_API_KEY`, `OPENWIKI_MODEL_ID`, `LANGSMITH_API_KEY`, `LANGCHAIN_PROJECT`, `LANGCHAIN_TRACING_V2`).
- `scripts/cargo-rustc-wrapper` can delegate to external helper wrappers when present (`cargo-bin-artifact-wrapper`) while preserving local wrapper behavior.

## Related project integrations (README additions)

- Links to related RMCP/community projects were added in `README.md` under **Related Servers**.

## For maintainers

If external integration behavior drifts:

- Check `openwiki/quickstart.md` + `openwiki/workflows.md` for update instructions.
- Reconcile versions in `.github/workflows/openwiki-update.yml` and `.github/workflows/ci.yml` with `jdx/mise-action` install IDs.
