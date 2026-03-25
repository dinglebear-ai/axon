# axon debug
Last Modified: 2026-03-03

Run `doctor`, then ask the configured LLM for prioritized troubleshooting steps.

## Synopsis

```bash
axon debug [context text ...] [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `[context text ...]` | Optional operator context appended to the LLM prompt. |

## Required Environment Variables

| Variable | Description |
|----------|-------------|
| `AXON_ACP_ADAPTER_CMD` | ACP adapter command (e.g. `codex`). Required — debug fails fast without it. |
| `OPENAI_MODEL` | Model name passed to the ACP adapter for troubleshooting guidance. Required. |

## Flags

All global flags apply. Key flag:

| Flag | Default | Description |
|------|---------|-------------|
| `--json` | `false` | Include both `doctor_report` and `llm_debug` in JSON. |

## Examples

```bash
# Basic debug workflow
axon debug

# Include symptom context for better guidance
axon debug "crawl jobs stuck in pending for 30m"

# Structured output
axon debug "qdrant timeout after restart" --json
```

## Notes

- Fails fast if `AXON_ACP_ADAPTER_CMD` is unset or empty.
- Fails fast if `OPENAI_MODEL` is unset or empty.
- LLM completions go through the ACP adapter subprocess (`AXON_ACP_ADAPTER_CMD`), not directly to an OpenAI-compatible HTTP endpoint.
