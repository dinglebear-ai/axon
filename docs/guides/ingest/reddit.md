# Reddit Ingest
Last Modified: 2026-03-09

Version: 1.0.0
Last Updated: 01:26:53 | 02/25/2026 EST

> CLI reference (flags, subcommands, examples): [`docs/reference/actions/ingest.md`](../../reference/actions/ingest.md)

Ingests subreddit posts and comment threads into Qdrant via the Reddit OAuth2 API (client credentials flow — no user login required).

## What Gets Indexed

- **Posts**: title + selftext body (for text posts); title + URL (for link posts)
- **Comments**: full thread up to `--depth` levels deep, filtered by `--min-score`
- Deleted (`[deleted]`) and removed (`[removed]`) comments are skipped
- Post metadata: score, flair, author, timestamp

## Prerequisites

A Reddit **script app** with client credentials. Both `REDDIT_CLIENT_ID` and `REDDIT_CLIENT_SECRET` are required — the command fails immediately if either is missing.

1. Go to [reddit.com/prefs/apps](https://www.reddit.com/prefs/apps) → **"create another app"**
2. Select **"script"** type, set redirect URI to `http://localhost:8080`
3. Copy the **client ID** (displayed under the app name) and **client secret**

```bash
# ~/.axon/.env
REDDIT_CLIENT_ID=your_client_id
REDDIT_CLIENT_SECRET=your_client_secret
```

## How It Works

1. Validates subreddit names and canonicalizes thread targets before constructing Reddit API URLs. Accepted thread inputs are `reddit.com`, `www.reddit.com`, `old.reddit.com`, or `/r/<subreddit>/comments/<id>/...` permalink-style paths. Non-Reddit `/comments/` URLs are rejected.
2. Authenticates via Reddit OAuth2 client credentials, obtaining a bearer token
3. Fetches posts from `https://oauth.reddit.com/r/<subreddit>/<sort>?limit=100`; paginates until `--max-posts` reached
4. For each post, fetches the comment tree at `https://oauth.reddit.com<permalink>.json?limit=100&depth=<n>`
5. Recursively traverses comments up to `--depth` levels, skipping entries below `--min-score`
6. Posts and comments embedded via `embed_prepared_docs()` → TEI → Qdrant

**User-Agent:** `axon-ingest/1.0 by /u/axon_bot` (Reddit requires a descriptive UA string per their API terms)

## Rate Limits

Reddit OAuth2 script apps are allowed 100 requests/minute. On 429 responses, the ingest worker honors numeric `Retry-After` headers up to a 60s cap, then falls back to exponential backoff (2s, 4s, 8s; max 3 retries) when the header is missing or invalid.

## Cancellation and Partial Comment Failures

The Reddit source has source-local cancellation hooks that can be wired by job/service layers. Checks occur before page fetches, before comment fetches, and during retry backoff sleeps.

Subreddit ingest treats per-post comment fetch failures as partial failures: the post can still be embedded, while `comment_fetch_attempts` and `comment_fetch_failures` are tracked in Reddit ingest stats and emitted through the source progress payload.

## Qdrant Metadata Fields

All Reddit post chunks carry structured `reddit_*` payload fields built in `src/ingest/reddit/meta.rs`. Fields are sourced from the `post["data"]` object in the Reddit API JSON response.

| Field | Type | Description |
|-------|------|-------------|
| `reddit_author` | `string` | Post author username; `"[deleted]"` if account removed |
| `reddit_created_utc` | `integer` | Post creation time as a Unix timestamp (UTC) |
| `reddit_score` | `integer` | Net upvote score at index time |
| `reddit_num_comments` | `integer` | Total comment count at index time |
| `reddit_upvote_ratio` | `float` | Upvote ratio (0.0–1.0) at index time |
| `reddit_subreddit` | `string` | Subreddit name (without `r/` prefix) |
| `reddit_domain` | `string` | Link domain for link posts; `"self.<subreddit>"` for text posts |
| `reddit_is_video` | `boolean` | Whether the post is a native Reddit video |
| `reddit_distinguished` | `string \| null` | Distinguishment status (`"moderator"`, `"admin"`, or `null`) |
| `reddit_gilded` | `integer` | Number of times the post has been gilded |
| `reddit_flair` | `string \| null` | Post flair text; `null` if no flair set |

## Known Limitations

| Limitation | Detail |
|-----------|--------|
| **Link posts** | Only title + URL indexed; no external page content. Use `axon crawl` for the linked URL |
| **Comment depth limits** | Reddit's API can truncate very deep threads before `--depth` is reached |
| **Private / quarantined subreddits** | Client credentials flow cannot access these; fails with 403 |
| **Score freshness** | Scores captured at index time; not updated on re-index |
| **Target validation** | Subreddit names are 3–21 chars, alphanumeric + underscore only. Thread URLs must be on `reddit.com`, `www.reddit.com`, or `old.reddit.com`, or use `/r/.../comments/...` permalink form |

## Troubleshooting

**`invalid subreddit name`**

Name contains invalid characters or wrong length. Remove any `r/` prefix.

**`401 Unauthorized`**

Wrong `REDDIT_CLIENT_ID` or `REDDIT_CLIENT_SECRET`. Verify in `~/.axon/.env` and confirm the app type is **"script"** on reddit.com/prefs/apps.

**`403 Forbidden`**

Subreddit is private or quarantined — not accessible with client credentials.

**Rate limit / 429 errors**

Handled automatically with exponential backoff retries. If errors persist, reduce request rate and verify app health on Reddit.
