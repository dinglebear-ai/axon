---
title: "Sessions Ingest"
created: 2026-02-23
updated: 2026-08-03
---

# Sessions Ingest

`axon sessions` indexes local Claude, Codex, and Gemini conversation exports
through the same `SourceRequest` pipeline used by every other source. The
session adapter discovers supported export files, decodes conversational turns,
redacts secret-shaped tokens, prepares transcript documents, and publishes them
through the shared embedding, ledger, vector, and graph stages.

For the exact current flags, see
[`docs/reference/actions/sessions.md`](../../reference/actions/sessions.md).

## Supported local roots

| Provider | Default roots | Format |
|---|---|---|
| Claude | `~/.claude/projects/` | JSONL |
| Codex | `~/.codex/sessions/` | JSONL |
| Gemini | `~/.gemini/history/`, `~/.gemini/tmp/` | JSON |

With no provider flag, `axon sessions` scans every existing root. Use
`--claude`, `--codex`, or `--gemini` to select providers; flags may be combined.
`--project <name>` applies the adapter's project filter.

```bash
# Enqueue unified source jobs for every available provider root
axon sessions

# Index only Codex exports and wait for completion
axon sessions --codex --wait true

# Filter Claude and Codex exports to an Axon project match
axon sessions --claude --codex --project axon --wait true
```

Detached execution is the default. It returns unified source job information
and ensures a worker process is available. Use `axon jobs list/get/cancel` for
lifecycle control.

## Explicit session selectors

Callers that already know a file or directory can use the transport-neutral
selector shape `session:<provider>:<path>`:

```bash
axon 'session:codex:/home/me/.codex/sessions/2026/07/15/session.jsonl' --wait true
axon 'session:gemini:/home/me/.gemini/history/' --wait true
```

The explicit prefix selects the session adapter instead of generic local-file
acquisition. A file selector indexes that export; a directory selector
discovers supported exports beneath that directory.

## Indexed content and metadata

Claude and Codex JSONL and Gemini JSON are decoded into redacted transcript
text. The published session document includes canonical source fields plus the
session metadata allowlisted by the adapter. Session content is classified as
internal and secret-shaped tokens are replaced before embedding.

Do not rely on legacy `project`, `project_path`, `gh_repo`, or direct-Qdrant
payload fields. Inspect the generated payload/schema references for the current
published contract.

## Server-side submission and uploads

REST or MCP callers may submit a `SourceRequest` with an explicit session
selector only when the Axon server can read that path. Remote clients stage an
export through the live uploads surface and then submit the resulting
server-owned source reference. See
[`uploads`](../../reference/actions/uploads.md) and
[`source`](../../reference/actions/source.md). There is no separate prepared-
session endpoint or session-specific queue.

## Implementation ownership

- CLI selection and enqueue: `crates/axon-cli/src/commands/sessions.rs`
- selector parsing: `crates/axon-services/src/sessions_target.rs`
- discovery, decoding, redaction, and metadata: `crates/axon-adapters/src/sessions.rs`
- durable execution: the unified source runner in `axon-services`

To add a provider, extend the adapter/provider model and its sidecar tests,
then update the CLI, source capability registry, and generated transport docs.
Do not create a provider-specific job table or embedding path.

## Troubleshooting

**No selected session roots exist**

None of the selected default directories exists on the Axon host. Select a
provider with an installed history root or use an explicit session selector.

**A transcript is rejected**

Confirm the provider and extension match: Claude/Codex use `.jsonl`; Gemini
uses `.json`. The adapter also rejects paths that escape the validated session
root.

**A detached job does not finish**

Inspect `axon jobs get <job_id>` and its events. Session jobs use the same
in-process worker runtime and provider dependencies as other source jobs.
