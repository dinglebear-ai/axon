---
title: "Git and GitHub Sources"
created: 2026-02-23
updated: 2026-08-02
---

# Git and GitHub Sources

Git repositories enter through the unified source pipeline and the `git`
source family. GitHub, GitLab, Gitea/Forgejo, and generic HTTPS clone URLs are
classified automatically.

Current command reference: [axon source](../../reference/actions/source.md).
Supported adapters and scopes: [Adapter Scopes](../../reference/sources/adapter-scopes.md).

## Index a repository

```bash
axon source https://github.com/owner/repo --scope repo --wait true
axon source https://gitlab.com/group/project --scope repo --wait true
axon source https://git.example.com/owner/repo.git --wait true
```

`repo` is the default git scope. `branch` is also supported by the generic git
family. Hosted adapters additionally declare issue, pull/merge-request, and
release scopes where a corresponding vertical extractor exists.

Without `--wait true`, the command returns a detached durable source job. Use
`axon jobs get <job-id>` and `axon jobs events <job-id>` for lifecycle details.

## Repository acquisition

Repository acquisition is owned by
`crates/axon-adapters/src/git/acquire.rs`:

1. The clone URL is normalized and SSRF-validated.
2. The hostname is resolved and every resolved address is checked before the
   external `git` process starts.
3. Axon performs a shallow HTTPS clone with `--depth=1`, `--no-tags`, redirects
   disabled, and `GIT_TERMINAL_PROMPT=0`.
4. The checkout lives in a temporary directory and is removed after
   acquisition.
5. The checked-out tree is converted into `SourceDocument` values and continues
   through the shared document, embedding, vector, ledger, and graph stages.

Private repositories must already be accessible to non-interactive `git` on the
Axon host, such as through a configured credential helper. Axon does not open an
interactive credential prompt.

## Files and exclusions

The local-tree selector used after clone is implemented under
`crates/axon-adapters/src/local_select.rs`. It excludes common generated,
cache, VCS, and dependency directories by default. Add request-specific
substrings with repeatable `--exclude-path` options:

```bash
axon source https://github.com/owner/repo \
  --exclude-path vendor/ \
  --exclude-path docs/generated/ \
  --wait true
```

Adapters emit whole `SourceDocument` values. They do not pre-chunk files.
`axon-document` selects code, manifest, markdown, schema, or text preparation
profiles from content kind and path.

## Code preparation

Code-aware preparation is owned by `crates/axon-document/` and parser facts
from `crates/axon-parse/`:

- supported source files use parser-aware symbol boundaries when available;
- manifests and structured files use structured profiles;
- unsupported or failed parsing falls back to deterministic safe chunking;
- chunks record extraction/fallback metadata rather than pretending parser
  coverage succeeded.

See [Chunking](../../reference/sources/chunking.md) and
[Parsing](../../reference/sources/parsing.md).

## Metadata

`crates/axon-adapters/src/git/metadata.rs` stamps approved repository identity
onto each source document, including:

- `source_family=code`
- `source_kind=git`
- adapter and scope
- `git_provider`
- `git_host`
- `git_owner` when present
- `git_repo`
- `git_web_url`
- canonical item identity

Document preparation may add normalized `code_*`, chunk, symbol, language, and
source-range fields. The generated payload contract is authoritative:
[Vector Payload](../../reference/sources/vector-payload.md).

## GitHub vertical targets

GitHub issue, pull-request, and release URLs are routed through the git family
without cloning the full repository. The vertical path lives in
`crates/axon-adapters/src/git/vertical.rs` and dispatches to the corresponding
extractor in `axon-extract`.

Examples:

```bash
axon source https://github.com/owner/repo/issues/123 --wait true
axon source https://github.com/owner/repo/pull/456 --wait true
axon source https://github.com/owner/repo/releases/tag/v1.2.3 --wait true
```

A vertical extraction failure is surfaced as an acquisition error; it does not
silently fall back to cloning an unrelated repository scope.

## Refresh behavior

Git sources support refresh through the shared source lifecycle. Manifest diff,
generation publication, vector writes, graph updates, and cleanup debt use the
same ledger semantics as every other mutable source.

## Related documentation

- [Source Pipeline](../../architecture/source-pipeline.md)
- [Local Sources](../local-sources.md)
- [Adding a Source Adapter](../../development/adding-source-adapter.md)
- [Runtime Jobs](../../reference/runtime/jobs.md)
