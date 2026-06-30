# Source Graph Contract
Last Modified: 2026-06-30

## Contract

This is the target SourceGraph contract. A general `SourceGraph`/`GraphStore` is
not implemented today.

SourceGraph links source identities across ecosystems. It does not replace
SourceLedger. SourceLedger owns lifecycle; SourceGraph owns relationships and
evidence.

Every structured source family needs a dedicated parser/extractor that emits
graph candidates in addition to normal `SourceDocument` content. Generic
tree-sitter/code chunking is not enough for manifests, runtime topology, API
schemas, session transcripts, tool calls, skills, agents, or dependency graphs.

Each adapter/scope declares graph extraction obligations in its
`SourceScopeCapability`:

- `required_graph_fact_kinds` are part of the scope contract. If they cannot be
  parsed, the source item is degraded or failed according to the source failure
  policy.
- `optional_graph_fact_kinds` are opportunistic enrichment. Missing optional
  facts produce warnings or lower confidence, not failed ingestion.
- `unsupported_graph_fact_kinds` are never silently inferred by the generic
  pipeline. A dedicated parser must be added before Axon claims support.

Graph data answers questions like:

- What official docs, repos, packages, and local checkouts belong to shadcn?
- Which indexed sessions mention this repo?
- Which package points to this docs site?
- Is this DeepWiki page derived from an official repo or an unofficial mirror?
- Which crawled pages are aliases of the same canonical source?
- Which local worktree corresponds to this GitHub repository?

## Current Implementation Snapshot

Implemented today:

- Durable memory has a small SQLite graph-like model through memory nodes and
  edges. Current node kinds include `decision`, `fact`, `preference`, `task`,
  and `bug`; current edge kinds include `relates_to` and `supersedes`.
- Memory content is embedded into Qdrant and linked to SQLite memory metadata.

Not implemented yet:

- There is no general `GraphStore` crate/boundary, graph REST surface, graph MCP
  action, source graph query DTO, or graph-aware ask/retrieval contract.
- Source graph nodes, edges, evidence, conflict handling, authority mapping,
  and graph candidate ingestion are target architecture.
- The removed `ask.graph` field is explicitly rejected by current tests.

## Node Shape

```json
{
  "node_id": "node_...",
  "kind": "repo",
  "canonical_uri": "https://github.com/shadcn-ui/ui",
  "display_name": "shadcn-ui/ui",
  "authority": "official",
  "confidence": 0.98,
  "source_id": "src_...",
  "metadata": {},
  "created_at": "2026-06-30T16:20:00Z",
  "updated_at": "2026-06-30T16:20:00Z"
}
```

`source_id` is optional. Some graph nodes are not directly indexed sources yet.

## Edge Shape

```json
{
  "edge_id": "edge_...",
  "kind": "repo_has_docs",
  "from_node_id": "node_repo",
  "to_node_id": "node_docs",
  "authority": "inferred",
  "confidence": 0.86,
  "evidence": [
    {
      "kind": "github_homepage",
      "value": "https://ui.shadcn.com/docs",
      "source": "github_api",
      "job_id": "job_...",
      "observed_at": "2026-06-30T16:20:00Z"
    }
  ],
  "created_at": "2026-06-30T16:20:00Z",
  "updated_at": "2026-06-30T16:20:00Z"
}
```

Edges are never “just true.” They are claims with evidence, confidence, and
authority.

## Node Kinds

