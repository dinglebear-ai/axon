# URL Normalization

Last Modified: 2026-07-19

Source resolution turns a raw user string (`https://example.com/docs`,
`github.com/owner/repo`, `crates:serde`, `r/rust`, `/home/me/proj`) into a
canonical URI, a source kind, an adapter candidate, and an authority level.
Stable identity across refreshes depends on it.

> Contract source:
> [`docs/pipeline-unification/sources/url-normalization.md`](../../pipeline-unification/sources/url-normalization.md).
> Implementation: [`crates/axon-route/src/`](../../../crates/axon-route/src/)
> (`resolver.rs`, `router.rs`, `canonical.rs`, `source_id.rs`, `authority.rs`,
> `alias.rs`, `scope.rs`).

## Resolution pipeline

```text
raw input → lexical normalization → source kind detection
  → adapter candidate resolution → authority lookup
  → canonical_uri → source_id / source_fingerprint
```

`source_fingerprint = hash(source_kind, canonical_uri, scope_identity_version)`
and `source_id = "src_" + stable_id(source_fingerprint)`. Stable across
refreshes; the mutable generation does not change `source_id`.

## Canonical URI scheme families

| Source | Canonical URI |
|---|---|
| web page/site/docs | `https://host/path` |
| local project | `local://<local_project_key>` |
| local file item | `local://<local_project_key>/<relative_path>` |
| GitHub repo | `github://<owner>/<repo>` (commit: `?rev=<sha>`) |
| GitLab | `gitlab://<host>/<namespace>/<repo>` |
| generic git | `git+https://host/path/repo.git` |
| package | `pkg://<registry>/<namespace>/<name>` (version: `@<version>`) |
| Docker | `docker://<registry>/<namespace>/<image>:<tag>` |
| feed | `feed://<host>/<feed_key>` |
| Reddit | `reddit://r/<subreddit>` |
| YouTube | `youtube://video/<video_id>` |
| session | `session://<provider>/<session_id>` |
| CLI tool / MCP | `cli://<tool_id>`, `mcp://<server_id>/tools/<tool_name>` |
| artifact / upload | `artifact://<artifact_id>`, `upload://<upload_id>` |

> **Shipped divergence:** code-search currently uses a private
> `local-code://<project_key>/g/<generation>/<path>` item URI rather than the
> target public `local://` scheme. The local family also ships
> `local_checkout`/`local_path_key`/`local_git_remote`/`local_git_commit`
> payload fields rather than the target `local_project_key`/`local_root_label`/
> `local_relative_path`.

## Input shorthand

| Input | Resolves to |
|---|---|
| `crates:serde` | `pkg://crates/serde` |
| `npm:@modelcontextprotocol/sdk` | `pkg://npm/@modelcontextprotocol/sdk` |
| `pypi:FastAPI` | `pkg://pypi/fastapi` |
| `docker:library/postgres:16` | `docker://docker.io/library/postgres:16` |
| `r/rust` | `reddit://r/rust` |
| `jmagar/axon` | provider-resolved git URI |

## Authority levels

`user_pinned`, `official`, `inferred`, `community`, `mirror`, `unknown`
(graph authority adds `verified` and `conflicting`). Authority registry
records carry `authority_id`, `canonical_uri`, `source_kind`, `aliases`,
`entrypoints`, `evidence`, `confidence`, `updated_at`. The resolver consults
the SourceGraph authority registry without network access.

## Lexical rules

Lowercase scheme/host, remove default ports, IDNA/punycode, percent-encoding
normalize, drop fragments unless the adapter declares fragment identity,
collapse duplicate slashes, sort stable query params, bounded canonical
redirect probe.

**Tracking params dropped:** `utm_*`, `fbclid`, `gclid`, `mc_cid`, `mc_eid`,
`igshid`, `ref`, `ref_src`, `spm`, `vero_id`.

**Sensitive params redacted:** `token`, `access_token`, `api_key`, `key`,
`signature`, `sig`, `auth`, `code`, `session`, `password`, `secret`.

## Alias resolution

`AliasRecord` resolves aliases to authorities **without network access**.
Rules: explicit schemes win over shorthand; existing local paths win over host
shorthand; ambiguous input fails before network acquisition unless the caller
supplied adapter/scope hints; bounded network probes only when the resolver
declares them needed.

## Security

SSRF policy runs before any network probe; local-path policy before filesystem
access; credentials are never placed in canonical URIs; remote callers cannot
resolve arbitrary local paths; DNS-rebinding / private-IP protections apply to
web probes.

## Where normalized identity is used

Source ids, manifest diffing, duplicate detection, vector payloads, graph
edges, and cleanup selectors all derive from the canonical URI — which is why
normalization correctness is load-bearing for refresh correctness.

If the resolution rules change, update this file and `crates/axon-route/src/`
in the same PR.
