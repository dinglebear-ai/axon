# axon debug
Last Modified: 2026-06-01

Run `doctor`, then ask the configured LLM for prioritized troubleshooting steps.

## Synopsis

```bash
axon debug [context text ...] [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `[context text ...]` | Optional operator context appended to the LLM prompt. |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `AXON_HEADLESS_GEMINI_CMD` | Optional Gemini CLI command. Defaults to `gemini`. |
| `AXON_HEADLESS_GEMINI_MODEL` | Optional Gemini model override for troubleshooting guidance. |

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

- LLM completions go through Gemini headless.
- `AXON_HEADLESS_GEMINI_MODEL` optionally overrides the default Gemini model.
