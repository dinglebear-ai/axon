---
title: "Sessions Ingest"
created: 2026-02-23
updated: 2026-09-29
---

# Sessions Ingest

Last reviewed: 2026-09-29

Index local Claude, Codex, and Gemini CLI session history through the unified
source pipeline. The [session adapter](../../../crates/axon-adapters/src/sessions.rs)
discovers transcripts, decodes semantic turns, redacts their text, and emits
`SourceDocument` values. Shared preparation, embedding, and publication then
make committed content searchable. There is no separate session ingest queue
or adapter-owned vector writer.

See the [CLI reference](../../reference/actions/sessions.md) for flags and
[session source overview](../sessions.md) for navigation.

## Supported providers and roots

| Provider | Default root under the executing process HOME | Transcript format |
|---|---|---|
| Claude | `~/.claude/projects/` | JSONL |
| Codex | `~/.codex/sessions/` | JSONL |
| Gemini | `~/.gemini/history/` and `~/.gemini/tmp/` | JSON |

These are agent CLI history formats, not a promise that an arbitrary consumer
chat application export has the same schema. Provider selection and root
validation live in
[`sessions/selection.rs`](../../../crates/axon-adapters/src/sessions/selection.rs).
Roots come from the executing process HOME. A client path does not become a
server-readable path merely because it appears in a request.

## Submit and follow a job

```bash
# Existing roots for all three providers
axon sessions --wait true

# One provider, with an optional project filter
axon sessions --codex --project axon --wait true

# One explicit export under an approved provider root
axon "session:codex:${HOME}/.codex/sessions/2026/07/15/session.jsonl" --wait true

# Inspect a returned durable job
axon jobs get <job_id>
axon jobs events <job_id>
```

Replace the example transcript path with an existing local export. Without a
provider flag, `axon sessions` selects every existing provider root. Gemini
history and temporary roots are separate source submissions. Consequently a
command can return more than one source result/job, not one job for the whole
fleet of roots. JSON output groups these under `sessions`.

Without `--wait true`, the CLI enqueues source jobs and attempts to ensure a
worker process exists. A job ID is not completion. With `--wait true`, each
selected source is executed with active workers and waited to terminal state.
Use [unified jobs](../../reference/runtime/jobs.md) for cancellation, retry,
and recovery. The CLI implementation is
[`commands/sessions.rs`](../../../crates/axon-cli/src/commands/sessions.rs).

A transport-neutral selector is `session:<provider>:<path>`. REST callers use
`POST /v1/sources` with that selector in `SourceRequest.source`; MCP callers
use the corresponding source request. Authorization and provider-root checks
still apply on the executing host. Do not use the removed prepared-session
endpoint or assume an upload is automatically decoded as a session export.

## Discovery, refresh, and limits

Discovery checks provider-specific extensions, walks without following links,
computes content fingerprints, and emits manifest entries. Acquisition reads
added and modified files; unchanged content can reuse the existing generation
projection. The semantic document version participates in freshness so decoder
changes can invalidate previously prepared output.

A configured effective item limit caps the selected manifest inventory. A
truncated inventory is marked partial, so omitted transcripts are not evidence
of deletion. The directory walk can still inspect additional entries while
selecting a deterministic bounded set. Item-count bounds are not a guarantee
that discovery time or each transcript's byte size is bounded.

The old `AXON_SESSION_INGEST_MAX_BYTES` variable is **not read by the current
session adapter**. Do not rely on the previously documented 20 MiB limit.
The current acquisition implementation reads each selected transcript into
text; choose a bounded export/file set and account for memory use when
indexing large sessions. This is an implementation limitation, not a
configurable fail-closed byte cap.

Selection rejects symlink roots, unsupported types/extensions, secret path
components, and paths outside the matching provider's approved roots. The
[discovery implementation](../../../crates/axon-adapters/src/sessions/discovery.rs)
and [selection implementation](../../../crates/axon-adapters/src/sessions/selection.rs)
are the source of truth for these checks.

## Searchable text and metadata

[Provider decoders](../../../crates/axon-adapters/src/sessions/decode.rs)
project semantic conversation text and redact it before preparation.
Normalized documents are plain text with the session-turn chunk profile; raw
JSON/JSONL transport is not sent through a second transcript parser.

The [metadata projection](../../../crates/axon-adapters/src/sessions/metadata.rs)
uses stable opaque session/document identities and a strict field allowlist.
Canonical source fields, `session_provider`, opaque `session_id`, generation,
visibility, and redaction state are retained. Optional session turn/tool/skill
fields are allowed when present. Raw local paths and export IDs must not be
reconstructed from public vector metadata.

Do not depend on the old `project`, `project_path`, or `gh_repo` enrichment
contract. Decoder observations such as workspace path, model, tool summaries,
and Git branch are not automatically retained in the normalized vector
payload. A project filter is input selection, not a promise of a corresponding
searchable payload field.

Successfully normalized, redacted semantic text is marked `clean` for normal
retrieval. The `redacted` visibility state is not a success flag to set on all
session vectors. See [redaction](../../reference/runtime/redaction.md) and
[metadata payload](../../reference/sources/metadata-payload.md).

## Capture and recall

The usage plugin registers no automatic SessionStart ingest hook.
`axon memory context` is explicit recall; it does not scan session roots.
Recurring capture belongs to the unified source/watch lifecycle, not the
retired session-specific watcher, setup service, or status/smoke helpers.

## Troubleshooting and extension

**No selected roots:** Check HOME and the provider history directories on the
executing host. An empty selected-root result is not proof that a remote
client's transcripts were inspected. Do not invent an export command or move
files outside the approved roots to bypass selection.

**Selection denied:** Check provider, extension, canonical root, symlink use,
and permissions. Use the appropriate session selector, not generic local-file
indexing, when session-specific decoding/redaction is required.

**Unexpected or empty text:** Compare a small redacted fixture with the
provider decoder and inspect job events. Provider format changes need decoder
coverage; they are not corrected by changing the file suffix.

**Queued but not progressing:** Inspect the returned source job and worker
state. Check data-plane/provider diagnostics before retrying; preserve the
job ID and inspect completed stages.

To add a format, extend the session provider/selection and decoder modules in
`axon-adapters`, update its adapter declarations and transport selectors, and
follow [source onboarding](../../development/adding-source.md). Test semantic
text, redaction, metadata allowlisting, path denial, partial inventories, and
added/modified/removed/unchanged refresh behavior. Existing coverage includes
[adapter tests](../../../crates/axon-adapters/src/sessions_tests.rs),
[decoder tests](../../../crates/axon-adapters/src/sessions/decode_tests.rs), and
[CLI tests](../../../crates/axon-cli/src/commands/sessions_tests.rs).
