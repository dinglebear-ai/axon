# axon-services Agent Instructions

This file is the agent-facing contract for the `axon-services` crate docs.

## When Editing

- Keep transport-neutral orchestration and service entrypoints here.
- Compose lower crates; do not duplicate their internals.
- Do not add CLI formatting, MCP registration, or REST routing.
- Update `README.md`, `../../foundation/types/service-contract.md`,
  `../../foundation/source-pipeline.md`, and surface contracts together.
- Ensure every CLI/MCP/REST action maps to a service request/result.

## Review Checklist

- Service stage order matches the source pipeline contract.
- Service results are `axon-api` DTOs.
- Errors and progress use `axon-error` and `axon-observe`.
