# Adapter Scopes Contract
Last Modified: 2026-06-30

## Contract

This is the target adapter/scope registry contract. Current source handling is
still split across command-specific services and source classifiers.

Adapters declare source kinds, supported scopes, default scope rules,
capabilities, option schemas, credential requirements, watch/refresh behavior,
chunking/parser hints, graph fact support, degraded modes, and safety policy
before acquisition starts.

`--scope` means: execute only that acquisition strategy for the resolved source.

`map` is both:

- a scope on source-capable adapters
- a top-level CLI command, REST route, and MCP action that projects to
  `SourceRequest { scope: "map", embed: false }`

## Design Rules

- Adapters are acquisition boundaries, not legacy command names.
- All adapters emit `SourceDocument`.
- Adapter scopes are data in capability documents.
- Unsupported scope fails before acquisition.
- Scope defaults are deterministic.
- Every scope declares whether it embeds by default.
- Every scope declares watch/refresh support.
- Every scope declares graph/chunk/parser behavior.
- Adapter options are validated against schema.
- Adapter degraded behavior is explicit.

## Current Implementation Snapshot

Implemented today:

- Web crawl/scrape/map, ingest providers, local embed, local code-search,
  sessions, URL watch, search, research, memory, and system operations are
  implemented through existing command/service families.
- Scope-like behavior exists through command-specific flags, source classifiers,
  crawl options, ingest source detection, and code-index configuration.
- `--scope` is not a universal CLI flag today, and unsupported source scopes are
  not validated through one adapter capability document.
- Authz currently uses broad read/write scopes rather than per-adapter scope
  capabilities.

Planned by this contract:

- Every adapter publishes a capability document with supported scopes, default
  scope, option schema, credential requirements, watch/refresh behavior,
  chunking hints, graph fact support, and degraded modes.
- `--scope` means execute only the selected acquisition strategy for the
  resolved source.
- Registry/package/social/session/tool/MCP/CLI adapters become first-class
  source adapters under the same registry.

## Capability Shape

Each adapter exposes:

```json
{
  "name": "github",
  "version": "2026-06-30",
  "source_kinds": ["github", "git"],
  "default_scope": "repo",
  "scopes": [],
  "patterns": [],
  "credential_requirements": [],
  "limits": {},
  "metadata_fields": [],
  "parser_families": [],
  "graph_fact_kinds": [],
  "degraded_modes": [],
  "watch_supported": true,
  "refresh_supported": true
}
```

Scope capability:

| Field | Required | Meaning |
|---|---:|---|
| `name` | yes | Scope name. |
| `description` | yes | Human/agent-readable meaning. |
| `embeds_by_default` | yes | Whether source run writes vectors. |
| `watch_supported` | yes | Whether watch can keep scope fresh. |
| `refresh_supported` | yes | Whether refresh applies. |
| `requires_credentials` | yes | Whether credentials may be needed. |
| `may_access_local_paths` | yes | Local filesystem access. |
| `may_perform_network_fetches` | yes | Network fetches. |
| `may_call_render_provider` | yes | Browser/render capability. |
| `may_execute_tools` | yes | CLI/MCP execution. |
| `accepts_uploads` | yes | Prepared uploads accepted. |
| `output_item_kind` | yes | file/page/repo_file/package/transcript/etc. |
| `option_schema` | yes | JSON schema for scope-specific options. |
| `chunking_hints` | yes | Default chunk profile/parser hints. |
| `required_graph_fact_kinds` | yes | Graph facts this scope must emit when the source contains the corresponding structures. |
| `optional_graph_fact_kinds` | yes | Opportunistic graph facts this scope may emit without failing when absent. |
| `degraded_modes` | yes | Allowed degraded behavior. |

## Target Adapter Registry

