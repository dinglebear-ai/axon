# MCP Response Protocol

## Success envelope

```json
{ "ok": true, "action": "<resolved>", "subaction": "<resolved>", "data": { … } }
```

`warnings` (a string array) is also present when non-empty. Everything action-specific lives under `data`.

## Response modes

`response_mode` ∈ `path` | `inline` | `both` | `auto_inline`. You rarely set it: the server runs **in-process** and size-routes automatically. With no `response_mode`, small payloads (≤ ~8 KB) return **inline**; larger ones are persisted to a local file and returned as a `path`. Exception: `scrape` and `retrieve` are document-reading actions and default to **inline-first paged** responses regardless of size.

In `path` mode, `data` looks like:

```json
{
  "response_mode": "path",
  "shape": { … },                 // recursive type/size summary
  "artifact_handle": { "kind": "…", "relative_path": "search/rust-async.json",
                       "display_path": "…", "bytes": 1234, "line_count": 56, "url": "…" },
  "artifact": { "path": "search/rust-async.json", "display_path": "…", "sha256": "…", … }
}
```

**Read `shape` first** — it answers most questions (counts, status, which URLs were touched) without opening anything.

## Getting the content — use RAG, not the file

Everything axon scrapes/crawls/ingests is **already embedded**. When you need the actual content, go back through the index, not the artifact bytes:

- **`ask`** — synthesized, cited answer over what's indexed.
- **`query`** — top-K semantic chunks.
- **`retrieve`** — all indexed chunks for a specific URL.

This is the intended path. Reading the artifact `path` off disk is a **last resort** — only when you need exact raw bytes the RAG path doesn't surface (e.g. a precise HTML snapshot). To force the payload in-band instead of a file, set `response_mode: "inline"` (or `"auto_inline"`).

> **There is no `artifacts` MCP action.** It was removed (in 5.0.0) on purpose — RAG is how you get content. `{ "action": "artifacts", … }` deserializes as an unknown action and is rejected. There are no `wc`/`head`/`grep`/`search`/`read`/`clean`/`delete` artifact subactions on the MCP surface.

## Where artifacts live

Path-mode files are written under **`~/.axon/artifacts/<context>`** (`<context>` is derived from the client's repo/dir name). Override the root with `AXON_MCP_ARTIFACT_DIR`; resolution is `AXON_MCP_ARTIFACT_DIR` → `AXON_DATA_DIR/artifacts` → `$HOME/.axon/artifacts`, with `/tmp/axon-mcp/<context>` as a fallback when the primary isn't writable. Files are named deterministically by operation + target slug (`search/<query>.json`, `scrape/<url>.md`), so re-running the same op **overwrites** the same file rather than accumulating.

## artifacts vs. retrieve vs. stats

Don't conflate the corpus with the on-disk output:

- **The artifact `path`** is one tool call's *output file*. Read it directly only as a last resort (see above).
- **`retrieve`** fetches indexed *chunks* for a URL from Qdrant — the corpus. Don't point it at an artifact path; it validates a URL.
- **`stats` / `sources` / `domains`** describe the corpus (points, vectors, indexed URLs). The on-disk artifact files are a transient working cache, unrelated to corpus size.

## Errors (JSON-RPC)

Failures come back as standard JSON-RPC 2.0 errors (via rmcp `ErrorData`), not a `{ "ok": false, … }` body:

| Code | Name | Meaning | Action |
|---|---|---|---|
| `-32602` | `invalid_params` | Bad/unknown action, missing field, wrong type, failed URL validation | Fix the payload. Don't retry the same request. |
| `-32603` | `internal_error` | Service down, timeout, unexpected crash | Run `{ "action": "doctor" }`. Retry may help. |

HTTP-only actions (`debug`, `dedupe`, `migrate`, `watch`, `setup`) sent over MCP are rejected with a message pointing at the HTTP API.
