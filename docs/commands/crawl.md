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
| `--max-depth <n>` | `10` | Maximum crawl depth. |
| `--render-mode <mode>` | `auto-switch` | `http`, `chrome`, `auto-switch`. |
| `--include-subdomains <bool>` | `false` | Include subdomains under the same parent domain. |
| `--sitemap-only` | `false` | Sync-only path: run sitemap backfill without full crawl. |
| `--skip-embed` | `false` | Do not queue an embed job from crawl output. |
| `--json` | `false` | JSON output for job metadata/status responses. |

With `--wait false`, `crawl` writes a SQLite job row and exits without draining
other pending crawl rows. Workers run the same Axon sitemap backfill before
auto-embedding the crawl output, so sitemap-added pages are visible to the
dependent embed job. Use `--wait true` to wait for the submitted crawl and its
explicit dependent embed job, if one is created. Pass `--skip-embed` to crawl
without indexing the output.

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

# Crawl diagnostics for a job
axon crawl errors 550e8400-e29b-41d4-a716-446655440000

# Enqueue through the canonical server
AXON_SERVER_URL=http://127.0.0.1:8001 axon crawl https://example.com --json
```

## Behavior Notes

- Async mode prints one job ID per URL and returns immediately.
- In server mode (`AXON_SERVER_URL`), crawl submit and lifecycle subcommands call `axon serve`; `--wait true` polls server job state and does not spawn host-local workers. Use `--local` to force the local behavior.
- Async JSON output now includes the predicted `output_dir` plus `predicted_paths` for each enqueued job.
- Sync mode writes crawl artifacts under `<output-dir>/domains/<domain>/sync/`.
- With `scrape.discover-sitemaps = true` in `config.toml`, both async worker mode and sync `--wait true` mode run Axon's sitemap backfill before the embed handoff. Sync mode performs the embed inline; async mode queues a dependent embed job after backfill completes.
- With `scrape.discover-llms-txt = true` (default), the backfill pass also probes `/llms.txt` at the site root, parses its markdown links, host-scopes them, and **merges** them (deduped) into the same backfill candidate set as sitemap discovery. The cap `scrape.max-llms-txt-urls` (default 512) bounds the llms.txt fan-out only — sitemap-URL backfill stays uncapped. Raw `.md`/`.markdown`/`.txt` targets are stored verbatim (no HTML→markdown transform).
- Completed crawl status JSON may include `output_files` when the worker has a manifest-backed file list available.
- `axon crawl errors <job_id>` reports `error_text`, page-error aggregates, WAF-blocked counts, sitemap backfill errors, and bounded diagnostic samples from `result_json.diagnostics`. Samples are capped so a large crawl cannot grow the SQLite row without bound.
- `--render-mode auto-switch` now treats one- and two-page HTTP crawls as too little signal and may retry in Chrome even when the pages are not technically thin.
- Malformed discovered URLs are filtered before they enter the accepted result set, which keeps crawl/page counts aligned with canonical URLs instead of raw Spider candidates.
- `clear` is destructive and prompts unless `--yes` is passed.
- URLs that look like local filenames (for example `README.md` as host) trigger a warning and are still treated as web URLs.
