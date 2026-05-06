---
name: status
description: Use when the user wants to see the current job queue status, check what jobs are running or pending, monitor async jobs, or get an overview of the job queue. Triggers on "job status", "what's running in axon", "check the queue", "any jobs pending", "axon status", "what's in progress". Different from doctor (service health) — status shows job activity.
allowed-tools: mcp__plugin_axon_axon__axon
---

# axon-status

Global async job queue snapshot — counts by status and family.

## MCP (preferred)

```json
{ "action": "status" }
```

## CLI fallback

```bash
axon status
```

## Family-specific status

```json
{ "action": "crawl",   "subaction": "status", "job_id": "<uuid>" }
{ "action": "embed",   "subaction": "status", "job_id": "<uuid>" }
{ "action": "ingest",  "subaction": "status", "job_id": "<uuid>" }
{ "action": "extract", "subaction": "status", "job_id": "<uuid>" }
```

List recent jobs per family:
```json
{ "action": "crawl", "subaction": "list", "limit": 10 }
```

## Stuck jobs

If jobs show `running` for too long:
```json
{ "action": "crawl", "subaction": "recover" }
```

## Cleanup

```json
{ "action": "crawl", "subaction": "cleanup" }
{ "action": "embed", "subaction": "cleanup" }
```

Live view: MCP App resource `ui://axon/status-dashboard`
