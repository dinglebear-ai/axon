# Source Graph

Last Modified: 2026-08-02

The source graph records relationships between sources, documents, entities,
and extracted facts. Edges are never "just true" — they are evidence-backed
claims with authority and confidence.

> Authoritative schema: [`graph.schema.json`](graph.schema.json). Contract
> source: [`docs/reference/sources/source-graph.md`](source-graph.md).
> Implementation: [`crates/axon-graph/src/`](../../../crates/axon-graph/src/)
> (`SqliteGraphStore` is the live tested impl; Phase 7 landed).

## Nodes and edges

**Node:** `node_id`, `kind`, `canonical_uri`, `display_name`, `authority`,
`confidence`, `source_id` (optional — some nodes aren't directly indexed
sources), `metadata`, `created_at`, `updated_at`.

**Edge:** `edge_id`, `kind`, `from_node_id`, `to_node_id`, `authority`,
`confidence`, `evidence[]`, `created_at`, `updated_at`. Each evidence item
carries `kind`, `value`, `source`, `job_id`, `observed_at`.

`GraphNodeKind` and `GraphEdgeKind` are **closed** Rust enums — currently
**55 node kinds** and **83 edge kinds**.

## Node Kinds

`GraphNodeKind` is a closed registry with **55 kinds**. Every node
kind requires evidence before merge.

| Kind | Category |
|---|---|
| `source` | Source and artifact |
| `web_origin` | Web and feed |
| `docs_site` | Web and feed |
| `web_page` | Web and feed |
| `repo` | Repository and build |
| `repo_branch` | Repository and build |
| `repo_commit` | Repository and build |
| `repo_file` | Repository and build |
| `local_checkout` | Repository and build |
| `package` | Package and registry |
| `package_version` | Package and registry |
| `registry_namespace` | Package and registry |
| `container_image` | Package and registry |
| `container_image_tag` | Package and registry |
| `github_action` | Repository and build |
| `github_action_ref` | Repository and build |
| `toolchain` | Repository and build |
| `toolchain_version` | Repository and build |
| `system_package` | Repository and build |
| `terraform_provider` | Package and registry |
| `helm_chart` | Package and registry |
| `runtime_service` | Runtime infrastructure |
| `network_endpoint` | Runtime infrastructure |
| `volume_mount` | Runtime infrastructure |
| `environment_variable` | Runtime infrastructure |
| `secret_reference` | Runtime infrastructure |
| `api_surface` | API and schema |
| `api_operation` | API and schema |
| `schema_type` | API and schema |
| `schema_field` | API and schema |
| `protocol` | API and schema |
| `model` | API and schema |
| `reddit_subreddit` | Social and media |
| `reddit_thread` | Social and media |
| `youtube_video` | Social and media |
| `youtube_playlist` | Social and media |
| `youtube_channel` | Social and media |
| `feed` | Web and feed |
| `feed_entry` | Web and feed |
| `session` | Agent and session |
| `session_turn` | Agent and session |
| `agent` | Agent and session |
| `agent_invocation` | Agent and session |
| `tool` | Agent and session |
| `tool_call` | Agent and session |
| `external_resource` | Source and artifact |
| `skill` | Agent and session |
| `skill_invocation` | Agent and session |
| `memory` | Knowledge and collaboration |
| `decision` | Knowledge and collaboration |
| `issue` | Knowledge and collaboration |
| `pull_request` | Knowledge and collaboration |
| `person_or_org` | Knowledge and collaboration |
| `derived_source` | Source and artifact |
| `artifact` | Source and artifact |

Naming rule: no schema may use `site`/`repository`/`file`/`api_endpoint` when
the registry names are `web_origin`/`repo`/`repo_file`/`api_operation`.
## Edge Kinds

`GraphEdgeKind` is a closed registry with **83 kinds**. Every edge
kind requires at least one evidence record.

