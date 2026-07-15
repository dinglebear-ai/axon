# CLI Tool Sources
Last Modified: 2026-07-15

CLI tool sources let Axon document local command surfaces through the unified
source pipeline.

## Source Shape

Use a `cli:` source identifier for tool documentation targets. The adapter owns
tool discovery, command metadata capture, normalization, and safety policy.

## Pipeline Behavior

CLI tool content is acquired as source items, prepared into documents, and
published through the same ledger, parser, graph, embedding, and vector stages
as other source families.

## Safety

Adapters must treat local execution as privileged. Discovery should prefer
static help, schema files, or explicit safe commands, and must honor redaction
and local-path policy.
