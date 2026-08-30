# Axon E2E catalog

`catalog.json` is the versioned, data-only inventory and scenario contract for
all E2E adapters. `scripts/e2e/validate-catalog.py` reconciles its static
coverage denominator with the generated CLI registry and the established
cross-surface matrix. Runtime results and the coverage numerator are separate.
Release qualification derives its denominator independently as every declared
`(scenario_id, surface)` pair; executing one surface never covers its siblings.

The catalog deliberately cannot express commands, hooks, conditions, shell
templates, interpolation, or plugins. A request is an inert repository-relative
JSON fixture path; an assertion is a stable oracle ID implemented by an adapter.
