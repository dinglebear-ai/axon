---
name: monitor
description: Use Axon source watches and job monitoring to track recurring source changes and observe durable job activity.
---

# Axon Monitor

Axon has two related surfaces:

- `axon watch` defines **recurring source re-indexing**. A watch persists a
  canonical source request plus a schedule; each due tick leases the watch and
  enqueues one `source` job.
- `axon monitor jobs` streams unified durable job lifecycle events.

Use this skill when the user wants repeated checks, change history, or job monitoring.

## Source Watch

A watch takes a **source** and an interval — nothing else is required. There is
no `--task-type` and no `--task-payload`; those belonged to the retired
per-family watch model.

```bash
axon watch create "https://example.com/pricing" --every-seconds 3600
```

Optional: `--collection <name>` to target a non-default Qdrant collection.

Manage it:

```bash
axon watch list
axon watch get <watch-id>
axon watch status <watch-id>
axon watch exec <watch-id>          # run immediately
axon watch pause <watch-id>
axon watch resume <watch-id>
axon watch update <watch-id>
axon watch history <watch-id>       # run history, with the job id per tick
axon watch delete <watch-id>
```

`watch history` records the `source` job id for each tick — follow it into
`axon jobs get <job-id>` for stage-level detail.

## Job Event Monitor

```bash
axon monitor jobs --jsonl
axon monitor jobs --watch --jsonl --interval-secs 5
```

## Guidance

- Use `watch` for recurring re-indexing of any source, not just web URLs — the
  watch source string accepts the same selectors `axon <source>` does.
- Use `monitor jobs` for queue visibility while detached source and extract jobs
  run; use `axon jobs get/events` for one job's detail and `axon status` for the
  global snapshot.
- Detached work only advances while a worker is running. If ticks fire but
  nothing progresses, check for a live worker (`axon jobs worker`, `axon serve`,
  or HTTP-mode `axon mcp`) before debugging the watch.
- Do not document unsupported hosted webhook/email monitor flows unless Axon implements them.
