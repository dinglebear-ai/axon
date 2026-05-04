# axon crawl
Last Modified: 2026-03-25

Site crawl command with async job mode (default) and synchronous inline mode (`--wait true`). Supports crawl job lifecycle subcommands (`status`, `cancel`, `errors`, `list`, `cleanup`, `clear`, `worker`, `recover`, `audit`, `diff`).

## Synopsis

```bash
axon crawl <url>... [FLAGS]
axon crawl --urls "<url1>,<url2>" [FLAGS]
axon crawl <SUBCOMMAND> [ARGS]
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<url>...` | One or more crawl start URLs |

## URL Input Rules

- At least one URL is required via positional args, `--urls`, or `--url-glob`.
- `--start-url` is global but not used by `crawl`.
- URL inputs are normalized and deduplicated before enqueue/run.

## Job Subcommands

```bash
axon crawl status <job_id>
axon crawl cancel <job_id>
axon crawl errors <job_id>
axon crawl list
axon crawl cleanup
axon crawl clear
axon crawl worker
axon crawl recover
axon crawl audit <url>
axon crawl diff
```

## Flags

All global flags apply. Key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--wait <bool>` | `false` | `false`: enqueue crawl jobs and return. `true`: run crawl inline and block. |
| `--max-pages <n>` | `0` | Page cap (`0` = uncapped). |
| `--max-depth <n>` | `5` | Maximum crawl depth. |
| `--render-mode <mode>` | `auto-switch` | `http`, `chrome`, `auto-switch`. |
| `--discover-sitemaps <bool>` | `true` | Enable sitemap discovery/backfill. |
| `--sitemap-since-days <n>` | `0` | Restrict sitemap backfill by `<lastmod>` age. |
| `--include-subdomains <bool>` | `false` | Include subdomains under the same parent domain. |
| `--respect-robots <bool>` | `false` | Respect `robots.txt` directives. |
| `--min-markdown-chars <n>` | `200` | Thin-page threshold. |

Lite mode preserves fire-and-forget semantics for `--wait false`: `crawl` enqueues and exits without draining other pending crawl rows. Use `--wait true` to run the submitted crawl inline and wait for its explicit dependent embed job, if one is created.
| `--drop-thin-markdown <bool>` | `true` | Skip thin pages. |
| `--sitemap-only` | `false` | Sync-only path: run sitemap backfill without full crawl. |
| `--embed <bool>` | `true` | Queue embed job from crawl output. |
| `--json` | `false` | JSON output for job metadata/status responses. |

## Examples

```bash
# Default async mode (enqueue)
axon crawl https://example.com

# Multiple start URLs
axon crawl --urls "https://docs.rs,https://tokio.rs"

# Synchronous crawl
axon crawl https://example.com --wait true

# Chrome-only crawl with custom limits
axon crawl https://example.com --render-mode chrome --max-pages 200 --max-depth 3

# Job status
axon crawl status 550e8400-e29b-41d4-a716-446655440000
```

## Behavior Notes

- Async mode prints one job ID per URL and returns immediately.
- Async JSON output now includes the predicted `output_dir` plus `predicted_paths` for each enqueued job.
- Sync mode writes crawl artifacts under `<output-dir>/domains/<domain>/sync/`.
- Completed crawl status JSON may include `output_files` when the worker has a manifest-backed file list available.
- `--render-mode auto-switch` now treats one- and two-page HTTP crawls as too little signal and may retry in Chrome even when the pages are not technically thin.
- Malformed discovered URLs are filtered before they enter the accepted result set, which keeps crawl/page counts aligned with canonical URLs instead of raw Spider candidates.
- `clear` is destructive and prompts unless `--yes` is passed.
- URLs that look like local filenames (for example `README.md` as host) trigger a warning and are still treated as web URLs.
