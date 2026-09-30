# GitHub Ingest

Last reviewed: 2026-09-29

GitHub acquisition uses the shared Git source adapter and unified source
pipeline. Repository scopes materialize a shallow checkout; supported
issue, pull-request, and release scopes acquire a targeted API document.
A repository URL does **not** automatically enumerate all issues, pull
requests, releases, or wiki pages.

## Submit a source

```bash
axon https://github.com/owner/repository --wait true
axon https://github.com/owner/repository/issues/123 --wait true
axon https://github.com/owner/repository/pull/456 --wait true
```

Replace placeholders with authorized real targets. These are source writes,
not capability probes. Use the [adapter scope matrix](../../reference/sources/adapter-scopes.md)
for accepted selectors/options. Do not infer that every scope works
identically on GitHub, GitLab, Gitea, or an arbitrary Git host.

## Acquisition behavior

The [Git adapter](../../../crates/axon-adapters/src/git.rs) owns the temporary
checkout and emits normalized documents. Services own preparation, embedding,
publication, graph updates, and cleanup under one source job.

[Clone acquisition](../../../crates/axon-adapters/src/git/acquire.rs) uses
HTTPS, source URL/DNS validation, pinned transport resolution, disabled
redirects, and a shallow `--depth=1 --no-tags` clone. Interactive credential
prompts are disabled. A 300-second clone timeout and owned process group
allow cleanup on failure/cancellation. This is not complete Git history, a
submodule mirror, or a guaranteed bounded clone-output size.

Public repositories may need no credentials. `GITHUB_TOKEN` is used only
for the matched GitHub HTTPS origin through a scoped credential helper;
its value stays out of argv. Private repositories and targeted API documents
need appropriate provider permissions. Keep tokens out of URLs, examples,
logs, and committed configuration.

[Discovery](../../../crates/axon-adapters/src/git/discovery.rs) respects Git
ignore/exclude rules, explicit excluded paths, and containment; it does not
follow links. It selects deterministic item keys and hashes file content.
An item-capped inventory is partial, not evidence that omitted files were
deleted. A selection cap need not stop the filesystem walk at that count.

## Targeted GitHub documents

[Vertical acquisition](../../../crates/axon-adapters/src/git/vertical.rs)
handles GitHub issue, pull-request, and release scopes through matching
`axon-extract` API extractors, without cloning. The resulting document
re-enters shared preparation/publication, not another indexing pipeline.
One targeted document is not an exhaustive issue/PR inventory.

## Preparation and metadata

Repository files become `SourceDocument` values. Document/parser owners
choose appropriate chunk profiles; the adapter does not implement a fixed
chunking policy or pre-embed symbols. See [source pipeline](../../architecture/source-pipeline.md).

[Git metadata](../../../crates/axon-adapters/src/git/metadata.rs) includes
approved provider/host/owner/repository identity, canonical item identity,
source scope, and visibility/generation fields. Regular checkout ingestion
does not automatically fetch stars, topics, issue counts, or the old `gh_*`
enrichment table. Targeted API documents and preparation can contribute
different declared metadata. Use [metadata payload](../../reference/sources/metadata-payload.md)
and [vector payload](../../reference/sources/vector-payload.md).

## Refresh and completion

Stable source/item identity and hashes distinguish added, modified, removed,
and unchanged files. Unchanged content can skip re-embedding. Deletions
require complete discovery or explicit tombstones; partial/interrupted
acquisition must not remove unseen content. Publication advances committed
generation visibility before derived graph/cleanup work.

```bash
axon jobs get <job_id> --json
axon jobs events <job_id> --json
```

Inspect terminal status, provenance, and refresh/removal behavior. A job ID
or successful clone is not proof of published vectors. On failure, inspect
the stage and recovery guidance; do not duplicate unknown-commit writes.

Development coverage includes [adapter tests](../../../crates/axon-adapters/src/git_tests.rs),
[clone tests](../../../crates/axon-adapters/src/git/acquire_tests.rs), and
[vertical tests](../../../crates/axon-adapters/src/git/vertical_tests.rs).
Changes also need shared lifecycle, redaction, failure/cancellation, and
visibility tests.
