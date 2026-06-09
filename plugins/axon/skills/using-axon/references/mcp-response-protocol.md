# MCP Response Protocol

## Envelope

```json
{ "ok": true, "action": "<resolved>", "subaction": "<resolved>", "data": { … } }
```

`response_mode` values: `path` (default), `inline`, `both`, `auto_inline`. Override per-call with `response_mode`.

In `path` mode the response includes a `shape` (recursive type/size summary) plus an `artifact` (path, bytes, line_count, sha256). **Read `shape` first** — it's frequently enough to answer the question without opening the file.

When `shape` isn't enough, escalate in this order (least to most expensive). Pass the
artifact handle's **`relative_path`** (e.g. `search/rust-async.json`) as `path`, not the
absolute display path:

```json
{ "action": "artifacts", "subaction": "wc",     "path": "search/rust-async.json" }                          // line/byte count
{ "action": "artifacts", "subaction": "head",   "path": "search/rust-async.json", "limit": 25 }
{ "action": "artifacts", "subaction": "grep",   "path": "search/rust-async.json", "pattern": "error", "context_lines": 3 }
{ "action": "artifacts", "subaction": "search", "pattern": "error", "limit": 25 }    // cross-artifact regex
{ "action": "artifacts", "subaction": "read",   "path": "search/rust-async.json", "pattern": "…" }   // filtered dump
{ "action": "artifacts", "subaction": "read",   "path": "search/rust-async.json", "full": true }    // last resort
```

`read` requires either `pattern` (filtered) or `full: true` (whole file). `grep`/`search`
default to `limit: 25` matches.

## Artifacts vs. retrieve vs. stats

These touch three different stores — don't mix them up:

- **`artifacts`** reads tool *output files* (search dumps, scrape markdown, crawl results). This is how you open a `path`-mode response.
- **`retrieve`** fetches indexed *chunks* for a URL from Qdrant — the corpus, not the output cache. Pointing it at an artifact path fails with `-32603`.
- **`stats` / `sources` / `domains`** describe the corpus (points, vectors, indexed URLs). The artifact count from `artifacts list` is just the working cache and is unrelated to corpus size.

Artifacts are named deterministically by operation + target slug (`search/<query>.json`,
`scrape/<url>.md`), so re-running the same op **overwrites** the same file instead of
accumulating. Artifact count ≠ number of calls, and ≠ number of indexed documents.

## Artifact cleanup

```json
{ "action": "artifacts", "subaction": "list" }
{ "action": "artifacts", "subaction": "clean", "max_age_hours": 24, "dry_run": true }     // preview
{ "action": "artifacts", "subaction": "clean", "max_age_hours": 24, "dry_run": false }    // commit
{ "action": "artifacts", "subaction": "delete", "path": ".cache/axon-mcp/…" }
```

`max_age_hours` is required for `clean`; `dry_run` defaults to `true`. Never recurses into `screenshots/`.

## Errors

```json
{ "ok": false, "error": { "code": "invalid_params", "message": "…" } }
```

| Code | Meaning | Action |
|---|---|---|
| `invalid_params` | Bad action, missing field, wrong type | Fix the payload. Don't retry the same request. |
| `internal_error` | Service down, timeout, unexpected crash | Run `{ "action": "doctor" }`. Retry may help. |
