# Unified Job Lifecycle

All detached Axon operations use the same durable jobs surface. Source families
do not have separate status, cancel, cleanup, or worker commands.

Start a detached source request:

```json
{ "action": "source", "source": "https://docs.example.com", "scope": "site", "detached": true }
```

Manage the returned job ID:

```json
{ "action": "jobs", "subaction": "get", "job_id": "<uuid>" }
{ "action": "jobs", "subaction": "events", "job_id": "<uuid>" }
{ "action": "jobs", "subaction": "cancel", "job_id": "<uuid>" }
{ "action": "jobs", "subaction": "retry", "job_id": "<uuid>" }
{ "action": "jobs", "subaction": "list", "limit": 25 }
{ "action": "jobs", "subaction": "cleanup" }
{ "action": "jobs", "subaction": "recover" }
```

CLI mirror:

```bash
axon jobs get <job-id>
axon jobs events <job-id>
axon jobs cancel <job-id>
axon jobs retry <job-id>
axon jobs list
axon jobs cleanup
axon jobs recover
```

For CLI one-shots, prefer `--wait true` when the deliverable requires terminal
completion. `jobs clear` removes terminal rows only; active work must be
canceled or recovered first.
