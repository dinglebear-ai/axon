# axon summarize
Last Modified: 2026-05-19

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon summarize ...` |
| REST | `POST /v1/summarize` (Implemented) |
| MCP | `{ "action": "summarize" }` |
| Service | `services::summarize::summarize` |

Parity notes: Supports render mode, selectors, and headers for the underlying scrape step.
<!-- END GENERATED ACTION SURFACES -->


Scrape one or more URLs as markdown, inject the scraped context into the configured LLM backend, and return a brief summary.

## Synopsis

```bash
axon summarize <url>...
axon summarize --urls "https://a.example,https://b.example"
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<url>...` | One or more URLs to scrape and summarize |

## Flags

All global scrape/config flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--render-mode <mode>` | `auto-switch` | Fetch mode: `http`, `chrome`, or `auto-switch`. |
| `--urls <csv>` | — | Comma-separated URL list. |
| `--url-glob <glob>` | — | Expand URL patterns before summarizing. |
| `--root-selector <selector>` | — | Scope page extraction before summarization. |
| `--exclude-selector <selector>` | — | Remove elements before summarization. |
| `--json` | `false` | Emit the typed `SummarizeResult` payload. |

## Behavior

- Runs synchronously.
- Uses `services::summarize::summarize`, which is shared by CLI, REST, and MCP.
- Scrapes pages through the normal scrape service with markdown output.
- Does not hardcode a model; the request goes through `services::llm_backend` and the configured LLM backend. The default is Gemini headless; `AXON_LLM_BACKEND=openai-compat` uses an OpenAI-compatible chat-completions endpoint.
- Treats scraped page content as untrusted context in the LLM prompt.
- Accepts at most 10 URLs per request.
- Generic CLI client-to-server forwarding was removed in 5.0.0. `AXON_SERVER_URL` does not route `axon summarize` through HTTP; call the `/v1/summarize` REST route or MCP HTTP endpoint directly when using `axon serve` as a remote service.

## Examples

```bash
axon summarize https://example.com
axon summarize https://a.example https://b.example
axon summarize --urls "https://a.example,https://b.example" --json
axon summarize https://example.com --json
```

## API And MCP

REST:

```http
POST /v1/summarize
{ "url": "https://example.com" }
```

MCP/action API:

```json
{ "action": "summarize", "url": "https://example.com" }
{ "action": "summarize", "urls": ["https://a.example", "https://b.example"] }
```