| Kind | Meaning | Canonical URI examples |
|---|---|---|
| `source` | Generic Axon source identity | `source://src_...` |
| `web_origin` | Website origin | `https://ui.shadcn.com` |
| `docs_site` | Documentation root/site | `https://ui.shadcn.com/docs` |
| `web_page` | Specific crawled/scraped page | `https://ui.shadcn.com/docs/components/button` |
| `repo` | Source repository | `https://github.com/shadcn-ui/ui` |
| `repo_branch` | Mutable branch ref | `https://github.com/shadcn-ui/ui/tree/main` |
| `repo_commit` | Immutable commit ref | `git+https://github.com/shadcn-ui/ui@<sha>` |
| `repo_file` | Repository file identity | `https://github.com/shadcn-ui/ui/blob/main/apps/www/content/docs/index.mdx` |
| `local_checkout` | Local worktree/checkouts | `local://<project_key>` |
| `package` | Registry package abstract identity | `npm:@radix-ui/react-slot` |
| `package_version` | Specific package version | `npm:@radix-ui/react-slot@1.2.0` |
| `registry_namespace` | Registry org/scope/owner | `npm:@radix-ui` |
| `container_image` | Docker/container image | `docker:library/postgres` |
| `container_image_tag` | Specific image tag/digest | `docker:qdrant/qdrant:v1.13.1` |
| `github_action` | GitHub Action dependency | `github-action:actions/checkout` |
| `github_action_ref` | GitHub Action pinned ref | `github-action:actions/checkout@v4` |
| `toolchain` | Runtime/toolchain dependency | `toolchain:rust`, `toolchain:node` |
| `toolchain_version` | Specific toolchain version | `toolchain:rust@1.88.0` |
| `system_package` | OS/system dependency | `apt:libssl-dev` |
| `terraform_provider` | Terraform/OpenTofu provider | `terraform-provider:hashicorp/aws` |
| `helm_chart` | Helm chart dependency | `helm:bitnami/postgresql` |
| `runtime_service` | Declared local/runtime service | `service:repo_key:postgres` |
| `network_endpoint` | Declared port/host endpoint | `endpoint:repo_key:postgres:5432` |
| `volume_mount` | Declared volume/bind mount | `volume:repo_key:postgres-data` |
| `environment_variable` | Declared env var contract | `env:repo_key:DATABASE_URL` |
| `secret_reference` | Declared secret/config reference | `secret:repo_key:POSTGRES_PASSWORD` |
| `api_surface` | API contract or service interface | `api:repo_key:public-rest` |
| `api_operation` | API route/method/rpc operation | `api-op:repo_key:GET:/v1/ask` |
| `schema_type` | Request/response/event/message schema | `schema:repo_key:AskRequest` |
| `schema_field` | Field/property within a schema | `schema-field:repo_key:AskRequest.question` |
| `protocol` | API protocol/transport | `protocol:rest`, `protocol:graphql`, `protocol:grpc` |
| `model` | Model/package source | `hf:Qwen/Qwen3-Embedding-0.6B` |
| `reddit_subreddit` | Subreddit | `reddit:r/rust` |
| `reddit_thread` | Reddit thread/post | `reddit:thread:<id>` |
| `youtube_video` | YouTube video | `youtube:video:<id>` |
| `youtube_playlist` | YouTube playlist | `youtube:playlist:<id>` |
| `youtube_channel` | YouTube channel | `youtube:channel:<id>` |
| `feed` | RSS/Atom/JSON feed | `feed:https://example.com/feed.xml` |
| `feed_entry` | Feed entry | `feed-entry:<stable_key>` |
| `session` | Claude/Codex/Gemini session | `session:<provider>:<id>` |
| `session_turn` | User/assistant/tool turn inside a session | `session-turn:<provider>:<id>:<seq>` |
| `agent` | Agent/persona/runtime capability | `agent:lavra-reviewer`, `agent:codex` |
| `agent_invocation` | Agent run observed in a session/job | `agent-invocation:<provider>:<id>:lavra-reviewer` |
| `tool` | Reusable tool/command/MCP/API capability | `tool:mcp__lumen__semantic_search` |
| `tool_call` | Tool or command invocation observed in a session | `tool-call:<provider>:<id>:<call_id>` |
| `external_resource` | External resource touched by a tool | `resource:github:issue:298`, `resource:qdrant:collection:axon` |
| `skill` | Agent skill/capability definition | `skill:lavra:lavra-review` |
| `skill_invocation` | Skill use observed in a session | `skill-invocation:<provider>:<id>:lavra:lavra-review` |
| `memory` | Durable Axon memory node | `memory:mem_...` |
| `decision` | Design/implementation decision captured in conversation | `decision:<stable_key>` |
| `issue` | GitHub/GitLab/etc. issue | `issue:https://github.com/jmagar/axon/issues/298` |
| `pull_request` | GitHub/GitLab/etc. pull request | `pr:https://github.com/jmagar/axon/pull/123` |
| `person_or_org` | Human/org/namespace identity | `github-org:shadcn-ui` |
| `derived_source` | Derived/community knowledge source | `deepwiki:https://deepwiki.com/shadcn-ui/ui` |
| `artifact` | Axon artifact/manifest/report | `artifact:<id>` |

## Edge Kinds

