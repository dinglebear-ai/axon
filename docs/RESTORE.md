# Restore Guide
Last Modified: 2026-03-20

Version: 1.0.0
Last Updated: 22:10:00 | 03/20/2026 EDT

Use this with [EXPORT.md](./EXPORT.md) backup files.

## Preconditions

- Axon services are up (`postgres`, `redis`, `rabbitmq`, `qdrant`, workers).
- You have a seed-focused export file (default `axon export` output).

## 1) Verify Backup Before Restore

```bash
# Human-readable
axon export verify /path/to/backup.json

# Machine-readable
axon --json export verify /path/to/backup.json
```

Do not restore from a file that fails verify.

## 2) Recommended Replay Order

1. Apply `settings_snapshot` values to `.env` / runtime config.
2. Recreate `refreshes.schedules` and `watches`.
3. Replay crawl seeds (`rebuild_seeds.crawl_seed_urls`).
4. Replay ingest source seeds (`github_repos`, `reddit_targets`, `youtube_targets`, `session_targets`, `local_paths`).
5. Replay scrape + extraction/query seeds (`scrape_requests`, `extraction_requests`, `search_requests`, `research_requests`).
6. Validate restored integrity against export `integrity` hashes/counts.

## 3) Replay Examples

```bash
# Crawl seeds
axon crawl https://docs.rs --wait true

# GitHub seed
axon ingest rust-lang/rust --wait true

# Scrape seed
axon scrape https://example.com --wait true

# Extraction seed
axon extract https://example.com --prompt "extract key facts" --wait true

# Query/research seed
axon search "rust async streams"
axon research "qdrant sparse+dense hybrid retrieval"
```

## Notes

- Seed-only export intentionally excludes full crawl fanout URL inventories.
- Verify catches version drift, missing required keys, and integrity mismatches before replay.
