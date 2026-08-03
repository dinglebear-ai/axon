---
title: "YouTube Sources"
created: 2026-02-23
updated: 2026-08-02
---

# YouTube Sources

The YouTube adapter acquires transcript and video metadata with `yt-dlp`, emits
one `SourceDocument` per usable video transcript, and hands those documents to
the shared source ledger, preparation, embedding, vector, graph, and cleanup
pipeline.

See the [source action reference](../../reference/actions/source.md) for the
shared CLI, REST, and MCP entry points.

## Run it

Single-video URLs are recognized by the public resolver:

```bash
axon source https://www.youtube.com/watch?v=VIDEO_ID \
  --scope video \
  --wait true
```

Short `youtu.be` URLs resolve to the same canonical video source. The adapter
also implements playlist and channel acquisition, but the current lexical
resolver only classifies single-video YouTube URLs without a preconfigured
authority mapping. Treat playlist/channel submission as unavailable through a
plain public `axon source` call until that resolver boundary is extended.

Use `axon jobs get <job-id>` and `axon jobs events <job-id>` to inspect a
detached or failed run.

## Prerequisite

`yt-dlp` must be on `PATH`. Override the binary with `AXON_YTDLP` or `YT_DLP`.

```bash
yt-dlp --version
```

No YouTube API key is required.

## Accepted video forms

The adapter parser accepts:

- `https://www.youtube.com/watch?v=<id>`
- `https://youtu.be/<id>`
- YouTube `/embed/<id>`, `/shorts/<id>`, and `/v/<id>` URLs
- a bare 11-character video id once the request has already been routed to the
  YouTube adapter

The canonical source URI is `youtube://video/<id>` and the published document
URI is `https://www.youtube.com/watch?v=<id>`.

## Current pipeline behavior

1. The resolver selects the `youtube` adapter for a recognized video URL.
2. The adapter validates the canonical HTTPS URL with the source URL security
   policy.
3. `yt-dlp` downloads English auto-subtitles and `.info.json` into a bounded
   temporary workspace. The command has a five-minute timeout; subtitle files
   larger than 50 MiB are skipped.
4. VTT cues, timestamps, markup, and repeated adjacent lines are normalized
   into transcript text.
5. A video with no usable transcript produces no document. If the target
   yields no usable videos, the source job fails clearly.
6. Each usable transcript becomes one `SourceDocument`; shared preparation
   chunks it and the normal embedding/vector stages publish the chunks.

Descriptions are captured in the prepared `yt-dlp` dump but are not currently
published as a second description document. There is no direct YouTube-to-
Qdrant path and no separate YouTube job lifecycle.

The effective single-video `yt-dlp` shape is:

```bash
yt-dlp --write-auto-sub --write-info-json --skip-download \
  --sub-format vtt --convert-subs vtt --sub-langs en \
  --no-exec --no-warnings --sleep-requests 1 \
  -o '<temporary-dir>/%(id)s' -- \
  'https://www.youtube.com/watch?v=<id>'
```

## Metadata

Normalized YouTube documents include shared source metadata plus:

| Field | Meaning |
|---|---|
| `video_id` | YouTube video id |
| `title` | Video title |
| `media_url` | Canonical watch URL |
| `channel` | Channel display name, when present |
| `channel_url` | Channel URL, when present |
| `yt_uploader_id` | Uploader/channel identifier |
| `yt_upload_date` | Upload date from `yt-dlp` |
| `yt_duration` | Display duration |
| `yt_view_count` | View count at acquisition time |
| `yt_like_count` | Like count at acquisition time |
| `yt_tags` | Tag list |
| `yt_categories` | Category list |
| `yt_thumbnail` | Thumbnail URL |

## Failure boundaries

- Missing or outdated `yt-dlp`, private/age-restricted videos, and subprocess
  failures fail the source job.
- Only English auto-subtitles are requested. Manual-only or non-English-only
  caption sets currently produce no document.
- Metadata counts are snapshots; re-run the source to refresh them.
- The adapter does not promise per-video retries. A failed `yt-dlp` invocation
  is surfaced rather than silently retried.
