# Export Backup Contract
Last Modified: 2026-03-20

Version: 1.0.0
Last Updated: 21:07:52 | 03/20/2026 EDT

Axon export is the canonical backup artifact for rebuilding the knowledge base.

## Purpose

The export is designed for **fast rebuild**, not raw archival of every indexed URL.

It captures:
- Runtime-safe settings and queue/config defaults (`settings_snapshot`)
- Seed data required to replay ingest/crawl/extract/search/research/scrape workflows (`rebuild_seeds`)
- Scheduling state (`refreshes.schedules`, `watches`)
- Integrity metadata (counts + hashes) for drift checks

It does **not** include:
- Giant URL inventories like per-page crawl dumps or GitHub issue/PR URL lists
- `qdrant_summary.indexed_urls` (removed intentionally)

## Current Schema

Current manifest version is `3`.

Top-level keys:
- `version`
- `exported_at`
- `collection`
- `metadata`
- `settings_snapshot`
- `integrity`
- `rebuild_seeds`
- `crawls`
- `scrapes`
- `extractions`
- `embeds`
- `ingests`
- `refreshes`
- `watches`
- `qdrant_summary`

`qdrant_summary` fields:
- `total_points`
- `source_type_counts`
- `domain_counts`

## Seed Data (`rebuild_seeds`)

`rebuild_seeds` includes:
- `crawl_seed_urls`
- `scrape_urls`
- `scrape_requests` (with `request_id`, `created_at`, `url`, `options`)
- `github_repos` (repo slugs only, e.g. `owner/repo`)
- `github_requests` (with request metadata + options)
- `reddit_targets`
- `youtube_targets`
- `session_targets`
- `local_paths`
- `extraction_requests` (with request metadata + prompt + config)
- `search_requests` / `research_requests` (with request metadata + options)
- `search_queries` / `research_queries`

Dedup rules:
- `crawl_seed_urls`, `scrape_urls`, `github_repos`, `reddit_targets`, `youtube_targets`, `session_targets`, `local_paths`, `search_queries`, `research_queries` are trimmed, sorted, and deduplicated.
- `search_requests` / `research_requests` dedupe on `(query, options)`.
- `scrape_requests` dedupe on `(url, options)`.
- `github_requests` dedupe on `(target, options)`.

## Seed-Only vs History

Default export mode is **seed-only**.

Seed-only mode:
- Includes `rebuild_seeds`, `settings_snapshot`, `metadata`, `integrity`
- Includes `refreshes.schedules` and `watches` (always)
- Omits history-heavy sections by default (`crawls`, `scrapes`, `extractions`, `embeds`, `ingests`, `refreshes.jobs`)

Use `--include-history` to include historical job sections.

Compatibility behavior:
- If `axon_query_history` does not exist yet, query seed sections export as empty lists.
- If `axon_scrape_seeds` does not exist yet, scrape seed requests export as empty lists.

## CLI

```bash
# Seed-only backup (default)
axon export --output .cache/axon-rust/output/backup.json

# Include historical job sections
axon export --include-history --output .cache/axon-rust/output/backup-full.json

# Verify schema + integrity before restore
axon export verify .cache/axon-rust/output/backup.json
```

If `--output` is omitted, CLI writes to `axon-export-YYYYMMDD-HHMMSS.json` in the current working directory.

## MCP

`action=export` accepts:
- `include_history` (optional, default `false`)
- `response_mode` (optional)

## Integrity

`integrity` provides:
- `counts` by seed category
- SHA-256 `hashes` for canonical sorted seed lists

Use it to verify:
- Export consistency between runs
- Restore completeness after replay

## Restore Guidance (Recommended Order)

1. Apply `settings_snapshot`
2. Recreate `refreshes.schedules` and `watches`
3. Replay seed sources (`crawl_seed_urls`, `github_repos`, `scrape_requests`, etc.)
4. Replay extraction/query seeds (`extraction_requests`, `search_requests`, `research_requests`)
5. Compare resulting seed counts/hashes against `integrity`

## Notes

- `qdrant_summary.indexed_urls` was intentionally removed to avoid noisy, non-rebuild payload bloat.
- Scrape seed requests are tracked in Postgres table `axon_scrape_seeds`.
