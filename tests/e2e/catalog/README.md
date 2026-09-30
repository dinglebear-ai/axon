# Axon E2E catalog

`catalog.json` is the versioned, data-only inventory and scenario contract for
all E2E adapters. `scripts/e2e/validate-catalog.py` reconciles its static
coverage denominator with the generated CLI, MCP operation, and REST route
JSON registries plus the established cross-surface matrix. The human parity
Markdown is not an input. Literal family identities remain distinct across
transports (for example CLI code-search versus MCP code_search). The legacy
`api_parity` inventory label identifies these registry-derived families.
Newly advertised families expand the behavioral coverage denominator; their
classification alone does not count as passing runtime evidence. Runtime results and the coverage numerator are separate.
Release qualification derives its denominator independently as every declared
`(scenario_id, surface)` pair; executing one surface never covers its siblings.

The catalog deliberately cannot express commands, hooks, conditions, shell
templates, interpolation, or plugins. A request is an inert repository-relative
JSON fixture path; an assertion is a stable oracle ID implemented by an adapter.
