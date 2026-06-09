# Async Job Lifecycle

`crawl`, `extract`, `embed`, and `ingest` are async by default. The lifecycle is uniform across families:

```json
{ "action": "crawl",   "subaction": "start",   "urls": ["https://…"] }   // subaction defaults to "start"
{ "action": "crawl",   "subaction": "status",  "job_id": "<uuid>" }
{ "action": "crawl",   "subaction": "cancel",  "job_id": "<uuid>" }
{ "action": "crawl",   "subaction": "list",    "limit": 25 }
{ "action": "crawl",   "subaction": "cleanup" }                          // remove finished
{ "action": "crawl",   "subaction": "clear" }                            // remove ALL in family
{ "action": "crawl",   "subaction": "recover" }                          // restart stalled workers
```

Replace `"crawl"` with `"extract"`, `"embed"`, or `"ingest"` for those families.

CLI mirror: `axon <family> <status|cancel|list|cleanup|clear|recover|errors|worker> [args]`. The CLI also exposes `errors` (per-family error summary) and `worker` (worker-pool diagnostics) which the MCP surface doesn't.

For one-shots in the CLI, prefer `--wait true` over polling. The MCP equivalent is to start the job, then poll `subaction: "status"` (artifacts contain the streaming output).