| Edge | From -> To | Meaning |
|---|---|---|
| `alias_of` | any -> any | Same logical source under another URI/name. |
| `canonicalizes_to` | any -> canonical node | Resolver selected this canonical source. |
| `official_for` | source -> product/package/repo | Source claims official authority. |
| `derived_from` | derived source -> official/community source | Derived/cached/mirrored from another source. |
| `mirrors` | source -> source | Mirrors equivalent or near-equivalent content. |
| `package_has_repo` | package -> repo | Package metadata links repository. |
| `package_has_docs` | package -> docs_site | Package metadata links docs. |
| `package_has_version` | package -> package_version | Version belongs to package. |
| `package_owned_by` | package -> person_or_org | Registry owner/scope/org. |
| `repo_declares_dependency` | repo -> package/container/tool/action | Repo manifest declares dependency. |
| `repo_locks_dependency_version` | repo -> package_version/container_image_tag/action_ref/toolchain_version | Lockfile pins resolved dependency version/ref. |
| `repo_uses_container_image` | repo -> container_image/container_image_tag | Compose/Docker/Kubernetes references image. |
| `repo_uses_github_action` | repo -> github_action/github_action_ref | Workflow references action. |
| `repo_uses_toolchain` | repo -> toolchain/toolchain_version | Config pins or declares runtime/toolchain. |
| `repo_uses_system_package` | repo -> system_package | Dockerfile/CI/devcontainer references OS package. |
| `repo_uses_terraform_provider` | repo -> terraform_provider | Terraform/OpenTofu config references provider. |
| `repo_uses_helm_chart` | repo -> helm_chart | Helm chart dependency. |
| `repo_declares_service` | repo -> runtime_service | Compose/devcontainer/Kubernetes declares a runtime service. |
| `service_uses_image` | runtime_service -> container_image/container_image_tag | Service runs a container image. |
| `service_exposes_endpoint` | runtime_service -> network_endpoint | Service exposes or publishes a port/host endpoint. |
| `service_mounts_volume` | runtime_service -> volume_mount | Service declares a volume, bind mount, tmpfs, or cache mount. |
| `service_requires_env` | runtime_service -> environment_variable/secret_reference | Service declares required runtime configuration. |
| `repo_declares_env_var` | repo -> environment_variable/secret_reference | Example env or config file declares an environment contract. |
| `repo_declares_api` | repo -> api_surface | Repo declares an API surface. |
| `service_exposes_api` | runtime_service -> api_surface | Runtime service exposes an API surface. |
| `api_uses_protocol` | api_surface -> protocol | API uses REST/GraphQL/gRPC/etc. |
| `api_has_operation` | api_surface -> api_operation | API contains route/query/mutation/rpc operation. |
| `operation_uses_schema` | api_operation -> schema_type | Operation uses request/response/message schema. |
| `schema_has_field` | schema_type -> schema_field | Schema contains field/property. |
| `package_generates_api_client` | package -> api_surface | Package is a generated client/server SDK for an API. |
| `repo_has_docs` | repo -> docs_site | Repo metadata or docs point to site. |
| `repo_has_wiki` | repo -> docs_site | Repo wiki/docs relation. |
| `repo_owned_by` | repo -> person_or_org | Repo owner/org. |
| `repo_has_branch` | repo -> repo_branch | Branch belongs to repo. |
| `branch_points_to_commit` | repo_branch -> repo_commit | Branch observed at commit. |
| `commit_contains_file` | repo_commit -> repo_file | File observed in commit. |
| `local_checkout_tracks_repo` | local_checkout -> repo | Local git remote points to repo. |
| `local_checkout_at_commit` | local_checkout -> repo_commit | Local checkout observed at commit. |
| `docs_site_contains_page` | docs_site -> web_page | Page belongs to docs site/root. |
| `web_origin_has_docs` | web_origin -> docs_site | Origin maps to docs root. |
| `feed_contains_entry` | feed -> feed_entry | Feed entry belongs to feed. |
| `youtube_channel_has_video` | youtube_channel -> youtube_video | Channel owns video. |
| `youtube_playlist_has_video` | youtube_playlist -> youtube_video | Playlist includes video. |
| `subreddit_has_thread` | reddit_subreddit -> reddit_thread | Thread belongs to subreddit. |
| `session_has_turn` | session -> session_turn | Turn belongs to session. |
| `session_about_repo` | session -> repo/local_checkout | Session project/git metadata maps to repo or checkout. |
| `session_mentions_repo` | session -> repo | Session references repo. |
| `session_mentions_source` | session -> source | Session references source. |
| `session_mentions_issue` | session -> issue | Session references an issue. |
| `session_mentions_pr` | session -> pull_request | Session references a pull request. |
| `session_mentions_package` | session -> package | Session references a package/dependency. |
| `session_produced_decision` | session -> decision | Session contains durable design/implementation decision. |
| `session_invoked_agent` | session -> agent_invocation | Session delegated work to an agent. |
| `agent_invocation_uses_agent` | agent_invocation -> agent | Invocation points to reusable agent definition. |
| `agent_invocation_used_skill` | agent_invocation -> skill_invocation | Agent run used a skill. |
| `agent_invocation_used_tool` | agent_invocation -> tool_call | Agent run used a tool. |
| `agent_invocation_produced_artifact` | agent_invocation -> artifact | Agent run produced a review/report/patch/etc. |
| `agent_invocation_related_to_repo` | agent_invocation -> repo/local_checkout | Agent run was scoped to a repo/check-out. |
| `agent_invocation_related_to_issue` | agent_invocation -> issue/pull_request | Agent run was scoped to an issue or PR. |
| `session_invoked_skill` | session -> skill_invocation | Session used an agent skill. |
| `skill_invocation_uses_skill` | skill_invocation -> skill | Invocation points to reusable skill definition. |
| `skill_invocation_produced_artifact` | skill_invocation -> artifact | Skill run produced a plan/review/report/patch/etc. |
| `skill_invocation_related_to_repo` | skill_invocation -> repo/local_checkout | Skill run was scoped to a repo/check-out. |
| `skill_invocation_related_to_issue` | skill_invocation -> issue/pull_request | Skill run was scoped to an issue or PR. |
| `turn_invoked_tool` | session_turn -> tool_call | Turn caused a tool/command invocation. |
| `turn_invoked_skill` | session_turn -> skill_invocation | Turn explicitly invoked or selected a skill. |
| `tool_call_uses_tool` | tool_call -> tool | Invocation points to reusable tool definition. |
| `tool_call_touched_file` | tool_call -> repo_file | Tool/command read or changed a file when evidence is available. |
| `tool_call_produced_artifact` | tool_call -> artifact | Tool/command produced output, report, patch, screenshot, etc. |
| `tool_call_read_resource` | tool_call -> external_resource/source | Tool read external state. |
| `tool_call_mutated_resource` | tool_call -> external_resource/source | Tool changed external state. |
| `tool_call_related_to_repo` | tool_call -> repo/local_checkout | Tool run was scoped to a repo/check-out. |
| `tool_call_related_to_issue` | tool_call -> issue/pull_request | Tool run referenced or updated an issue/PR. |
| `memory_relates_to` | memory -> memory/source/graph node | Memory relates to another memory or source entity. |
| `memory_supersedes` | memory -> memory | Replacement memory supersedes an older memory. |
| `memory_contradicts` | memory -> memory | Memories conflict and require review or resolution. |
| `memory_compacts` | memory -> memory | Compacted/distilled memory summarizes source memories. |
| `memory_about_source` | memory -> source/repo/local_checkout/docs_site/package | Memory is about a source entity. |
| `memory_about_file` | memory -> repo_file | Memory is about a specific file. |
| `memory_about_issue` | memory -> issue/pull_request | Memory is about an issue or PR. |
| `memory_used_in_context` | memory -> session/agent_invocation/job | Memory was recalled into a context. |
| `source_produced_artifact` | source -> artifact | Job produced manifest/report/screenshot/etc. |
| `source_indexed_as` | source -> source_id node | SourceGraph node maps to ledger source. |

