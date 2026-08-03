---
title: "Reddit Sources"
created: 2026-02-23
updated: 2026-08-02
---

# Reddit Sources

Reddit targets enter the same durable source pipeline as every other source.
The Reddit adapter authenticates with Reddit, materializes a bounded JSON
snapshot, emits `SourceDocument` values, and lets the shared document,
embedding, ledger, vector, graph, and cleanup stages finish the job.

See the [source action reference](../../reference/actions/source.md) for the
shared CLI, REST, and MCP entry points.

## Run it

```bash
# One hot-listing page from a subreddit (at most 100 posts)
axon source r/rust --scope subreddit --wait true

# One post and its comment tree
axon source \
  https://www.reddit.com/r/rust/comments/POST_ID/TITLE \
  --scope thread \
  --wait true
```

The bare-source form is equivalent:

```bash
axon r/rust --scope subreddit --wait true
```

Use `axon jobs get <job-id>` and `axon jobs events <job-id>` to inspect a
detached or failed run.

## Credentials

Create a Reddit script application and put both values in `~/.axon/.env`:

```bash
REDDIT_CLIENT_ID=your_client_id
REDDIT_CLIENT_SECRET=your_client_secret
```

Both values are required. Axon reads them before making a network request and
does not include their values in errors.

## Accepted targets and scopes

| Target | Scope | Behavior |
|---|---|---|
| `r/<subreddit>` | `subreddit` | Fetch the hot listing, limited to one page and 100 posts |
| `reddit.com/r/<subreddit>` | `subreddit` | Same as the shorthand form |
| Reddit post permalink | `thread` | Fetch the post and a comment tree up to depth 10 |

Subreddit names must be 3–21 ASCII letters, digits, or underscores. Thread URLs
must use `reddit.com`, `www.reddit.com`, or `old.reddit.com` and the canonical
`/r/<subreddit>/comments/<id>/...` shape.

The retired Reddit command flags (`--sort`, `--time`, `--max-posts`,
`--min-score`, `--depth`, and `--scrape-links`) are not part of `axon source`.
The current acquisition bounds are fixed in the adapter: hot sort, one page,
100 posts, and comment depth 10.

## Current pipeline behavior

1. The resolver selects the `reddit` adapter and validates the requested scope.
2. The adapter requests an OAuth client-credentials token.
3. It fetches either one hot listing or one thread through Axon's bounded HTTP
   client. A response is capped at 16 MiB and a request at 60 seconds.
4. The adapter maps the response into a temporary prepared dump, discovers its
   manifest items, and records a ledger generation.
5. Each post becomes one `SourceDocument`. Comments are flattened into that
   post's text with reply context; they are not published as separate points.
6. Shared preparation, embedding, vector publication, graphing, and cleanup run
   under the same durable source job id.

The adapter does not expose a separate Reddit job store or direct Qdrant write
path.

## Metadata

Normalized Reddit documents include the shared source metadata plus:

| Field | Meaning |
|---|---|
| `reddit_author` | Post author, or `[deleted]` |
| `reddit_created_utc` | Unix creation timestamp |
| `reddit_score` | Score at acquisition time |
| `reddit_num_comments` | Reported comment count |
| `reddit_upvote_ratio` | Upvote ratio |
| `reddit_subreddit` | Subreddit name |
| `reddit_domain` | Link domain or `self.<subreddit>` |
| `reddit_is_video` | Native-video flag |
| `reddit_distinguished` | Moderator/admin distinction, if present |
| `reddit_gilded` | Gild count |
| `reddit_flair` | Link flair text, if present |
| `reddit_permalink` | Canonical post permalink |
| `reddit_kind` | Reddit thing kind (`t3` for the post document) |

## Failure boundaries

- Missing credentials fail before acquisition.
- Private or quarantined communities may return an authorization error.
- HTTP failures and oversized or malformed responses fail the source job; the
  current adapter does not promise an internal retry policy.
- Deleted/removed comment bodies are omitted by the response mapping.
- Scores and metadata are snapshots; re-run the source to refresh them.
