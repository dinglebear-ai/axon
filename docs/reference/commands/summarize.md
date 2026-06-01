# axon summarize
Last Modified: 2026-05-19

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
- Does not hardcode a model; the request goes through `services::llm_backend` and the configured headless LLM settings. The current implementation supports Gemini CLI.
- Treats scraped page content as untrusted context in the LLM prompt.
- Accepts at most 10 URLs per request.
- In server mode (`AXON_SERVER_URL`), the CLI calls the direct `POST /v1/summarize` route.

## Examples

```bash
axon summarize https://example.com
axon summarize https://a.example https://b.example
axon summarize --urls "https://a.example,https://b.example" --json
AXON_SERVER_URL=http://127.0.0.1:8001 axon summarize https://example.com --json
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