| Adapter | Source Kinds | Scopes | Default |
|---|---|---|---|
| `web` | `web` | `page`, `site`, `docs`, `map` | `site` |
| `local` | `local_path` | `file`, `directory`, `workspace`, `repo`, `map` | detected |
| `upload` | `derived`, `session`, `local_path` | `file`, `archive`, `repomix`, `session`, `warc`, `bundle`, `map` | detected |
| `github` | `github`, `git` | `repo`, `branch`, `commit`, `issues`, `prs`, `wiki`, `org`, `map` | `repo` |
| `gitlab` | `gitlab`, `git` | `repo`, `branch`, `commit`, `issues`, `mrs`, `wiki`, `group`, `map` | `repo` |
| `gitea` | `gitea`, `git` | `repo`, `branch`, `commit`, `issues`, `prs`, `wiki`, `org`, `map` | `repo` |
| `generic_git` | `git` | `repo`, `branch`, `commit`, `map` | `repo` |
| `crates` | `registry_package` | `package`, `version`, `owner`, `docs`, `dependencies`, `map` | `package` |
| `npm` | `registry_package` | `package`, `version`, `scope`, `docs`, `dependencies`, `map` | `package` |
| `pypi` | `registry_package` | `package`, `version`, `docs`, `dependencies`, `map` | `package` |
| `docker` | `registry_package` | `image`, `tag`, `namespace`, `manifest`, `map` | `image` |
| `maven` | `registry_package` | `package`, `version`, `group`, `dependencies`, `map` | `package` |
| `nuget` | `registry_package` | `package`, `version`, `owner`, `dependencies`, `map` | `package` |
| `rubygems` | `registry_package` | `package`, `version`, `owner`, `dependencies`, `map` | `package` |
| `packagist` | `registry_package` | `package`, `version`, `vendor`, `dependencies`, `map` | `package` |
| `hex` | `registry_package` | `package`, `version`, `owner`, `dependencies`, `map` | `package` |
| `pub` | `registry_package` | `package`, `version`, `publisher`, `dependencies`, `map` | `package` |
| `terraform_registry` | `registry_package` | `provider`, `module`, `version`, `namespace`, `map` | detected |
| `helm` | `registry_package` | `chart`, `version`, `repository`, `dependencies`, `map` | `chart` |
| `huggingface` | `registry_package` | `model`, `dataset`, `space`, `org`, `map` | detected |
| `reddit` | `reddit` | `subreddit`, `thread`, `user`, `search`, `map` | detected |
| `youtube` | `youtube` | `video`, `playlist`, `channel`, `captions`, `map` | detected |
| `feed` | `feed` | `feed`, `entry`, `site`, `map` | `feed` |
| `sessions` | `session` | `project`, `provider`, `file`, `upload`, `map` | detected |
| `cli_tool` | `derived` | `tool`, `script`, `command`, `run`, `help`, `schema`, `map` | `run` |
| `mcp_tool` | `derived` | `server`, `tool`, `resource`, `prompt`, `schema`, `call`, `map` | detected |
| `deepwiki` | `web`, `git` | `repo`, `org`, `index`, `map` | `repo` |

Memory is intentionally not a source adapter. Durable memory lifecycle belongs
to `axon-memory`.

## Common Scope Semantics

| Scope | Meaning |
|---|---|
| `map` | discover items/links/resources without embedding |
| `page` | one web page |
| `site` | crawl site/subtree |
| `docs` | resolve/crawl official docs |
| `repo` | source repository tree |
| `branch` | branch snapshot |
| `commit` | immutable commit snapshot |
| `org`/`group` | organization/group members |
| `package` | registry package latest/default |

`map` scopes may support watches. A watched map refreshes the candidate
manifest and source graph hints for the mapped collection. It does not publish
vectors unless the watch explicitly creates child source jobs for discovered
items.
| `version` | specific package version |
| `dependencies` | dependency metadata |
| `file` | one local/uploaded file |
| `directory`/`workspace` | local tree |
| `video`/`playlist`/`channel` | YouTube scopes |
| `subreddit`/`thread`/`user` | Reddit scopes |
| `feed`/`entry` | feed scopes |
| `tool`/`script`/`command`/`run`/`help`/`schema` | CLI tool scopes |
| `server`/`resource`/`prompt`/`call` | MCP scopes |

## Web Adapter

| Scope | Embeds | Watch | Graph Facts | Notes |
|---|---:|---:|---|---|
| `page` | yes | yes | links, canonical, metadata | one page only |
| `site` | yes | yes | internal links, sitemap, docs refs | bounded crawl |
| `docs` | yes | yes | official docs authority links | uses authority/docs mapping |
| `map` | no | yes | candidate links/sitemap refs | discovery only |