## Authority Levels

| Level | Meaning |
|---|---|
| `official` | Claimed by package/repo/site owner or user-pinned as official. |
| `user_pinned` | User explicitly declared relation. |
| `inferred` | Derived from metadata, redirects, sitemap, llms.txt, or content evidence. |
| `community` | Community/third-party but useful. |
| `mirror` | Mirror/cache/copy of another source. |
| `unknown` | Relation exists but authority is not established. |
| `conflicting` | Evidence disagrees; do not silently pick a winner. |

## Evidence Kinds

| Evidence | Example |
|---|---|
| `user_pinned` | User maps `shadcn.com` to `https://ui.shadcn.com/docs`. |
| `redirect` | `shadcn.com/docs` redirects to `ui.shadcn.com/docs`. |
| `html_canonical` | Page canonical link. |
| `sitemap` | URL appears in sitemap. |
| `robots` | Robots points to sitemap. |
| `llms_txt` | URL appears in `/llms.txt`. |
| `github_homepage` | GitHub repo homepage field. |
| `github_topics` | Repo topics indicate package/docs relation. |
| `package_repository` | Registry package repository field. |
| `package_homepage` | Registry package homepage/docs field. |
| `dependency_manifest` | Dependency declared in a manifest file. |
| `dependency_lockfile` | Dependency pinned/resolved in a lockfile. |
| `container_manifest` | Container image declared in Compose/Kubernetes/Dockerfile. |
| `runtime_manifest` | Compose/devcontainer/Kubernetes declares services, ports, volumes, env, health, dependencies. |
| `env_example` | `.env.example` or equivalent declares runtime/env contract. |
| `api_schema` | OpenAPI/GraphQL/protobuf/JSON Schema/etc. declares API contract. |
| `framework_route` | Source code route declaration exposes API operation. |
| `ci_workflow` | GitHub/GitLab/etc. workflow declares tool/action/package. |
| `toolchain_manifest` | Toolchain/runtime declared in project config. |
| `docs_linkback` | Docs page links back to repo/package. |
| `local_git_remote` | Local checkout remote URL. |
| `local_git_commit` | Local checkout commit SHA. |
| `session_metadata` | Session project/git metadata. |
| `session_jsonl` | Claude/Codex JSONL transcript event. |
| `session_json` | Gemini JSON transcript event. |
| `agent_invocation_event` | Agent dispatch/start/result marker observed in a session. |
| `tool_call_event` | Tool call, shell command, MCP call, or result observed in a session. |
| `tool_result_event` | Tool result/status/error observed in a session. |
| `skill_invocation_event` | Skill invocation marker or explicit skill reference observed in a session. |
| `conversation_reference` | Issue, PR, URL, file path, package, source, or error referenced in conversation. |
| `text_mention` | Content mention above confidence threshold. |
| `derived_source_attribution` | DeepWiki or similar links back to repo. |

## Required Graph Updates by Adapter

| Adapter | Required graph candidates |
|---|---|
| `web` | web origin, docs site/page containment, aliases/canonical links, sitemap/llms.txt evidence. |
| `local` | local checkout, git remote, current commit, repo edge when available, dependency graph from manifests. |
| `github`/`gitlab`/`gitea` | repo, owner/org, branch/commit/file nodes, homepage/docs/wiki edges, dependency graph from manifests. |
| `generic_git` | repo, branch/commit/file nodes, remote URL evidence, dependency graph from manifests. |
| `crates`/`npm`/`pypi` | package, version, namespace, repo/docs/homepage edges. |
| `docker` | image, tag, namespace, repo/docs edges when metadata exposes them. |
| `huggingface` | model/dataset/space, owner, repo/docs/card edges. |
| `reddit` | subreddit/thread/user edges. |
| `youtube` | video/playlist/channel edges. |
| `feed` | feed/entry/source page edges. |
| `sessions` | session/project/local checkout/repo/source mention edges. |
| `deepwiki` | derived_source -> repo/docs/source attribution edges. |

