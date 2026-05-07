# MCP Response Protocol

## Envelope

```json
{ "ok": true, "action": "<resolved>", "subaction": "<resolved>", "data": { … } }
```

`response_mode` values: `path` (default), `inline`, `both`, `auto_inline`. Override per-call with `response_mode`.

In `path` mode the response includes a `shape` (recursive type/size summary) plus an `artifact` (path, bytes, line_count, sha256). **Read `shape` first** — it's frequently enough to answer the question without opening the file.

When `shape` isn't enough, escalate in this order (least to most expensive):

```json
{ "action": "artifacts", "subaction": "head",   "path": ".cache/axon-mcp/…", "limit": 25 }
{ "action": "artifacts", "subaction": "grep",   "path": ".cache/axon-mcp/…", "pattern": "error", "context_lines": 3 }
{ "action": "artifacts", "subaction": "search", "pattern": "error", "limit": 25 }    // cross-artifact regex
{ "action": "artifacts", "subaction": "read",   "path": ".cache/axon-mcp/…", "pattern": "…" }   // filtered dump
{ "action": "artifacts", "subaction": "read",   "path": ".cache/axon-mcp/…", "full": true }    // last resort
```

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
