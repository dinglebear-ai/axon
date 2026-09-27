# axon-cli Agent Instructions

This file is the agent-facing contract for the `axon-cli` crate docs.

## When Editing

- Keep clap command parsing, help text, human rendering, JSON output, progress
  display, config inspection, and exit codes here.
- Do not bypass `axon-services`.
- Do not add backward-compatibility aliases for removed commands.
- Update `README.md`, `../../surfaces/command-contract.md`,
  `../../surfaces/axon-help.md`, and `../../schemas/cli-schema.md` together.
- Preserve clear differences among `ask`, `query`, `retrieve`, and `search`.

## Review Checklist

- `--json` emits shared envelopes.
- Human progress is rendered from shared progress events.
- Commands remain thin transport adapters.