## Dependency Graph Extraction

Repo/local-code adapters must extract dependency relationships from known
manifest and lock files. This is how Axon answers “list all packages this
project uses” without relying on semantic chunk inference.

Dependency edges distinguish declared intent from resolved/pinned reality:

- `repo_declares_dependency`: manifest declaration, often a range or unpinned
  name.
- `repo_locks_dependency_version`: lockfile/resolved output with exact version,
  digest, commit, or action ref.

Each dependency edge must include:

- manifest path
- manifest kind
- dependency group: runtime, dev, build, optional, peer, workspace, test,
  transitive, toolchain, service, ci
- declared requirement/range when present
- resolved version/ref/digest when present
- package manager/ecosystem
- direct vs transitive when known
- source item key and line/range when known
- job id and evidence

### Required Manifest Support Matrix

| Ecosystem | Files | Nodes | Edges |
|---|---|---|---|
| Rust | `Cargo.toml` | `package`, `toolchain` | `repo_declares_dependency`, `repo_uses_toolchain` |
| Rust | `Cargo.lock` | `package_version` | `repo_locks_dependency_version` |
| Rust | `rust-toolchain`, `rust-toolchain.toml`, `.cargo/config.toml`, `.cargo/config` | `toolchain_version`, `system_package` | `repo_uses_toolchain`, dependency edges for registries when present |
| Node/npm | `package.json` | `package`, `toolchain` | `repo_declares_dependency`, `repo_uses_toolchain` |
| Node/npm | `package-lock.json`, `npm-shrinkwrap.json` | `package_version` | `repo_locks_dependency_version` |
| Node/npm | `.npmrc`, `.node-version`, `.nvmrc` | `toolchain_version`, `registry_namespace` | `repo_uses_toolchain`, registry/config evidence |
| pnpm | `pnpm-lock.yaml`, `pnpm-workspace.yaml` | `package_version`, `package` | `repo_locks_dependency_version`, workspace package edges |
| Yarn | `yarn.lock`, `.yarnrc.yml` | `package_version`, `toolchain_version` | `repo_locks_dependency_version`, `repo_uses_toolchain` |
| Bun | `bun.lock`, `bun.lockb` | `package_version` | `repo_locks_dependency_version` |
| Python | `pyproject.toml` | `package`, `toolchain` | `repo_declares_dependency`, `repo_uses_toolchain` |
| Python | `requirements.txt`, `requirements/*.txt`, `constraints.txt`, `constraints/*.txt` | `package`, `package_version` | declared or pinned dependency edges |
| Python | `poetry.lock`, `uv.lock`, `Pipfile.lock` | `package_version` | `repo_locks_dependency_version` |
| Python | `Pipfile`, `setup.py`, `setup.cfg`, `tox.ini`, `noxfile.py` | `package`, `toolchain` | `repo_declares_dependency`, test/toolchain edges |
| Python | `.python-version`, `runtime.txt` | `toolchain_version` | `repo_uses_toolchain` |
| Go | `go.mod` | `package`, `toolchain_version` | `repo_declares_dependency`, `repo_uses_toolchain` |
| Go | `go.sum` | `package_version` | `repo_locks_dependency_version` |
| Java/Maven | `pom.xml` | `package` | `repo_declares_dependency` |
| Java/Maven | `mvnw`, `.mvn/wrapper/maven-wrapper.properties` | `toolchain_version` | `repo_uses_toolchain` |
| Java/Gradle | `build.gradle`, `build.gradle.kts`, `settings.gradle`, `settings.gradle.kts`, `gradle/libs.versions.toml` | `package`, `toolchain` | `repo_declares_dependency`, `repo_uses_toolchain` |
| Java/Gradle | `gradle/wrapper/gradle-wrapper.properties` | `toolchain_version` | `repo_uses_toolchain` |
| .NET | `*.csproj`, `*.fsproj`, `*.vbproj`, `packages.lock.json`, `Directory.Packages.props` | `package`, `package_version` | declared/locked dependency edges |
| .NET | `global.json`, `NuGet.config` | `toolchain_version`, `registry_namespace` | `repo_uses_toolchain`, registry/config evidence |
| Ruby | `Gemfile` | `package` | `repo_declares_dependency` |
| Ruby | `Gemfile.lock` | `package_version` | `repo_locks_dependency_version` |
| Ruby | `.ruby-version`, `.tool-versions` | `toolchain_version` | `repo_uses_toolchain` |
| PHP | `composer.json` | `package` | `repo_declares_dependency` |
| PHP | `composer.lock` | `package_version` | `repo_locks_dependency_version` |
| Elixir | `mix.exs` | `package` | `repo_declares_dependency` |
| Elixir | `mix.lock` | `package_version` | `repo_locks_dependency_version` |
| Swift | `Package.swift`, `Package.resolved` | `package`, `package_version` | declared/locked dependency edges |
| Dart/Flutter | `pubspec.yaml`, `pubspec.lock` | `package`, `package_version` | declared/locked dependency edges |
| R | `DESCRIPTION`, `renv.lock` | `package`, `package_version` | declared/locked dependency edges |
| Nix | `flake.nix`, `flake.lock`, `shell.nix`, `default.nix` | `package`, `toolchain_version` | dependency/toolchain edges |
| Mise/asdf | `mise.toml`, `.mise.toml`, `.tool-versions` | `toolchain`, `toolchain_version` | `repo_uses_toolchain` |
| Docker | `Dockerfile`, `Containerfile` | `container_image`, `container_image_tag`, `system_package` | `repo_uses_container_image`, `repo_uses_system_package` |
| Compose | `docker-compose.yml`, `docker-compose.yaml`, `compose.yml`, `compose.yaml` | `runtime_service`, `container_image`, `container_image_tag`, `network_endpoint`, `volume_mount`, `environment_variable`, `secret_reference` | service/runtime topology edges |
| Env examples | `.env.example`, `.env.sample`, `.env.template`, `env.example`, `example.env`, `*.env.example`, `*.env.sample`, `*.env.template` | `environment_variable`, `secret_reference` | `repo_declares_env_var` |
| Direnv | `.envrc` | `environment_variable`, `toolchain` | `repo_declares_env_var`, `repo_uses_toolchain` |
| OpenAPI/Swagger | `openapi.yaml`, `openapi.yml`, `openapi.json`, `swagger.yaml`, `swagger.yml`, `swagger.json`, `api/openapi.*`, `docs/openapi.*` | `api_surface`, `api_operation`, `schema_type`, `schema_field` | API/schema edges |
| JSON Schema | `*.schema.json`, `schema/*.json`, `schemas/*.json`, `schemas/**/*.json` | `schema_type`, `schema_field` | schema field/type edges |
| GraphQL | `schema.graphql`, `schema.gql`, `*.graphql`, `*.gql` | `api_surface`, `api_operation`, `schema_type`, `schema_field` | API/schema edges |
| Protobuf/gRPC | `*.proto`, `proto/**/*.proto`, `protobuf/**/*.proto` | `api_surface`, `api_operation`, `schema_type`, `schema_field` | API/schema edges |
| AsyncAPI | `asyncapi.yaml`, `asyncapi.yml`, `asyncapi.json` | `api_surface`, `api_operation`, `schema_type`, `schema_field` | event/message API edges |
| Avro | `*.avsc`, `avro/**/*.avsc` | `schema_type`, `schema_field` | schema field/type edges |
| Thrift | `*.thrift` | `api_surface`, `api_operation`, `schema_type`, `schema_field` | API/schema edges |
| Smithy | `*.smithy`, `smithy/**/*.smithy` | `api_surface`, `api_operation`, `schema_type`, `schema_field` | API/schema edges |
| Connect/gRPC-Web configs | `buf.yaml`, `buf.gen.yaml`, `buf.work.yaml`, `buf.lock` | `api_surface`, `toolchain`, `package` | API/toolchain/client-generation edges |
| Claude sessions | `~/.claude/projects/**/*.jsonl`, exported Claude `.jsonl` | `session`, `session_turn`, `agent`, `agent_invocation`, `tool`, `tool_call`, `skill`, `skill_invocation`, `decision`, `repo`, `local_checkout`, `issue`, `pull_request`, `artifact` | session/project/agent/tool/skill/reference edges |
| Codex sessions | `~/.codex/sessions/**/*.jsonl`, exported Codex `.jsonl` | `session`, `session_turn`, `agent`, `agent_invocation`, `tool`, `tool_call`, `skill`, `skill_invocation`, `decision`, `repo`, `local_checkout`, `issue`, `pull_request`, `artifact` | session/project/agent/tool/skill/reference edges |
| Gemini sessions | `~/.gemini/history/**/*.json`, `~/.gemini/tmp/**/*.json`, exported Gemini `.json` | `session`, `session_turn`, `agent`, `agent_invocation`, `tool`, `tool_call`, `skill`, `skill_invocation`, `decision`, `repo`, `local_checkout`, `issue`, `pull_request`, `artifact` | session/project/agent/tool/skill/reference edges |
| Kubernetes | `*.yaml`, `*.yml` under `k8s/`, `manifests/`, `deploy/`, `charts/` | `runtime_service`, `container_image`, `container_image_tag`, `network_endpoint`, `volume_mount`, `environment_variable`, `secret_reference` | service/runtime topology edges |
| GitHub Actions | `.github/workflows/*.yml`, `.github/workflows/*.yaml` | `github_action`, `github_action_ref`, `toolchain` | `repo_uses_github_action`, `repo_uses_toolchain` |
| GitLab CI | `.gitlab-ci.yml` | `container_image`, `toolchain` | `repo_uses_container_image`, `repo_uses_toolchain` |
| CircleCI | `.circleci/config.yml` | `container_image`, `toolchain` | container/toolchain edges |
| Buildkite | `.buildkite/*.yml`, `.buildkite/*.yaml`, `buildkite.yml`, `buildkite.yaml` | `container_image`, `toolchain` | container/toolchain edges |
| Dev Containers | `.devcontainer/devcontainer.json`, `.devcontainer/docker-compose.yml`, `.devcontainer/docker-compose.yaml` | `runtime_service`, `container_image`, `toolchain`, `environment_variable` | service/container/toolchain edges |
| Terraform/OpenTofu | `*.tf`, `.terraform.lock.hcl` | `terraform_provider`, `package_version` | `repo_uses_terraform_provider`, locked provider edges |
| Helm | `Chart.yaml`, `Chart.lock` | `helm_chart`, `package_version` | `repo_uses_helm_chart`, locked chart edges |

