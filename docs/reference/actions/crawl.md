# axon crawl
Last Modified: 2026-03-25

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon crawl ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


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
| `--max-pages <n>` | `2000` | Page cap. Set `0` explicitly for uncapped. |
| `--max-depth <n>` | `10` | Maximum crawl depth. |
| `--render-mode <mode>` | `auto-switch` | `http`, `chrome`, `auto-switch`. |
| `--include-subdomains <bool>` | `false` | Include subdomains under the same parent domain. |
| `--budget <PATH=N>` | — | Per-path page cap, repeatable (e.g. `--budget /blog=100 --budget '*=1000'`). `*` = all paths. Unset = no budget. |
| `--etag-conditional` | `false` | Conditional re-crawl: seed spider's ETag cache from `etag.json` so unchanged pages return `304` and are reused (relinked, `changed=false`) instead of re-fetched. Independent of `--cache`. |
| `--warc <PATH>` | — | Write every fetched page to a WARC 1.1 archive at `PATH`. HTTP and Chrome render paths both archive. Round-trips through the crawl job config snapshot. |
| `--automation-script <PATH>` | — | JSON file mapping URL path prefixes → ordered Chrome web-automation steps run before each matching page is captured. Requires `--render-mode chrome`/`auto-switch`; ignored (with a warning) on HTTP-only. |
| `--sitemap-only` | `false` | Sync-only path: run sitemap backfill without full crawl. |
| `--skip-embed` | `false` | Do not queue an embed job from crawl output. |
| `--fresh <Nd>` | — | CLI-only: create or update a recurring freshness schedule, for example `--fresh 1d`. |
| `--json` | `false` | JSON output for job metadata/status responses. |

With `--wait false`, `crawl` writes a SQLite job row and exits without draining
other pending crawl rows. Workers run the same Axon sitemap backfill before
auto-embedding the crawl output, so sitemap-added pages are visible to the
dependent embed job. Use `--wait true` to wait for the submitted crawl and its
explicit dependent embed job, if one is created. Pass `--skip-embed` to crawl
without indexing the output.

Uncapped crawls (`--max-pages 0`) of a root or single-segment path (e.g.
`https://site/` or `https://site/docs`) are rejected unless you also provide an
explicit `--budget` or `--url-whitelist` scope. A deeper start URL (≥2 path
segments) is auto-scoped to its path subtree, so an uncapped crawl of it is
allowed and bounded to that subtree. Set `AXON_ALLOW_UNBOUNDED_BROAD_CRAWL=true`
only for intentional dangerous runs of an unscoped root.
During any crawl, Axon asks Spider to shut down if process RSS reaches
`AXON_CRAWL_MEMORY_ABORT_PERCENT` of the host/cgroup memory limit — the lower of
the two, so inside a container the denominator is the cgroup cap, not host RAM
(default `85`; `0` disables). Linux-only: the guard never trips on other platforms.

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

# Archive every fetched page to a WARC 1.1 file
axon crawl https://example.com --wait true --warc out/example.warc

# Chrome crawl driven by web-automation steps
axon crawl https://example.com --render-mode chrome --automation-script steps.json

# Job status
axon crawl status 550e8400-e29b-41d4-a716-446655440000

# Crawl diagnostics for a job
axon crawl errors 550e8400-e29b-41d4-a716-446655440000

# Enqueue locally and print JSON
axon crawl https://example.com --json

# Keep a docs subtree fresh daily
axon crawl https://modelcontextprotocol.io/docs/getting-started/intro --fresh 1d
```

## Automation-script format

`--automation-script <PATH>` takes a JSON object keyed by URL path prefix. spider
matches each page's URL path against the keys, so `"/"` applies to every page and
`"/blog"` only to pages under `/blog`. Each value is an ordered list of steps run
against the page (during a Chrome render) before it is captured:

```json
{
  "/": [
    { "action": "wait_for", "selector": "main" },
    { "action": "click", "selector": "button.accept-cookies" },
    { "action": "scroll_y", "pixels": 4000 },
    { "action": "wait", "ms": 1500 }
  ],
  "/blog": [
    { "action": "click", "selector": "button.load-more" },
    { "action": "infinite_scroll", "times": 5 }
  ]
}
```

Supported `action` values: `evaluate` (`script`), `click` / `click_all` /
`wait_for` / `wait_for_and_click` (`selector`), `wait` (`ms`),
`wait_for_navigation`, `scroll_x` / `scroll_y` (`pixels`), `infinite_scroll`
(`times`), `fill` (`selector`, `value`), and `screenshot` (`output`,
`full_page`, `omit_background`). Automation requires a Chrome render path; it is
skipped with a warning when `--render-mode http` is in effect.

## Behavior Notes

- `--warc` and `--automation-script` paths resolve on the **worker's** filesystem. Unlike `--output-dir`, they are not container-path-normalized, so for async crawls claimed by a dockerized worker a host path resolves inside the container. Use them with `--wait true` (or `axon serve`/`axon mcp` running on the same host) when pointing at host paths.
- Async mode prints one job ID per URL and returns immediately.
- `--fresh` is CLI-only in v1. It stores a safe replay snapshot and scheduled runs enqueue normal crawl jobs through the service layer; REST/MCP freshness management is not exposed yet.
- Freshness schedule creation rejects secret-bearing custom headers such as `Authorization`, `Cookie`, and `X-API-Key` so secrets are not persisted in SQLite history.
- Generic CLI client-to-server forwarding was removed in 5.0.0. `AXON_SERVER_URL` does not route `axon crawl` through HTTP; call the `/v1/crawl` REST route or MCP HTTP endpoint directly when using `axon serve` as a remote service.
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
