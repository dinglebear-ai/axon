# YouTube Ingest
Last Modified: 2026-05-03

> See [`docs/reference/commands/youtube.md`](../../reference/commands/youtube.md) for CLI reference and usage examples.

Ingests YouTube video transcripts and metadata into Qdrant via `yt-dlp`. No API key required. The source layer supports single videos, playlists, and channel URLs.

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

Playlist and channel URLs are detected by `is_playlist_or_channel_url()` and routed by `ingest_youtube_target()` to the playlist/channel pipeline (`ingest_youtube_playlist`), not the single-video path:

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

## How It Works — Playlist / Channel (`ingest_youtube_target` / `ingest_youtube_playlist`)

```bash
# Phase 1: enumerate all video URLs (single yt-dlp call)
yt-dlp --flat-playlist --print "%(url)s" --playlist-end 500 --no-exec -- <playlist_or_channel_url>

# Phase 2: per-video
ingest_youtube(cfg, "https://www.youtube.com/watch?v=<ID>")
```

Pipeline:

1. Target dispatch classifies the input as either a single video or playlist/channel. Raw `@handle` targets are normalized to `https://www.youtube.com/@handle`.
2. `yt-dlp --flat-playlist` enumerates up to 500 rows from the playlist/channel.
3. Each enumeration row is canonicalized through `extract_video_id()` to `https://www.youtube.com/watch?v=<ID>`.
4. Empty or invalid enumeration rows are skipped with warnings.
5. Initial progress is reported as `videos_done=0`, `videos_total=N`, `chunks_embedded=0`.
6. Valid videos are processed sequentially through the single-video `ingest_youtube()` pipeline.
7. After each video, progress is reported with updated `videos_done`, `videos_total`, and cumulative `chunks_embedded`.
8. Per-video failures are logged as warnings and the playlist/channel ingest continues with the remaining videos.

## Deduplication

All ingest methods (single video and playlist) use `embed_prepared_docs` via `PreparedDoc`, which upserts with deterministic UUID v5 point IDs and then deletes stale tail chunks. This means:

- Re-ingesting a video overwrites it cleanly — no duplicate chunks
- If transcript length changes between runs (e.g. better captions, yt-dlp update), old chunk count is fully replaced
- Safe to re-run playlist ingest: deterministic point IDs overwrite existing chunks cleanly, though source-side playlist processing still calls `yt-dlp` for each enumerated video.

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

Playlist/channel ingest logs the per-video failure and continues with remaining videos. Update `yt-dlp`, reduce target size, or retry later if YouTube rate-limits the run.

**Playlist job shows no progress for first video**

Progress (`[0 / N videos]`) is written immediately after enumeration — before the first video completes. If you see no progress at all, the enumeration step (`yt-dlp --flat-playlist`) may still be running.
