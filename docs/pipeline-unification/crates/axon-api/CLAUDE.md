# axon-api Agent Instructions

This file is the agent-facing contract for the `axon-api` crate docs. Do not
duplicate the README; use this as the maintenance checklist.

## When Editing

- Treat `axon-api` as the transport-neutral DTO and schema crate.
- Move shared request/result/envelope/enum shapes here instead of duplicating
  them in transports or domain crates.
- Do not add provider clients, stores, runtime side effects, or rendering.
- Update `README.md`, `../../foundation/api-contract.md`,
  `../../foundation/types/dto-contract.md`, and
  `../../schemas/api-dto-schema.md` together.
- Keep JSON names stable unless the clean-break contract explicitly changes
  them.

## Review Checklist

- DTOs are serializable, schema-generatable, and transport-neutral.
- New enum variants are reflected in schema fixtures.
- Domain crates depend on these DTOs rather than redefining them.
