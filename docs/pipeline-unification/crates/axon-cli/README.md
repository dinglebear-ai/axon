# axon-cli Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-cli` owns the human command-line surface: command parsing, help text,
foreground rendering, progress display, JSON output, and process exit behavior.

## Owns

- clap command tree matching the command contract
- `axon <source>` pipeline entrypoint and explicit action commands
- human output renderers and JSON envelope output
- progress display for foreground and wait paths
- config/env inspection commands
- process exit code mapping

## Must Not Own

- source pipeline domain logic
- provider/store/domain internals
- MCP/REST compatibility aliases
- duplicate DTOs or alternate job/status models

## Public Modules

```text
lib.rs
app.rs
args.rs
commands.rs
render.rs
progress.rs
json.rs
exit.rs
help.rs
config.rs
testing.rs
```

## Public API

- `run`
- `run_once`
- `Cli`
- `CommandModel`
- `CliRenderer`
- `ProgressRenderer`
- `JsonOutput`
- CLI test harness helpers

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-authz`, `axon-observe`,
  `axon-services`, optionally `axon-web`/`axon-mcp` only for subcommand bootstraps
- clap and terminal rendering crates

## Dependencies Forbidden

- direct provider/store/domain internals bypassing services
- ad hoc stdout from lower crates
- compatibility aliases for removed commands

## Generated Artifacts

- [../../schemas/cli-schema.md](../../schemas/cli-schema.md)
- hand-authored/help-verified output in
  [../../surfaces/axon-help.md](../../surfaces/axon-help.md)

## Fixtures And Fakes

- clap snapshot fixture
- JSON output fixture
- human progress fixture
- command failure/exit-code fixture

## Tests

- `axon --help` and `axon help` match the target contract
- every command maps to one service request/result path
- `--json` emits shared envelopes
- no old command aliases exist after the clean break

## Acceptance Criteria

- `axon <source>` is the default pipeline command
- `axon watch`, `axon map`, `axon extract`, `axon ask`, `axon query`,
  `axon retrieve`, and `axon search` have clear, non-overlapping semantics
- CLI remains a transport, not the pipeline owner

See [../README.md](../README.md) and
[../../surfaces/command-contract.md](../../surfaces/command-contract.md).
