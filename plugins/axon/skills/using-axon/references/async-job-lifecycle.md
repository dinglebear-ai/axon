# Durable job lifecycle

Load when an Axon call starts detached work, a job stalls, or a watch needs inspection.

One SQLite job model tracks source indexing, extraction, watch runs, attempts, stages, events, and artifacts. A source job keeps one job ID through acquisition, preparation, embedding, and publication.

| Surface | Default | Explicit alternative |
| --- | --- | --- |
| MCP `source` | Synchronous result | `detached: true` returns a job descriptor |
| CLI `axon <source>` | Enqueue and return job ID | `--wait true` blocks for completion |

For a detached operation, inspect `{ "action":"jobs", "subaction":"get", "job_id":"..." }` and `events`. Available lifecycle actions and their required parameters are in the current `help` response or `axon://schema/mcp-tool`. `jobs retry`, `cancel`, `recover`, `cleanup`, and `clear` change state; inspect the current job and obtain task authority before using them. A process with workers must be running for detached jobs to advance.

`extract` defaults to its `start` subaction. `watch` supports list, get, status, history, and mutation subactions such as create, update, pause, resume, exec, and delete. A watch stores a source request and schedule; each tick enqueues one source job. Inspect watch history and job events before repeating a failed execution.