| Kind | Category |
|---|---|
| `alias_of` | Identity and provenance |
| `canonicalizes_to` | Identity and provenance |
| `official_for` | Identity and provenance |
| `derived_from` | Identity and provenance |
| `mirrors` | Identity and provenance |
| `package_has_repo` | Package relationships |
| `package_has_docs` | Package relationships |
| `package_has_version` | Package relationships |
| `package_owned_by` | Package relationships |
| `repo_declares_dependency` | Repository relationships |
| `repo_locks_dependency_version` | Repository relationships |
| `repo_uses_container_image` | Repository relationships |
| `repo_uses_github_action` | Repository relationships |
| `repo_uses_toolchain` | Repository relationships |
| `repo_uses_system_package` | Repository relationships |
| `repo_uses_terraform_provider` | Repository relationships |
| `repo_uses_helm_chart` | Repository relationships |
| `repo_declares_service` | Repository relationships |
| `service_uses_image` | Runtime and API relationships |
| `service_exposes_endpoint` | Runtime and API relationships |
| `service_mounts_volume` | Runtime and API relationships |
| `service_requires_env` | Runtime and API relationships |
| `repo_declares_env_var` | Repository relationships |
| `repo_declares_api` | Repository relationships |
| `service_exposes_api` | Runtime and API relationships |
| `api_uses_protocol` | Runtime and API relationships |
| `api_has_operation` | Runtime and API relationships |
| `operation_uses_schema` | Runtime and API relationships |
| `schema_has_field` | Runtime and API relationships |
| `package_generates_api_client` | Package relationships |
| `repo_has_docs` | Repository relationships |
| `repo_has_wiki` | Repository relationships |
| `repo_owned_by` | Repository relationships |
| `repo_has_branch` | Repository relationships |
| `branch_points_to_commit` | Repository relationships |
| `commit_contains_file` | Repository relationships |
| `local_checkout_tracks_repo` | Repository relationships |
| `local_checkout_at_commit` | Repository relationships |
| `docs_site_contains_page` | Web and feed relationships |
| `web_origin_has_docs` | Other |
| `feed_contains_entry` | Web and feed relationships |
| `youtube_channel_has_video` | Other |
| `youtube_playlist_has_video` | Other |
| `subreddit_has_thread` | Other |
| `session_has_turn` | Session relationships |
| `session_about_repo` | Session relationships |
| `session_mentions_repo` | Session relationships |
| `session_mentions_source` | Session relationships |
| `session_mentions_issue` | Session relationships |
| `session_mentions_pr` | Session relationships |
| `session_mentions_package` | Session relationships |
| `session_produced_decision` | Session relationships |
| `session_invoked_agent` | Session relationships |
| `agent_invocation_uses_agent` | Agent and tool relationships |
| `agent_invocation_used_skill` | Agent and tool relationships |
| `agent_invocation_used_tool` | Agent and tool relationships |
| `agent_invocation_produced_artifact` | Agent and tool relationships |
| `agent_invocation_related_to_repo` | Agent and tool relationships |
| `agent_invocation_related_to_issue` | Agent and tool relationships |
| `session_invoked_skill` | Session relationships |
| `skill_invocation_uses_skill` | Other |
| `skill_invocation_produced_artifact` | Other |
| `skill_invocation_related_to_repo` | Other |
| `skill_invocation_related_to_issue` | Other |
| `turn_invoked_tool` | Other |
| `turn_invoked_skill` | Other |
| `tool_call_uses_tool` | Agent and tool relationships |
| `tool_call_touched_file` | Agent and tool relationships |
| `tool_call_produced_artifact` | Agent and tool relationships |
| `tool_call_read_resource` | Agent and tool relationships |
| `tool_call_mutated_resource` | Agent and tool relationships |
| `tool_call_related_to_repo` | Agent and tool relationships |
| `tool_call_related_to_issue` | Agent and tool relationships |
| `memory_relates_to` | Memory relationships |
| `memory_supersedes` | Memory relationships |
| `memory_contradicts` | Memory relationships |
| `memory_compacts` | Memory relationships |
| `memory_about_source` | Memory relationships |
| `memory_about_file` | Memory relationships |
| `memory_about_issue` | Memory relationships |
| `memory_used_in_context` | Memory relationships |
| `source_produced_artifact` | Source relationships |
| `source_indexed_as` | Source relationships |
## Authority, evidence, merge

**Authority levels (8):** `official`, `verified`, `user_pinned`, `inferred`,
`community`, `mirror`, `unknown`, `conflicting`.

**Evidence kinds (32):** `user_pinned`, `redirect`, `html_canonical`,
`sitemap`, `robots`, `llms_txt`, `github_homepage`, `github_topics`,
`package_repository`, `package_homepage`, `dependency_manifest`,
`dependency_lockfile`, `container_manifest`, `runtime_manifest`, `env_example`,
`api_schema`, `framework_route`, `ci_workflow`, `toolchain_manifest`,
`docs_linkback`, `local_git_remote`, `local_git_commit`, `session_metadata`,
`session_jsonl`/`_json`, `agent_invocation_event`, `tool_call_event`,
`tool_result_event`, `skill_invocation_event`, `conversation_reference`,
`text_mention`, `derived_source_attribution`.

**Merge/conflict rules:** candidate ingestion is **idempotent**; evidence is
required for every non-manual edge (or explicit authority record); **conflicting
evidence is preserved — the system does not silently pick a winner**;
user-pinned mappings win for routing but the graph retains conflicting
non-user evidence; official package/repo metadata outranks community/derived;
derived sources should not become official unless official evidence exists;
low-confidence text mentions should not create authoritative edges.

## `GraphCandidate`

Required fields: `kind` (node or edge candidate kind), `candidate_id`,
`evidence` (source doc/chunk/range evidence), `confidence` (0.0–1.0),
`merge_key` (optional, stable graph merge key), `metadata` (optional,
redacted). Candidates must reference source ranges (not just whole documents)
when the parser can identify exact provenance, and must include source id,
job id, item key, item canonical URI, parser/adapter name+version, node/edge
kind, confidence, evidence value+source, observed timestamp.

## Pipeline integration

Graph writes happen in the `graphing` stage, **after** `publishing`:
`axon-services::source::graph::write_baseline_graph` reads the already-committed
manifest and upserts container/document/containment skeleton from
`counts.graph_candidates`. Candidate ingestion validates against the closed
kind enums before merge.

Graph reads are exposed through `axon graph`, MCP `action=graph`, and the REST
`/v1/graph/*` routes. The shared graph service resolves kind inventory, nodes,
edges, queries, source subgraphs, and evidence-backed relationships through the
same `GraphStore` boundary. Memory retains its own memory-specific relationship
model in addition to source-graph integration.

## Ownership

Graph writes happen through graph services and source-pipeline stages, not
through transport-specific side effects. Module map: `store.rs` (GraphStore
trait + `query()`), `sqlite.rs` (`SqliteGraphStore` — only concrete impl),
`candidate.rs` (idempotent ingest), `merge.rs` (`GraphMergePolicy`),
`authority.rs` (`AuthorityDecision`), `schema_registry.rs` (kind registries
consumed by schema generation).

If the graph vocabulary changes, update this file,
`crates/axon-graph/src/schema_registry.rs`, and regenerate `graph.schema.json`
in the same PR.