Options:

- `max_pages`
- `max_depth`
- `include_subdomains`
- `respect_robots`
- `render_mode`
- `headers`
- `sitemap`
- `url_whitelist`
- `url_blacklist`

## Local Adapter

| Scope | Embeds | Watch | Graph Facts | Notes |
|---|---:|---:|---|---|
| `file` | yes | yes | file/content facts | one file |
| `directory` | yes | yes | file tree | directory tree |
| `workspace` | yes | yes | repo/project links | workspace roots |
| `repo` | yes | yes | git repo/dependency facts | VCS-aware |
| `map` | no | yes | file tree candidates | no vectors |

Options:

- `include_globs`
- `exclude_globs`
- `respect_gitignore`
- `follow_symlinks`
- `max_file_bytes`
- `binary_policy`
- `watch_policy`

## Git Adapters

GitHub/GitLab/Gitea/generic git scopes:

| Scope | Embeds | Watch | Graph Facts | Notes |
|---|---:|---:|---|---|
| `repo` | yes | yes | repo/files/dependencies | default branch or resolved ref |
| `branch` | yes | yes | branch snapshot | mutable |
| `commit` | yes | no | immutable snapshot | no freshness needed |
| `issues` | yes | yes | issue/user/repo refs | hosted providers |
| `prs`/`mrs` | yes | yes | PR/MR/commit/issue refs | hosted providers |
| `wiki` | yes | yes | docs/wiki refs | hosted providers |
| `org`/`group` | maybe | yes | org/repo membership | discovery and optional indexing |
| `map` | no | yes | repo tree/candidates | no vectors |

Options:

- `ref`
- `include_source`
- `include_issues`
- `include_prs`
- `include_wiki`
- `include_globs`
- `exclude_globs`
- `max_files`
- `max_file_bytes`
- `private`

## Registry Adapters

Registry scopes:

| Scope | Embeds | Watch | Graph Facts | Notes |
|---|---:|---:|---|---|
| `package` | yes | yes | package/repo/docs/deps | latest/default package view |
| `version` | yes | maybe | package version/deps | immutable-ish |
| `owner`/`scope`/`group`/`namespace` | maybe | yes | ownership/package membership | discovery and optional indexing |
| `docs` | yes | yes | docs links | registry docs/readme/homepage |
| `dependencies` | yes | yes | dependency graph | manifest/registry deps |
| `map` | no | yes | package candidates | no vectors |

Registry adapters:

- crates
- npm
- pypi
- docker
- maven
- nuget
- rubygems
- packagist
- hex
- pub
- terraform registry
- helm
- huggingface

## Feed, Social, and Media Adapters

| Adapter | Scope | Embeds | Watch | Graph Facts |
|---|---|---:|---:|---|
| `feed` | `feed` | yes | yes | feed/entry/source links |
| `feed` | `entry` | yes | yes | entry/source links |
| `reddit` | `subreddit` | yes | yes | subreddit/thread/user links |
| `reddit` | `thread` | yes | yes | thread/comment/user links |
| `reddit` | `user` | yes | yes | user/post links |
| `youtube` | `video` | yes | yes | video/channel/transcript links |
| `youtube` | `playlist` | yes | yes | playlist/video/channel links |
| `youtube` | `channel` | yes | yes | channel/video links |
| `youtube` | `captions` | yes | yes | transcript facts |
| all | `map` | no | yes | candidates only |

## Session Adapter

Scopes:

| Scope | Embeds | Watch | Graph Facts | Notes |
|---|---:|---:|---|---|
| `project` | yes | yes | sessions/repos/tools/skills | project/session tree |
| `provider` | yes | yes | provider/session links | Claude/Codex/Gemini |
| `file` | yes | yes | session/turn/tool facts | one session file |
| `upload` | yes | no | session graph | uploaded export |
| `map` | no | yes | session candidates | no vectors |

Session graph facts include:

- session
- session turn
- tool call
- skill invocation
- agent invocation
- decision
- issue/PR refs
- files/artifacts touched

## CLI Tool Adapter

Scopes:

| Scope | Embeds | Watch | Executes | Graph Facts | Meaning |
|---|---:|---:|---:|---|---|
| `tool` | yes | yes | no | tool metadata | describe installed tool |
| `script` | yes | yes | no | script/file/tool refs | index script source |
| `command` | yes | yes | safe introspection only | command schema | command help/version |
| `run` | yes | yes | yes | tool_call/artifact/external refs | execute allowlisted command |
| `help` | yes | yes | help only | command schema | help/man output |
| `schema` | yes | yes | schema only | command schema | machine-readable schema |
| `map` | no | yes | no | tool candidates | no vectors |

Safety requirements:

- `run` requires allowlist policy
- declare side-effect class
- redact argv/env/stdout/stderr before storage
- store large output in ArtifactStore
- include working-directory key, not raw private path

## MCP Tool Adapter

Scopes:

| Scope | Embeds | Watch | Executes | Graph Facts | Meaning |
|---|---:|---:|---:|---|---|
| `server` | yes | yes | no | server/capability refs | server identity/capabilities |
| `tool` | yes | yes | no | tool schema refs | one MCP tool schema |
| `resource` | yes | yes | maybe | resource refs | resource metadata/content |
| `prompt` | yes | yes | no | prompt refs | prompt template/schema |
| `schema` | yes | yes | no | schema refs | tools/resources/prompts |
| `call` | yes | yes | yes | tool_call/result/artifact refs | allowlisted tool invocation |
| `map` | no | yes | no | server/tool candidates | no vectors |

MCP discovery may use clients such as `mcporter`, but the source identity and
graph evidence describe the MCP server/tool/call/result, not the helper alone.

## Prepared Uploads

Prepared uploads are source inputs, not a replacement for adapters.

| Upload Kind | Routed As |
|---|---|
| single file | `local` scope `file` |
| archive | `local` scope `directory` or `repo` after unpack policy |
| Repomix output | `upload` scope `repomix`, then original file documents |
| session export | `sessions` scope `upload` |
| WARC | `web` scope `site`/`docs` with archived fetch evidence |
| captured CLI output | `cli_tool` scope `run` |
| captured MCP response | `mcp_tool` scope `call` |
| memory import bundle | `axon-memory`, not source adapter |

## Default Scope Rules

Default scope selection order:

1. explicit `--scope`/request scope
2. adapter pattern exact match
3. source kind default
4. authority registry entrypoint
5. safe detected default
6. structured error requiring scope

Examples:

| Input | Default |
|---|---|
| `https://host/docs` | `docs` or `site` based on authority |
| `https://host/page` | `page` if page-like, else `site` |
| local file | `file` |
| local directory with `.git` | `repo` |
| local directory without `.git` | `directory` |
| GitHub repo URL | `repo` |
| package spec | `package` |
| YouTube video URL | `video` |
| RSS feed URL | `feed` |
| MCP server URI | `server` or `map` based on command |

## Scope Validation

Invalid scope error:

```json
{
  "code": "source.scope.unsupported",
  "stage": "routing",
  "message": "Adapter `youtube` does not support scope `repo`.",
  "retryable": false,
  "severity": "failed",
  "details": {
    "adapter": "youtube",
    "requested_scope": "repo",
    "supported_scopes": ["video", "playlist", "channel", "captions", "map"]
  }
}
```

Validation rules:

- unknown adapter fails at routing
- unsupported scope fails before acquisition
- invalid options fail before acquisition
- credentials missing may fail or degrade depending scope
- unsafe tool execution fails before execution
- map scope forces `embed=false` unless explicitly overridden by trusted caller

## Observability

Every adapter emits:

- selected adapter/scope
- defaulting reason
- capability version
- options hash
- credential status
- item candidates
- degraded modes used
- graph fact counts
- parser/chunk hints

## Validation Checklist

Implementation is incomplete until:

- capabilities endpoint returns adapter and scope registry
- CLI/MCP/REST validate scopes through `SourceRouter`
- every adapter emits `SourceDocument`
- every scope declares embed/watch/refresh/network/local/render/tool behavior
- every scope has option schema
- every adapter declares metadata fields and graph fact kinds
- unsafe tool/MCP call scopes require allowlist policy
- map scope writes no vectors by default
- prepared uploads route through adapters
- default scope selection is deterministic and test-covered