### Runtime and Environment Contract Extraction

`docker-compose*.yml`, Compose-adjacent devcontainer files, Kubernetes manifests,
and env example files describe how a repo is expected to run. They should be
parsed into graph structure, not stored only as text chunks.

Compose extraction must capture at least:

- services: service name, profiles, image, build context, dockerfile, target,
  platform, restart policy, command, entrypoint, user, working dir
- images: repository, tag, digest, registry, pull policy when present
- build dependencies: build args, extra contexts, target stage, referenced
  Dockerfile path
- ports/endpoints: published port, target port, protocol, host IP, expose-only
  ports, inferred localhost URLs when safe
- service relationships: `depends_on`, health-gated dependencies, links,
  network aliases, `extends`
- env contract: `environment`, `env_file`, interpolation variables such as
  `${POSTGRES_PASSWORD}`, defaulted variables such as `${PORT:-3000}`
- secrets/configs: Compose `secrets`, `configs`, secret file paths, external
  secret names
- persistence: named volumes, bind mounts, tmpfs mounts, cache mounts,
  read-only flags, container paths, host paths when safe to expose
- networks: declared networks, external networks, aliases, internal/public
  boundary hints
- health/operations: healthcheck command, interval, timeout, retries,
  start period
- resources/security: privileged, capabilities, devices, GPU reservations,
  memory/cpu limits, security options
