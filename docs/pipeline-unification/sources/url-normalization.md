# URL and Source Normalization Contract
Last Modified: 2026-06-30

## Contract

This is the target URL/source normalization contract. Current normalization is
lighter and split across HTTP helpers, ingest classifiers, and command services.

Source strings, URLs, local paths, package ids, repo shorthands, feed URLs, MCP
URIs, CLI tool specs, and uploaded artifact refs normalize into canonical
source identities before acquisition.

Normalization is owned by `SourceResolver` and `SourceRouter`; individual
adapters may contribute patterns and authority hints, but they must not invent
transport-specific identity rules.

```text
raw input
  -> lexical normalization
  -> source kind detection
  -> adapter candidate resolution
  -> authority lookup
  -> canonical_uri
  -> source_id/source_fingerprint
```

## Design Rules

- Preserve the original requested value as `requested_uri`.
- Produce one `canonical_uri` for source identity.
- Produce item-level canonical URIs for documents/items.
- Never use raw absolute local paths as public identity.
- Normalize before SourceLedger lookup.
- Normalize before SourceGraph linking.
- Normalize before VectorStore payload construction.
- Do not fetch broad content during basic normalization.
- Bounded network probes are allowed only when requested by resolver hints or
  adapter policy.

## Current Implementation Snapshot

Implemented today:

- `normalize_url()` trims input, preserves existing schemes, and prepends
  `https://` to host-like strings.
- Current normalization does not universally remove tracking parameters, compute
  `canonical_uri`, compute `source_id`, or apply authority mappings.
- Ingest has separate source classifiers and validators for supported ingest
  target shapes.
- Code-search uses a private local `project_key` and local-code URL shape
  `local-code://<project_key>/g/<generation>/<path>`; this is not the target
  public `local://<local_project_key>` identity model yet.

Planned by this contract:

- `SourceResolver` owns canonical URI, source kind detection, authority lookup,
  source fingerprinting, and item-level canonical URIs for every source family.
- Resolver output is reused by SourceLedger, SourceGraph, VectorStore payloads,
  REST/MCP/CLI status, and retrieval citations.

## Input Classes

| Input Class | Examples | Canonical Form |
|---|---|---|
| web URL | `https://ui.shadcn.com/docs` | normalized HTTPS URL |
| scheme-less host | `shadcn.com` | `https://shadcn.com/` plus docs entry mapping when known |
| local path | `/home/jmagar/workspace/axon` | `local://<local_project_key>` |
| git host URL | `github.com/jmagar/axon` | `github://jmagar/axon` |
| repo shorthand | `jmagar/axon` | provider-resolved git URI |
| registry package | `crates:serde`, `npm:@scope/pkg` | `pkg://<registry>/<name>` |
| feed | `rss:https://example.com/feed.xml` | `feed://...` or canonical feed URL |
| reddit | `r/rust`, `reddit.com/r/rust` | `reddit://r/rust` |
| youtube | video/playlist/channel URLs | `youtube://video/<id>` etc. |
| session export | local/export path | `session://<provider>/<session_id>` |
| upload | upload/artifact id | `upload://<upload_id>` or `artifact://<artifact_id>` |
| CLI tool | `cli:rg --help`, binary path | `cli://<tool_id>` |
| MCP tool | MCP server/tool spec | `mcp://<server_id>/tools/<tool>` |

## Lexical URL Rules

For URL-like inputs:

- lowercase scheme and host
- remove default ports
- normalize punycode/idna hostnames
- normalize percent encoding
- remove fragments unless the adapter declares fragment identity
- remove tracking query params
- sort stable query params when order is not semantically meaningful
- preserve semantically meaningful query params
- collapse duplicate slashes in path except where scheme semantics forbid it
- remove trailing slash only when equivalent by adapter rules
- follow configured canonical host redirects only through bounded probe

Tracking params removed by default:

```text
utm_*
fbclid
gclid
mc_cid
mc_eid
igshid
ref
ref_src
spm
vero_id
```

Sensitive query params are redacted, not stored public:

```text
token
access_token
api_key
key
signature
sig
auth
code
session
password
secret
```

## Canonical URI Shapes

| Source Kind | Canonical URI Shape |
|---|---|
| web page/site/docs | `https://host/path` |
| local project | `local://<local_project_key>` |
| local file item | `local://<local_project_key>/<relative_path>` |
| GitHub repo | `github://<owner>/<repo>` |
| GitHub commit | `github://<owner>/<repo>?rev=<sha>` |
| GitLab project | `gitlab://<host>/<namespace>/<repo>` |
| generic git | `git+https://host/path/repo.git` |
| package | `pkg://<registry>/<namespace>/<name>` |
| package version | `pkg://<registry>/<namespace>/<name>@<version>` |
| docker image | `docker://<registry>/<namespace>/<image>:<tag>` |
| feed | `feed://<host>/<feed_key>` |
| reddit subreddit | `reddit://r/<subreddit>` |
| youtube video | `youtube://video/<video_id>` |
| session | `session://<provider>/<session_id>` |
| CLI tool | `cli://<tool_id>` |
| MCP server/tool | `mcp://<server_id>/tools/<tool_name>` |
| artifact/upload | `artifact://<artifact_id>`, `upload://<upload_id>` |

## Source ID and Fingerprint

`source_id` is derived from canonical identity, not display label.

Recommended fingerprint:

```text
source_fingerprint = hash(source_kind, canonical_uri, scope_identity_version)
source_id = "src_" + stable_id(source_fingerprint)
```

Rules:

- Source ids remain stable across refreshes.
- Mutable source generation changes do not change `source_id`.
- Immutable source versions may share `source_id` with generation/version fields
  or use version-specific source ids only when adapter policy requires it.
