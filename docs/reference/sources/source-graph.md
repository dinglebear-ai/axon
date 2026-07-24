# Source Graph

Last Modified: 2026-07-19

The source graph records relationships between sources, documents, entities,
and extracted facts. Edges are never "just true" — they are evidence-backed
claims with authority and confidence.

> Authoritative schema: [`graph.schema.json`](graph.schema.json). Contract
> source: [`docs/pipeline-unification/sources/source-graph.md`](../../pipeline-unification/sources/source-graph.md).
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

## Node kinds (selection)

`source`, `web_origin`, `docs_site`, `web_page`, `repo`, `repo_branch`,
`repo_commit`, `repo_file`, `local_checkout`, `package`, `package_version`,
`registry_namespace`, `container_image`, `container_image_tag`,
`github_action`, `toolchain`, `system_package`, `terraform_provider`,
`helm_chart`, `runtime_service`, `network_endpoint`, `volume_mount`,
`environment_variable`, `secret_reference`, `api_surface`, `api_operation`,
`schema_type`, `schema_field`, `protocol`, `model`, `reddit_subreddit`,
`reddit_thread`, `youtube_video`/`playlist`/`channel`, `feed`, `feed_entry`,
`session`, `session_turn`, `agent`, `agent_invocation`, `tool`, `tool_call`,
`external_resource`, `skill`, `skill_invocation`, `memory`, `decision`,
`issue`, `pull_request`, `person_or_org`, `derived_source`, `artifact`.

Naming rule: no schema may use `site`/`repository`/`file`/`api_endpoint` when
the registry names are `web_origin`/`repo`/`repo_file`/`api_operation`.

## Edge kinds (selection)

`alias_of`, `canonicalizes_to`, `official_for`, `derived_from`, `mirrors`,
`package_has_repo`/`_docs`/`_version`/`_owned_by`, `repo_declares_dependency`,
`repo_locks_dependency_version`, `repo_uses_container_image`,
`repo_uses_github_action`, `repo_uses_toolchain`, `repo_declares_service`,
`service_uses_image`, `service_exposes_endpoint`, `service_mounts_volume`,
`service_requires_env`, `repo_declares_api`, `api_uses_protocol`,
`api_has_operation`, `operation_uses_schema`, `schema_has_field`,
`branch_points_to_commit`, `commit_contains_file`, `local_checkout_tracks_repo`,
`docs_site_contains_page`, `feed_contains_entry`, `session_has_turn`,
`session_mentions_repo`/`_source`/`_issue`/`_pr`/`_package`,
`session_produced_decision`, `agent_invocation_used_skill`/`_tool`,
`tool_call_produced_artifact`/`_touched_file`/`_read_resource`/`_mutated_resource`,
`memory_relates_to`/`_supersedes`/`_contradicts`/`_compacts`/`_about_source`,
`source_produced_artifact`, `source_indexed_as`.

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

> **Not yet wired:** no graph REST/MCP surface or graph-aware ask/retrieval
> reads from `GraphStore` at runtime today — the graph is written during
> indexing but not yet read by any transport. Memory has its own smaller
> SQLite graph-like model independent of `GraphStore`.

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
