# axon suggest
Last Modified: 2026-03-03

Version: 1.0.0
Last Updated: 20:30:18 | 03/03/2026 EST

Suggest new documentation URLs to crawl. The command inspects already indexed URLs/domains, prompts an LLM for complementary crawl targets, then filters out already-indexed matches.

## Synopsis

```bash
axon suggest [focus] [FLAGS]
axon suggest --query "<focus>" [FLAGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `[focus]` | Optional focus text for suggestions (also accepted via `--query`). |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `QDRANT_URL` | Qdrant base URL (reads indexed URLs/domains). |
| `AXON_HEADLESS_GEMINI_CMD` | Optional Gemini CLI command. Defaults to `gemini`. |
| `AXON_HEADLESS_GEMINI_MODEL` | Optional Gemini model override for suggestion generation. |

`suggest` uses Qdrant state and Gemini headless synthesis.
For implementation details and troubleshooting see [`docs/ingest/suggest.md`](../ingest/suggest.md)

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--query <text>` | — | Focus text (alternative to positional argument). |
| `--limit <n>` | `10` | Desired number of suggested URLs (clamped to 1..100). |
| `--collection <name>` | `cortex` | Qdrant collection to analyze. |
| `--json` | `false` | Emits `{"suggestions": [{"url": "...", "reason": "..."}]}` JSON object. |

Note: `suggest` runs synchronously and does not enqueue jobs.

## Examples

```bash
# Generic suggestions
axon suggest

# Focus on a topic
axon suggest "refresh scheduler internals"

# Ask for more candidates
axon suggest "MCP server operations" --limit 20

# JSON output
axon suggest "qdrant filtering docs" --json
```

Plain-text output prints one suggestion per line as:

```text
https://example.com/docs<TAB>Reason from model
```

## Tuning Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `AXON_SUGGEST_BASE_URL_LIMIT` | `250` | Domain facet sample size for prompt context. |
| `AXON_SUGGEST_EXISTING_URL_LIMIT` | `500` | Max indexed URLs included in the LLM prompt. |
| `AXON_SUGGEST_INDEX_LIMIT` | `50000` | Max indexed URLs loaded for duplicate filtering. |

## Notes

- `suggest` requires existing indexed content. If collection is empty, it errors with `No indexed URLs found in Qdrant collection; run crawl/scrape first`.
- Only absolute `http/https` suggestions are accepted.
- Already-indexed URL variants are filtered out before final output.
- Suggestion reasons are preserved through the vector suggestion parser, service layer, CLI JSON output, and MCP-facing service contract.