- labels: reverse-proxy labels, Traefik/Caddy/SWAG hints, service discovery
  metadata

Env example extraction must capture at least:

- variable name
- default/example value, redacted when secret-like
- whether it is required, optional, empty, or defaulted
- category: service URL, credential, token, database, cache, queue, path,
  feature flag, tuning knob, model/provider, unknown
- referenced service when inferable from name or Compose interpolation
- description/comment immediately above or beside the variable
- source file path and line number

Secret-like values must never be copied verbatim into graph metadata. Store a
shape only: present/empty/example/redacted, plus provider/category hints.

Runtime/environment edges should let Axon answer:

- what services does this repo run locally?
- what ports or URLs does it expose?
- which env vars are required to start it?
- which env vars are secrets?
- which service consumes each env var?
- which services depend on Postgres/Redis/Qdrant/TEI/etc.?
- which repos use a given image, port, env var, or service name?

### API and Schema Contract Extraction

API/schema files describe how another system talks to this source. They should
be parsed into graph structure so Axon can answer interface questions without
guessing from prose or code chunks.

OpenAPI/Swagger extraction must capture at least:

- API title, version, description, servers/base URLs, tags
- path, method, operation id, summary, deprecation status
- path/query/header/cookie parameters
- request body content types and schema references
- response status codes, content types, schema references, error shape
- auth/security schemes and required scopes
- examples when small enough and non-secret
- schema component names, fields, required fields, enum values, nullable flags
- references between operations and request/response schemas

GraphQL extraction must capture at least:

- query, mutation, and subscription operation fields
- object/interface/union/input/enum/scalar types
- field arguments, nullability, list shape, default values
- resolver ownership when source code evidence can connect it
- directive usage, auth directives, deprecation reasons

Protobuf/gRPC extraction must capture at least:

- package name, service name, rpc method name
- unary/server-stream/client-stream/bidi-stream kind
- request and response message types
- message fields, field numbers, oneofs, map fields, enum values
- imported proto files and package references
- HTTP transcoding annotations when present

JSON Schema / Avro / Thrift / Smithy extraction must capture at least:

- schema/type name and namespace
- fields/properties and required/optional/nullability shape
- enum values, default values, format hints, constraints
- referenced schemas and inheritance/composition relations

Framework route extraction is allowed when no explicit schema file exists, but
it must be evidence-ranked lower than an explicit schema. Examples include Axum,
Express/Fastify, Next.js route handlers, Rails routes, Django/DRF/FastAPI,
Spring controllers, ASP.NET controllers/minimal APIs, and Go router
registrations.

API/schema edges should let Axon answer:

- what REST endpoints does this repo expose?
- which service exposes `/v1/ask`?
- what request/response schema does an operation use?
- which fields are required for `AskRequest`?
- which endpoints require auth or a scope?
- which package appears to be a generated client for this API?
- which repos expose or consume a given API operation/schema?

### Tool Invocation Graph Extraction

Tool calls are execution evidence. They should be represented as reusable tool
definitions plus concrete invocations observed in a session/job.

Tool extraction must capture at least:

- tool identity: name, namespace, provider/server, transport, version when known
- invocation identity: call id, parent turn, sequence, timestamp, duration
- invocation kind: shell command, MCP tool, REST call, browser action, file edit,
  git/GitHub action, database/vector-store operation, local helper
- input shape: argument keys, target paths/URLs/resources, redacted values,
  current working directory, environment shape when relevant
