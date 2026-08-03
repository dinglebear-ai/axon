# Local Source Ledger Spike Notes

Issue: [#298](https://github.com/jmagar/axon/issues/298)

Plan: local source ledger spike plan from the #298 implementation packet.

## Purpose

This spike proves the local source path can enter the target pipeline shape before
the public CLI, MCP, REST, watch, and job surfaces are rewritten.

The proof target is:

```text
local path
  -> local adapter prototype
  -> SourceLedger draft generation
  -> SourceDocument with LedgerPayload
  -> prepare_source_document
  -> PreparedDoc
```

The spike intentionally stops before embedding, Qdrant upsert, generation
publish, committed-generation search gating, and cleanup execution. Cleanup
placeholders in this PR are returned by the spike output only; they are not
persisted to ledger cleanup debt until the later ledger-owned lifecycle PR.

## Move Mostly As-Is

These pieces already have the right shape and should be moved behind the new
source pipeline rather than rewritten:

- Local file walking and pruning from `axon-vector::ops::file_ingest`.
- Path normalization into slash-separated source item keys.
- Runtime `SourceDocument` constructors for local files and local markdown.
- Code-aware preparation through `prepare_source_document`.
- Tree-sitter/code chunk metadata, including file path, language, line ranges,
  chunk locator, chunking method, and symbol extraction status.
- Markdown chunk preparation and per-chunk source ranges.
- Typed `LedgerPayload` stamping for source id, source kind, generation, item
  key, index version, and uncommitted status.
- TEI batching and throughput knobs after prepared documents exist.

## Rewrite Or Replace Later

These pieces should not be carried forward as-is:

- Public `axon embed` and `code-search-watch` command split. The target surface
  routes local paths through `axon <source>` / source actions with watch options.
- Code-search-specific lifecycle names in user-facing progress, status, logs,
  jobs, and docs. The target name is source/local-source, not code-search.
- Any path that allocates generations without first passing provider preflight.
- Any path that relies on Qdrant scrolls as the source of truth for mutable
  source state.
- Custom stale cleanup paths outside ledger cleanup debt.
- Watch status models that are separate from the unified source job model.
- Progress events that cannot be joined by one `job_id` across logs, ledger,
  traces, status, and vector payloads.

## Follow-Up PR Boundaries

- PR5 is the resolver/router registry: source canonicalization, source IDs,
  authority/alias mapping, adapter capability/scope metadata, and route-time
  validation before acquisition.
- PR6 is the local-source ledger spike: local source adapter prototype, ledger
  draft/generation row, `SourceDocument`, and existing prepare path proof.
- Later lifecycle PRs introduce committed-generation publish and source-owned
  cleanup debt execution.
- Public CLI/MCP/REST cutover is intentionally later, after source-family ports
  and generated-surface removal checks are ready.
