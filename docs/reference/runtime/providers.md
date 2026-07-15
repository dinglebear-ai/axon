# Providers
Last Modified: 2026-07-15

Providers are external or local services used by the runtime.

## Provider Families

| Family | Examples |
|---|---|
| embedding | TEI, OpenAI-compatible embedding providers |
| vector store | Qdrant |
| rendering | Chrome/CDP |
| completion | Gemini headless, OpenAI-compatible, Codex app-server |
| search | SearXNG, Tavily |

## Rules

Provider clients must have bounded timeouts, redacted errors, retry policy,
cooling behavior where applicable, and structured health diagnostics.
