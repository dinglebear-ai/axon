---
title: "Reddit Ingest"
created: 2026-02-23
updated: 2026-09-30
---

# Reddit Ingest

Last reviewed: 2026-09-29

The Reddit adapter acquires a bounded OAuth API snapshot and emits normalized
documents through shared preparation, embedding, and publication. There is
no independent Reddit queue or vector writer.

```bash
axon r/rust --wait true
axon https://www.reddit.com/r/rust/comments/POST_ID/title/ --wait true
```

Use a real authorized target instead of `POST_ID`. See the
[scope registry](../../reference/sources/adapter-scopes.md).

## Credentials and request boundaries

Both `REDDIT_CLIENT_ID` and `REDDIT_CLIENT_SECRET` must be nonblank in the
executing runtime. [Acquisition](../../../crates/axon-adapters/src/reddit/acquire.rs)
resolves them before network I/O and uses a client-credentials OAuth grant.
No user login occurs, so access requiring a particular signed-in user must
not be assumed. Keep credentials out of logs and source control.

The shared client uses a 60-second request timeout. Listing/thread GET
bodies are capped at 16 MiB, including while streaming. These are per-request
constraints, not a whole-job deadline or proof of complete data.

## What is fetched

| Target | Current acquisition | Completeness |
|---|---|---|
| Subreddit | One hot listing requesting 100 posts | Not every post or every comment thread |
| Thread | One response requesting limit 100 and depth 10 | Only returned comments; missing/deeper children are not exhaustively expanded |

The implementation does not follow subreddit pagination cursors, fetch all
listing comment trees, or expand every `more` placeholder. Old claims of
arbitrary sort/depth controls, full-archive pagination, or a fixed account-wide
rate allowance were not descriptions of this adapter.

Acquisition reports HTTP error status directly. Do not rely on the previous
429 Retry-After/three-retry backoff description. Follow actual provider/job
recovery guidance instead of immediately replaying a failed request.

## Normalization and metadata

The [adapter](../../../crates/axon-adapters/src/reddit.rs) owns a temporary
prepared snapshot. The [dump parser](../../../crates/axon-adapters/src/reddit/dump.rs)
renders post/comment content; linked external pages are not fetched
automatically. Submit linked URLs separately through the web source path.

[Metadata](../../../crates/axon-adapters/src/reddit/metadata.rs) includes
approved author, timestamp, score, comment count, subreddit, domain, video,
flair, permalink, and kind fields, plus shared identity/generation/visibility.
Values describe the snapshot, not live Reddit state. Optional/deleted fields
follow the implementation defaults and null handling.

Use [metadata payload](../../reference/sources/metadata-payload.md) for shared
rules. Do not assume a changed score alone forces content re-embedding or
that arbitrary old metadata survives normalization.

## Refresh and completion

Reddit manifests are explicitly **partial**. Absence from the next hot
listing is not a deletion. Stable identities and hashes select changed
items while unchanged items can be retained. A bounded listing must not
remove every unseen source item.

```bash
axon jobs get <job_id> --json
axon jobs events <job_id> --json
```

A job ID is acceptance, not completion; a wait timeout is not cancellation.
Inspect terminal results and distinguish credential denial, provider rate
limits, malformed/oversized responses, partial inventory, and publication
failure. Preserve source/job IDs and completed-stage evidence before retrying.

Tests include [adapter coverage](../../../crates/axon-adapters/src/reddit_tests.rs)
and [acquisition coverage](../../../crates/axon-adapters/src/reddit/acquire_tests.rs).
Changes require credential/error redaction, target validation, bounds,
partial inventories, refresh/removal, cancellation, and shared lifecycle tests.