- result shape: status, exit code, error kind, retry count, stdout/stderr/result
  summary, truncated/redacted output hash when useful
- side effects: files read/written/deleted, commands run, resources read,
  resources mutated, artifacts produced
- scope: repo/local checkout, issue/PR/job/source when inferable
- safety: destructive flag, external network mutation flag, secret-redaction
  status

Tool calls should create `external_resource` nodes when they touch things outside
the repo, such as GitHub issues/PRs/runs, Qdrant collections, Docker
containers, web pages, MCP resources, cloud objects, databases, or notification
targets.

Standalone `cli_tool` and `mcp_tool` source jobs use this same graph model even
when the tool call did not originate inside a Claude/Codex/Gemini session. The
source job creates the reusable `tool` node, the concrete `tool_call` node, any
`external_resource` or `artifact` nodes, and provenance edges back to the
`source`, `job`, and `artifact` records. Calling an MCP tool through a helper
such as `mcporter` is implementation detail; graph evidence must describe the
MCP server/tool/call/result, not the helper alone.

Tool edges should let Axon answer:

- which tools were used during this session/job?
- which tool touched this file or external resource?
- which commands failed, retried, or produced warnings?
- which tool mutated a GitHub issue, PR, branch, Qdrant collection, or Docker
  service?
- which artifacts came from tool output rather than assistant prose?

### Session Transcript Graph Extraction

Claude/Codex session `.jsonl` files and Gemini session `.json` files are both
documents to embed and graph evidence to mine. The transcript text flows through
`SourceDocument -> DocumentPreparer -> PreparedDocument`; the session parser
emits `SourceParseFacts` and `GraphCandidate` values in parallel.

Session extraction must capture at least:

- provider: Claude, Codex, Gemini, or future agent runtime
- stable session id, transcript path, mtime, byte size, and parser version
- project name and project path when present or decodable
- git root, remote URL, normalized repo slug, branch, and commit when available
- turn sequence, role, timestamp, model, and redacted text summary
- agents invoked: agent name, provider/runtime, model when known, role/purpose,
  dispatch prompt shape, parent/child relationship, outcome, and produced
  artifacts
- tool calls: tool name, command/action, arguments shape, exit status,
  duration, and redacted result summary
- skills invoked: skill name, plugin/provider namespace, version when known,
  skill path when safe, trigger text, invocation phase, outcome, and produced
  artifacts
- file paths read, written, patched, generated, or mentioned
- issue/PR/URL references and their normalized canonical URIs
- package/source/API/service/env references when detectable
- explicit decisions, TODOs, plans, reviews, errors, and resolution notes when
  confidence is high enough
- artifacts produced: plans, review reports, screenshots, patches, logs,
  generated docs, or uploaded files

Secret-like values inside transcripts must be redacted before embedding and
before graph metadata persistence. Tool arguments/results should preserve shape
and evidence, not raw secret values.

Session edges should let Axon answer:

- which sessions discussed this repo, issue, PR, package, file, or API?
- what decisions were made about this pipeline and when?
- which agents were invoked for this repo/issue/PR and what did they produce?
- which tool calls touched a file or produced a doc/artifact?
- which skills were invoked, in what order, and what did they produce?
- which repos/issues/PRs commonly use a given skill?
- which errors kept recurring across sessions?
- which repo or local checkout was a session about?
- what context should be recalled when starting work in this repo?

### Dependency Query Contract

The graph should support these queries without vector search:

- list direct dependencies for repo/local checkout
- list locked versions for repo/local checkout
- list runtime vs dev/build/test/toolchain/service dependencies
- list container images used by repo
- list GitHub Actions used by repo
- list local runtime services and their dependencies
- list required env vars and secret placeholders
- explain which manifest/env file introduced a service, port, image, or env var
- list API surfaces, REST routes, RPC methods, schemas, and required fields
- list sessions, decisions, tool calls, issues, PRs, and artifacts linked to a
  repo/source/file/API/package
- list agents invoked for a repo/source/issue/PR and artifacts produced by
  those agent runs
- list skills invoked for a repo/source/issue/PR and artifacts produced by
  those skill runs
- list dependency manifests that introduced a dependency
- explain why Axon believes repo uses package X
- find repos using package/image/action/toolchain X

Vector search may enrich these answers with docs/snippets, but the package list
itself should come from SourceGraph.

## Conflict Rules

- Preserve conflicting evidence; do not overwrite it with the newest claim.
- User-pinned mappings win for routing, but graph should still retain conflicting
  non-user evidence.
- Official package/repo metadata outranks community/derived sources.
- Derived sources such as DeepWiki should not become official unless official
  evidence exists.
- Low-confidence text mentions should not create authoritative edges.

## Ledger and Vector Links

Graph nodes should carry `source_id` when they correspond to a ledger source.
Graph edges should carry `job_id` and evidence so users can inspect why the edge
exists.

VectorStore payloads should include enough graph keys to filter/search by:

- `source_id`
- `canonical_uri`
- `source_kind`
- `source_authority`
- `graph_node_id` when known
- `graph_edge_ids` when a chunk is directly evidence for an edge
