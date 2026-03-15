# YouTube Ingest
Last Modified: 2026-03-09

> See [`docs/commands/youtube.md`](../commands/youtube.md) for CLI reference and usage examples.

Ingests YouTube video transcripts and metadata into Qdrant via `yt-dlp`. No API key required. Supports single videos, playlists, and channel URLs.

## What Gets Indexed

For each video:

- **Transcript** — auto-generated English subtitles (VTT format), stripped of timestamps, position cues, HTML tags, and deduplicated overlapping lines. Stored at `https://www.youtube.com/watch?v=<ID>`.
- **Description** — the video description text, embedded as a separate Qdrant document at `https://www.youtube.com/watch?v=<ID>?section=description`. Only indexed if non-empty.

### Qdrant Payload Fields

Every chunk (transcript and description) carries the following fields in its Qdrant payload:

#### Standard fields (all ingest sources)

| Field | Type | Value |
|-------|------|-------|
| `url` | string | Canonical video URL or description URL |
| `domain` | string | `www.youtube.com` |
| `source_type` | string | `youtube` |
| `source_command` | string | `youtube` |
| `content_type` | string | `text` |
| `title` | string | Video title (from `.info.json`); falls back to video ID if unavailable |
| `chunk_index` | integer | 0-based index of this chunk within the document |
| `chunk_text` | string | The raw text content of this chunk |
| `scraped_at` | string | RFC3339 timestamp of when the ingest ran |

#### YouTube-specific fields (from `.info.json`)

| Field | Type | Source |
|-------|------|--------|
| `yt_channel` | string | Channel display name |
| `yt_channel_url` | string | Channel URL |
| `yt_uploader_id` | string | Channel handle / uploader ID |
| `yt_upload_date` | string | Upload date (`YYYYMMDD`) |
| `yt_duration` | string | Duration string (e.g. `12:34`) |
| `yt_view_count` | integer \| null | View count at ingest time |
| `yt_like_count` | integer \| null | Like count at ingest time |
| `yt_tags` | string[] | Tag list |
| `yt_categories` | string[] | Category list |

YouTube-specific fields are sourced from the `.info.json` file written by `yt-dlp --write-info-json`. If info.json is missing or unparseable, these fields are omitted — the transcript is still indexed with only the standard fields.

## URL Handling

`extract_video_id()` accepts single-video URLs:

- Full watch URLs: `https://www.youtube.com/watch?v=<ID>`
- Short URLs: `https://youtu.be/<ID>`
- Embed/shorts/v path patterns: `/embed/<ID>`, `/shorts/<ID>`, `/v/<ID>`
- Bare 11-character video IDs

Playlist and channel URLs are detected by `is_playlist_or_channel_url()` and routed to the playlist pipeline (`ingest_youtube_playlist`), not the single-video path:

- `youtube.com/playlist?list=...` (no `v=` param)
- `youtube.com/@handle`
- `youtube.com/c/ChannelName`
- `youtube.com/channel/UCxxx`
- `youtube.com/user/username`

## Prerequisites: yt-dlp

`yt-dlp` must be installed and on `$PATH`.

### Docker (axon-workers container)

Installed automatically in the Dockerfile runtime stage using the standalone binary (no Python required), with arch detection for amd64 and arm64:

```dockerfile
RUN curl -fsSL -o /usr/local/bin/yt-dlp \
  "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux" \
  && chmod +x /usr/local/bin/yt-dlp
```

### Local development

```bash
# Linux standalone binary (no Python required)
curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux \
  -o ~/.local/bin/yt-dlp && chmod +x ~/.local/bin/yt-dlp

# macOS
brew install yt-dlp

# pip (cross-platform, requires Python)
pip install yt-dlp

# Verify
yt-dlp --version
```

## How It Works — Single Video (`ingest_youtube`)

The exact yt-dlp invocation:

```bash
yt-dlp --write-auto-sub --write-info-json --skip-download \
  --sub-format vtt --convert-subs vtt --sub-langs en \
  --no-exec --no-warnings --sleep-requests 1 \
  -o "<tmp>/%(id)s" -- https://www.youtube.com/watch?v=<ID>
```

Pipeline:

1. URL SSRF-validated (private IP ranges blocked)
2. Video ID extracted; URL reconstructed as canonical `watch?v=<ID>`
3. `yt-dlp` downloads `.vtt` subtitle file(s) and `.info.json` to a temp directory
4. `parse_vtt_to_text()` processes each VTT:
   - Strips `WEBVTT` header, timestamp lines, numeric cue identifiers, HTML tags
   - Deduplicates consecutive identical lines (overlapping subtitle windows)