- Local absolute path changes may create a new `local_project_key` unless
  project identity can be proven by VCS root.

## Scope and Item Identity

Scope affects acquisition strategy, not base source identity unless the adapter
declares the scope as identity-bearing.

Examples:

| Source | Scope | Same Source ID? | Notes |
|---|---|---:|---|
| `shadcn.com` | `site` vs `docs` | maybe | docs entry mapping may canonicalize to docs source |
| GitHub repo | `repo` vs `issues` | yes | same repo source, different item classes |
| package | `package` vs `version` | depends | version may be item/version under package source |
| local repo | `repo` vs `directory` | yes if same root | scope changes manifest |

Item keys are source-relative:

- repo/local: relative path
- web: normalized URL path or URL hash
- package: version/file/doc key
- feed/social: entry/thread/comment id
- session: turn/tool call id
- MCP: server/tool/resource/call id

## Authority Registry

Authority mapping links shorthand or known domains to official sources.

Authority levels:

| Level | Meaning |
|---|---|
| `user_pinned` | explicit user-selected source of truth |
| `official` | verified upstream/project-owned source |
| `inferred` | high-confidence resolver inference |
| `community` | useful but unofficial |
| `mirror` | mirror/copy of another source |
| `unknown` | no confidence claim |

Authority registry records:

| Field | Meaning |
|---|---|
| `authority_id` | stable id |
| `canonical_uri` | official source URI |
| `source_kind` | source kind |
| `aliases` | accepted shorthands/domains |
| `entrypoints` | docs/site/repo/package entrypoints |
| `evidence` | graph/source evidence |
| `confidence` | confidence score |
| `updated_at` | refresh time |

Examples:

- `shadcn.com` may map to official docs entrypoint `https://ui.shadcn.com/docs`
  if registry evidence says that is the docs source.
- `shadcn-ui/ui` may map to the GitHub repo.
- package docs URLs and repo URLs should graph-link to the same authority
  cluster when evidence supports it.

## Web Docs Entry Mapping

Scheme-less domains are ambiguous. Resolver should attempt ordered entrypoint
mapping:

1. user-pinned authority entrypoint
2. known SourceGraph authority mapping
3. package/repo metadata docs URL
4. sitemap and common docs paths with bounded probe
5. domain root

Common docs paths:

```text
/docs
/documentation
/guide
/learn
/reference
/api
```

Mapping result includes:

- selected entrypoint
- alternatives
- confidence
- evidence
- whether a network probe was used

## Local Path Normalization

Rules:

- expand `~` and relative paths internally
- resolve symlinks for safety checks
- detect VCS root when present
- compute `local_project_key`
- expose relative path and safe root label publicly
- hash absolute paths for private correlation
- do not put raw absolute path in public vector payloads

Local canonical examples:

```text
local://lp_01J.../
local://lp_01J.../crates/axon-cli/src/lib.rs
```

## Package and Registry Normalization

Package specs normalize into registry-aware identities.

Rules:

- preserve namespace/scope
- normalize case according to registry semantics
- normalize version/tag separately from package identity
- link package docs/repo/homepage through SourceGraph, not by changing package
  canonical URI
- registry adapters own registry-specific name validation

Examples:

| Input | Canonical |
|---|---|
| `crates:serde` | `pkg://crates/serde` |
| `npm:@modelcontextprotocol/sdk` | `pkg://npm/@modelcontextprotocol/sdk` |
| `pypi:FastAPI` | `pkg://pypi/fastapi` |
| `docker:library/postgres:16` | `docker://docker.io/library/postgres:16` |

## Tool and MCP Normalization

CLI tools/scripts:

- normalize binary path or command name to `tool_id`
- store command argv fingerprint separately
- store working directory as safe key/hash
- include allowlist policy in metadata

MCP servers/tools:

- normalize server identity independent of client implementation
- normalize tool/resource/prompt names under server id
- store `mcporter` or another client only as `mcp_client_provider`
- graph evidence describes MCP server/tool/call/result, not helper alone

## Resolver Outputs

`ResolvedSource` includes:

| Field | Meaning |
|---|---|
| `source` | original user/source string |
| `requested_uri` | raw requested value |
| `source_uri` | normalized input URI |
| `canonical_uri` | canonical source URI |
| `source_kind` | source kind |
| `adapter` | selected adapter |
| `default_scope` | default scope |
| `available_scopes` | adapter scopes |
| `authority` | authority level |
| `confidence` | resolution confidence |
| `reason` | human/debug explanation |
| `graph` | authority/relationship refs |
| `warnings` | ambiguity/degradation warnings |

## Security

- SSRF policy applies before network probe.
- Local path policy applies before filesystem access.
- Credentials are never part of canonical URI.
- Sensitive query params are redacted before storage/logging.
- Remote callers cannot resolve arbitrary local paths unless policy allows it.
- Authority probes are bounded by timeout, redirect limit, and host policy.
- DNS rebinding/private IP protections apply to web normalization probes.

## Validation Checklist

Implementation is incomplete until:

- every source request stores requested and canonical values
- canonical URI forms are deterministic
- SourceLedger lookup uses normalized identity
- SourceGraph linking uses canonical identity
- local public payloads never expose absolute paths by default
- docs entry mapping records evidence/confidence
- package/repo/docs relationships graph-link instead of overwriting identity
- sensitive query params are redacted
- unsupported/ambiguous resolution returns structured warnings/errors
- URL normalization tests cover redirects, fragments, query params, docs paths,
  package names, local paths, repo shorthands, CLI tools, and MCP tools
