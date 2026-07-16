# axon youtube (removed — use `axon <source>`)

Last Modified: 2026-07-14

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon <source>` |
| REST | `POST /v1/sources` |
| MCP | `{ "action": "source" }` |
| Service | `services::source::* via SourceRequest` |

Parity notes: Compatibility source page. Use the unified source action for CLI, REST, and MCP.
<!-- END GENERATED ACTION SURFACES -->


> **This command has been replaced.** Use the unified source command instead.
>
> `axon <source>` auto-detects the source type. YouTube URLs, `@handles`, and
> bare video IDs are recognized automatically.

> For implementation details and troubleshooting see [`docs/guides/ingest/youtube.md`](../../guides/ingest/youtube.md).

## Synopsis

```bash
axon <TARGET> [FLAGS]
axon jobs <SUBCOMMAND> [ARGS]
```

Replace `axon youtube` with `axon <source>`; inspect async work with `axon jobs`.

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
| `--wait <bool>` | `false` | Block until indexing completes |
| `--collection <name>` | `axon` | Target Qdrant collection |
| `--json` | `false` | Machine-readable output |

## Job Subcommands

```bash
axon jobs status <job_id>   # show one source job
axon jobs cancel <job_id>   # cancel a pending/running job
axon jobs list              # list recent jobs
```

## Required Environment Variables

None required for YouTube. Qdrant (`QDRANT_URL`) and TEI (`TEI_URL`) must be running.

## Examples

```bash
# Video URL
axon "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --wait true

# Channel @handle (auto-expanded)
axon @SpaceinvaderOne

# Bare video ID
axon dQw4w9WgXcQ --wait true

# Playlist
axon "https://www.youtube.com/playlist?list=PLxxxxxx" --wait true

# Job control
axon jobs list
axon jobs status 550e8400-e29b-41d4-a716-446655440000
axon jobs cancel 550e8400-e29b-41d4-a716-446655440000
```

## Migration

```bash
# Before
axon youtube "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --wait true
axon youtube @SpaceinvaderOne

# After
axon "https://www.youtube.com/watch?v=dQw4w9WgXcQ" --wait true
axon @SpaceinvaderOne
```