5. All existing Qdrant points for this URL deleted (`qdrant_delete_by_url_filter`) before upsert — prevents orphan chunks on re-ingest
6. Cleaned transcript embedded via `embed_prepared_docs()` → TEI → Qdrant, with YouTube metadata merged into every chunk's payload
7. Description (from `.info.json`) embedded as a separate document at `?section=description`
8. Temp directory cleaned up automatically on drop

## How It Works — Playlist / Channel (`ingest_youtube_playlist`)

```bash
# Phase 1: enumerate all video URLs (single yt-dlp call)
yt-dlp --flat-playlist --print "%(url)s" --no-exec -- <playlist_or_channel_url>

# Phase 2: per-video (up to 5 concurrent)
ingest_youtube(cfg, video_url)   # full single-video pipeline above
```

Pipeline:

1. Prior progress loaded from `result_json` DB column (`completed_urls`, `chunks_embedded`)
2. `yt-dlp --flat-playlist` enumerates all video URLs in the playlist/channel
3. Already-completed video URLs (from prior run) filtered out — enables resume after restart or reclaim
4. Initial progress record written to DB immediately: `[0 / N videos, 0 chunks embedded]`
5. Up to **5 videos processed concurrently** via `FuturesUnordered`
6. On each video completion:
   - `completed_urls` and `chunks_embedded` persisted to DB (enables resume)
   - Progress display updated
7. On **HTTP 429 / Too Many Requests** from yt-dlp: retried up to 3 times with 10s → 20s → 40s backoff. Non-429 errors skip the video and log a warning.
8. `--sleep-requests 1` passed to every per-video yt-dlp invocation — 1s polite delay between YouTube API requests within each subprocess

### Resume Behavior

If a playlist job is killed mid-run and reclaimed as stale, the next run resumes from where it left off. The `completed_urls` list (stored in `result_json` JSONB, ~45 bytes × N videos) is read on startup and used to skip already-indexed videos.

On final completion, `completed_urls` is discarded and the standard `{"chunks_embedded": N}` result is written.

### Performance

| Scenario | Approximate time |
|----------|-----------------|
| 278-video channel, avg 3s/video, sequential (old) | ~23 min |
| 278-video channel, N=5 concurrent (current) | ~3 min |

## Deduplication

All ingest methods (single video and playlist) use `embed_prepared_docs` via `PreparedDoc`, which upserts with deterministic UUID v5 point IDs and then deletes stale tail chunks. This means:

- Re-ingesting a video overwrites it cleanly — no duplicate chunks
- If transcript length changes between runs (e.g. better captions, yt-dlp update), old chunk count is fully replaced
- Safe to re-run playlist ingest: already-done videos in `completed_urls` are skipped at the job level, so no redundant yt-dlp calls either

## Known Limitations

| Limitation | Detail |
|-----------|--------|
| **English captions required** | Only `--sub-langs en` is requested. Fails if no English captions exist. Run `yt-dlp --list-subs <url>` to check. |
| **Age-restricted / private videos** | `yt-dlp` exits non-zero; error surfaces as a job failure (single video) or a per-video skip warning (playlist). |
| **`yt-dlp` version drift** | YouTube format changes periodically require `yt-dlp` updates: `pip install -U yt-dlp` or re-pull the Docker image. |
| **Manual captions not used** | Only `--write-auto-sub` is passed; `--write-subs` (manual captions) is not. If a video has manual captions but no auto-generated ones, it will fail. |
| **Description-only videos** | If a video has no English captions, the transcript embed fails and the description is not embedded either (description embed requires a successful transcript run first). |

## Troubleshooting

**`yt-dlp not found or failed to start`**

Binary not on `$PATH`. See Prerequisites above.

**`yt-dlp produced no VTT subtitle files`**

No English auto-generated captions on this video. Run `yt-dlp --list-subs <url>` to see available languages.

**`yt-dlp exited non-zero`**

Run the yt-dlp command manually to see the raw error:

```bash
yt-dlp --write-auto-sub --write-info-json --skip-download \
  --sub-format vtt --sub-langs en "https://www.youtube.com/watch?v=<ID>"
```

**429 errors on large channels**

Handled automatically with 3-attempt retry (10s/20s/40s backoff). If a video exhausts all retries it is skipped with a warning log; the job continues with the remaining videos.

**Playlist job shows no progress for first video**

Progress (`[0 / N videos]`) is written immediately after enumeration — before the first video completes. If you see no progress at all, the enumeration step (`yt-dlp --flat-playlist`) may still be running.
