# axon extract
Last Modified: 2026-03-03

Version: 1.0.0
Last Updated: 20:29:46 | 03/03/2026 EST

Extract structured data from one or more URLs using deterministic parsers with LLM fallback. Supports async job mode (default) and synchronous inline extraction (`--wait true`).

## Synopsis

```bash
axon extract <url>... --query "<prompt>" [FLAGS]
axon extract --urls "<url1>,<url2>" --query "<prompt>" [FLAGS]
axon extract <SUBCOMMAND> [ARGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<url>...` | One or more URLs to extract from |

## Required Inputs

- At least one URL via positional args, `--urls`, or `--url-glob`. Extract errors out if no URL is supplied.
- `--query <prompt>` is strongly recommended. It is not enforced by the parser — an empty prompt falls back to deterministic parsers only and skips the LLM extraction pass — but most useful extractions need a prompt.

## Job Subcommands

```bash
axon extract status <job_id>
axon extract cancel <job_id>
axon extract errors <job_id>
axon extract list
axon extract cleanup
axon extract clear
axon extract worker
axon extract recover
```

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--query <text>` | — | Extraction prompt (recommended; empty prompt skips the LLM pass). |
| `--wait <bool>` | `false` | `false`: enqueue extract job. `true`: run extraction inline and block. |
| `--max-pages <n>` | `1` when omitted, `0` when explicitly uncapped | Passed to extract web runner as crawl/page limit. |
| `--output-dir <dir>` | `.cache/axon-rust/output` | Base path for extract artifacts. |
| `--output <path>` | — | Summary JSON output path (sync mode). |
| `--json` | `false` | JSON output for enqueue/status and sync summary. |

LLM fallback extraction runs through the Gemini headless backend. Configure the
model with `AXON_HEADLESS_GEMINI_MODEL` and the binary with
`AXON_HEADLESS_GEMINI_CMD` — see [`docs/guides/configuration.md`](../../guides/configuration.md). The legacy
`--openai-*` flags and `OPENAI_*` env vars were removed in 3.0.0.

## Examples

```bash
# Async extract job (default)
axon extract https://example.com/pricing --query "extract plan names and monthly prices"

# Synchronous extraction
axon extract https://example.com/docs --query "extract API endpoints" --wait true

# Multiple URLs from CSV
axon extract --urls "https://a.dev,https://b.dev" --query "extract contact info"

# Job status
axon extract status 550e8400-e29b-41d4-a716-446655440000

# Enqueue through the canonical server
AXON_SERVER_URL=http://127.0.0.1:8001 axon extract https://example.com --query "extract title" --json
```

## Sync Output Artifacts

When `--wait true`, extract writes:
- Summary JSON: `<output-dir>/extract-summary.json` (or `--output` path)
- NDJSON items: `<output-dir>/extract-items.ndjson`

Summary includes page counts, deterministic vs LLM fallback counts, token usage, parser hit counts, and per-run stats.

## Behavior Notes

- Async mode returns immediately with a job ID.
- In server mode (`AXON_SERVER_URL`), extract submit and lifecycle subcommands call `axon serve`; `--wait true` polls server job state and does not spawn host-local workers.
- `clear` is destructive and prompts unless `--yes` is passed.
- `extract` runs URLs concurrently in sync mode and aggregates metrics across runs.
