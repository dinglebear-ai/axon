# Stage Results
Last Modified: 2026-07-15

Stage results describe pipeline progress and outcomes across acquisition,
preparation, graphing, embedding, publishing, and cleanup.

## Contract

Stage results are transport-neutral. CLI, MCP, REST, web, Palette, Android, and
Chrome clients should consume the same DTO shapes rather than inventing
surface-specific progress models.

## Required Fields

- stage or phase
- status
- source id and generation when available
- item/document counts when applicable
- warning and error summaries
- durable job id for asynchronous work

## Ownership

DTO ownership belongs in `axon-api`. Emission belongs in the service/domain
stage that performs the work.
