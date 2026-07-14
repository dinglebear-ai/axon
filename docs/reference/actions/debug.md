# axon debug
Last Modified: 2026-06-13

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon debug ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


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
| `AXON_LLM_BACKEND` | Completion backend. Defaults to `gemini-headless`; set `openai-compat` for OpenAI-compatible chat completion endpoints. |
| `AXON_HEADLESS_GEMINI_CMD` | Optional Gemini CLI command. Defaults to `gemini`. |
| `AXON_SYNTHESIS_HEADLESS_GEMINI_MODEL` / `AXON_HEADLESS_GEMINI_MODEL` | Optional Gemini synthesis model override; the unprefixed form is a legacy alias. |
| `AXON_OPENAI_BASE_URL` / `AXON_SYNTHESIS_OPENAI_MODEL` | OpenAI-compatible endpoint/model when `AXON_LLM_BACKEND=openai-compat`. |
| `AXON_OPENAI_MODEL` | Legacy alias for `AXON_SYNTHESIS_OPENAI_MODEL`. |

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
axon debug "source jobs stuck in pending for 30m"

# Structured output
axon debug "qdrant timeout after restart" --json
```

## Notes

- LLM completions go through the configured LLM backend.
