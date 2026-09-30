---
title: "Session Sources"
created: 2026-07-15
updated: 2026-09-29
---

# Session Sources

Last reviewed: 2026-09-29

Claude, Codex, and Gemini CLI transcripts are inputs to the unified source
pipeline. Use `axon sessions` for existing local provider roots or an explicit
`session:<provider>:<path>` selector for one file/directory.

```bash
axon sessions --codex --wait true
axon sessions --claude --codex --project axon --wait true
```

The adapter validates the executing host's provider roots, discovers changed
transcripts, and emits redacted semantic text. Shared services prepare, embed,
and publish it under the same source job. Partial inventories must not delete
unseen items. Item limits do not establish a per-file byte cap.

Public metadata uses opaque identities and an allowlist, not raw local paths
or an assumed Git enrichment record. The distinction matters when using prior
sessions to recover decisions, implementation context, or project evidence.

Use [Sessions Ingest](ingest/sessions.md) for formats, limits, privacy, remote
submission, and troubleshooting; the [sessions action](../reference/actions/sessions.md)
for CLI flags; and [jobs](../reference/runtime/jobs.md) to follow queued work.
