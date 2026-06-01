# axon youtube (removed — use `axon ingest`)

Last Modified: 2026-03-09

> **This command has been replaced.** Use [`axon ingest`](ingest.md) instead.
>
> `axon ingest` auto-detects the source type. YouTube URLs, `@handles`, and bare video IDs are recognized automatically.

> For implementation details and troubleshooting see [`docs/guides/ingest/youtube.md`](../../guides/ingest/youtube.md).

## Synopsis

```bash
axon ingest <TARGET> [FLAGS]
axon ingest <SUBCOMMAND> [ARGS]
```

Replace `axon youtube` with `axon ingest` — flags and behavior are identical.

## Prerequisites

`yt-dlp` must be on `PATH`. Install with:

```bash
pip install yt-dlp
# or
brew install yt-dlp
# or
pipx install yt-dlp
```

## Arguments

| Argument | Description |
|----------|-------------|
| `<TARGET>` | YouTube video URL, playlist URL, channel URL, `@handle`, or bare 11-character video ID |

## Auto-detected Target Formats

| Input | Detected as |
|-------|------------|
| `https://www.youtube.com/watch?v=...` | Video |
| `https://www.youtube.com/playlist?list=...` | Playlist |
| `https://www.youtube.com/channel/...` | Channel |
| `https://youtube.com/@handle` or `@handle` | Channel handle |
| `youtu.be/<id>` | Video (short URL) |
| Bare 11-character ID | Video |

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--wait <bool>` | `false` | Block until ingestion completes |
| `--collection <name>` | `axon` | Target Qdrant collection |
| `--json` | `false` | Machine-readable output |

## Job Subcommands

```bash
axon ingest status <job_id>   # show one ingest job
axon ingest cancel <job_id>   # cancel a pending/running job
axon ingest errors <job_id>   # show job error text
axon ingest list              # list recent ingest jobs (last 50)
axon ingest cleanup           # remove failed/canceled + old completed jobs
axon ingest clear             # delete all ingest jobs and purge the queue
axon ingest recover           # reclaim stale/interrupted jobs
axon ingest worker            # run ingest worker inline (blocking)
```

## Required Environment Variables

None required for YouTube. Qdrant (`QDRANT_URL`) and TEI (`TEI_URL`) must be running.

## Examples

```bash
# Video URL
axon ingest "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --wait true

# Channel @handle (auto-expanded)
axon ingest @SpaceinvaderOne

# Bare video ID
axon ingest dQw4w9WgXcQ --wait true

# Playlist
axon ingest "https://www.youtube.com/playlist?list=PLxxxxxx" --wait true

# Job control
axon ingest list
axon ingest status 550e8400-e29b-41d4-a716-446655440000
axon ingest cancel 550e8400-e29b-41d4-a716-446655440000
```

## Migration

```bash
# Before
axon youtube "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --wait true
axon youtube @SpaceinvaderOne

# After
axon ingest "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --wait true
axon ingest @SpaceinvaderOne
```
