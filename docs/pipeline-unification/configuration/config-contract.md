# Config TOML Contract
Last Modified: 2026-06-30

## Contract

This is the target clean-break `config.toml` shape. Current implementation uses
the existing `config.toml.example` shape and accepts sections such as `build`,
`services`, `llm`, `search`, `tei`, `workers`, `watch`, `scrape`, and related
runtime groups with unknown keys denied.

`config.toml` is the stable tuning and behavior contract. It contains sensible
defaults plus the knobs a real user will actually tune: pipeline concurrency,
provider limits, retrieval behavior, crawl/fetch behavior, watch cadence,
retention, security policy, and observability.

It is not a secret store, not a Docker Compose interpolation file, and not a
200-line dump of every internal constant.

## Design Rules

- Defaults are good enough for first boot.
- Common tuning is visible and documented.
- Rare/expert tuning is grouped and can stay commented out.
- Secrets and deployment URLs stay in `.env`.
- Unknown keys fail with a clear error.
- Environment overrides are allowed only for bootstrap, CI, and temporary
  smokes; TOML is the normal home for non-secret tuning.
- Config is captured in job `config_snapshot_id` for reproducibility.
- Config shape aligns with provider/job contracts.

## Target Top-Level Sections

```toml
[server]
[sources]
[pipeline]
[jobs]
[providers.embedding]
[providers.vector]
[providers.llm]
[providers.search]
[providers.fetch]
[providers.render]
[retrieval]
[ask]
[crawl]
[watch]
[memory]
[graph]
[artifacts]
[prune]
[observability]
[security]
```

This is the full desired shape. The generated example should include concise
defaults and comments, not every advanced key expanded by default.

## Server

Non-secret behavior:

| Key | Default | Meaning |
|---|---|---|
| `default_collection` | `"axon"` | Default vector collection. |
| `json_pretty` | `false` | Pretty JSON for CLI/API when requested. |
| `request_timeout_secs` | `300` | Default service request timeout. |

URLs, bind addresses, public URLs, and auth secrets stay in `.env`.

**Resolved (G1-02, 2026-07-09 audit):** the rename from `[search].collection`
(current landed key, `crates/axon-core/src/config/`) to `[server].default_collection`
(this target shape) is confirmed intentional — `default_collection` is a
server-wide default, not search-specific, so it belongs under `[server]` in
the target layout. This is not yet implemented in code; Workstream J
(Configuration) owns moving the key from `[search]` to `[server]` and adding
a deprecated-key read compatibility shim for `[search].collection`, per the
`env-contract.md`/`config-contract.md` deprecation pattern used elsewhere.

## Sources

| Key | Default | Meaning |
|---|---|---|
| `embed_by_default` | `true` | Source jobs write vectors unless `--no-embed`. |
| `default_scope_web` | `"site"` | Default web scope. |
| `default_scope_local` | `"directory"` | Default local path scope. |
| `authority_map_enabled` | `true` | Use known official source mappings. |
| `max_source_items` | `10000` | Safety cap before explicit override. |
| `source_id_strategy` | `"canonical-uri"` | Stable id strategy. |

## Pipeline

| Key | Default | Meaning |
|---|---|---|
| `max_active_source_jobs` | `4` | Concurrent source jobs. |
| `max_active_interactive_jobs` | `8` | Concurrent ask/query/retrieve jobs. |
| `batch_yield_ms` | `10` | Yield between large batches. |
| `item_failure_policy` | `"degrade"` | `degrade` or `fail`. |
| `publish_requires_cleanup` | `false` | Whether cleanup debt blocks success. |
| `max_document_bytes` | `4194304` | Default per-document content cap. |

## Jobs

| Key | Default | Meaning |
|---|---|---|
| `heartbeat_secs` | `15` | Active job heartbeat interval. |
| `stale_after_secs` | `300` | Stale candidate threshold. |
| `stale_grace_secs` | `60` | Recovery grace after stale threshold. |
| `event_retention_days` | `14` | Job event retention. |
| `failed_event_retention_days` | `60` | Failed job event retention. |
| `terminal_retention_days` | `30` | Terminal job row retention. |
| `max_events_per_job` | `50000` | Safety cap. |
| `default_priority` | `"normal"` | Default priority. |

## Providers: Embedding

This section is critical for avoiding bottlenecks.

```toml
[providers.embedding]
batch_size = 128
max_concurrent_requests = 4
max_in_flight_inputs = 320
request_timeout_ms = 30000
max_retries = 5
retry_backoff_ms = 500
cooldown_after_failures = 3
cooldown_secs = 30
interactive_reserved_requests = 1
background_max_concurrent_requests = 3
maintenance_max_concurrent_requests = 1
query_instruction_enabled = true
```

Rules:

- `interactive_reserved_requests` protects `ask`/`query` query embeddings.
- background source jobs cannot consume the reserved interactive lane.
- `max_in_flight_inputs` is global across jobs.
- batch size is a provider hint, not a license to exceed scheduler capacity.

## Providers: Vector

| Key | Default | Meaning |
|---|---|---|
| `write_concurrency` | `4` | Concurrent vector writes. |
| `read_concurrency` | `16` | Concurrent vector reads. |
| `delete_concurrency` | `2` | Cleanup/prune delete concurrency. |
| `upsert_batch_points` | `256` | Points per upsert. |
| `delete_batch_points` | `512` | Points per delete selector batch. |
| `hybrid_enabled` | `true` | Dense + sparse RRF when collection supports it. |
| `hnsw_ef` | `128` | Named-mode search ef. |
| `hnsw_ef_legacy` | `64` | Legacy unnamed ef. |

## Providers: LLM

| Key | Default | Meaning |
|---|---|---|
| `backend` | `"gemini-headless"` | Default backend; may be env when runtime-specific. |
| `completion_concurrency` | `4` | Global LLM completions. |
| `completion_timeout_secs` | `300` | Per-call timeout. |
| `synthesis_model` | `""` | Backend default when empty. |
| `chat_model` | `""` | Falls back to synthesis model. |
| `high_context` | `null` | Infer from model when unset. |
| `codex_pool_idle_ttl_secs` | `300` | Codex child pool TTL. |

Secrets and binary homes stay in `.env`.

## Providers: Search, Fetch, Render

```toml
[providers.search]
default = "searxng-then-tavily"
result_limit = 10
research_full_content = true

[providers.fetch]
concurrency = 32
request_timeout_ms = 30000
retries = 3
retry_backoff_ms = 500
user_agent = ""

[providers.render]
enabled = true
max_concurrent_pages = 4
page_timeout_ms = 45000
```

Endpoint URLs and API keys stay in `.env`.

## Retrieval

| Key | Default | Meaning |
|---|---|---|
| `limit` | `10` | Default query result count. |
| `hybrid_candidates` | `100` | RRF prefetch per arm. |
| `ask_hybrid_candidates` | `150` | Wider ask retrieval. |
| `min_score` | `null` | Optional global score floor. |
| `exclude_local_code_by_default` | `true` | Keep code-search separate unless requested. |

## Ask

| Key | Default | Meaning |
|---|---|---|
| `max_context_chars` | `300000` | LLM context budget. |
| `chunk_limit` | `24` | Returned chunks before adaptive overrides. |
| `candidate_limit` | `250` | Candidate fetch count. |
| `full_docs` | `6` | Max full docs included. |
| `backfill_chunks` | `5` | Backfill chunks from top docs. |
| `doc_fetch_concurrency` | `4` | Full-doc fetch concurrency. |
| `min_citations_nontrivial` | `2` | Citation floor. |
| `authoritative_domains` | `[]` | Rerank boost domains. |
| `authoritative_boost` | `0.0` | Boost amount. |

## Crawl

| Key | Default | Meaning |
|---|---|---|
| `max_pages` | `2000` | Default site cap. |
| `max_depth` | `10` | Default depth. |
| `respect_robots` | `false` | Preserve current default unless changed intentionally. |
| `discover_sitemaps` | `true` | Sitemap backfill. |
| `min_markdown_chars` | `200` | Thin page threshold. |
| `drop_thin_markdown` | `true` | Skip thin pages. |
| `memory_abort_percent` | `85` | Crawl RSS guard. |

## Watch

| Key | Default | Meaning |
|---|---|---|
| `tick_secs` | `15` | Scheduler tick. |
| `lease_secs` | `300` | Watch lease TTL. |
| `max_due_per_tick` | `10` | Due watch cap. |
| `max_concurrent_runs` | `2` | Concurrent watch runs. |
| `coalesce_source_refreshes` | `true` | Avoid duplicate refresh jobs. |

## Memory

| Key | Default | Meaning |
|---|---|---|
| `collection` | `"axon_memory"` | Memory vector collection. |
| `decay_enabled` | `true` | Enable memory decay scoring. |
| `review_interval_days` | `30` | Review cadence. |
| `pin_boost` | `0.3` | Recall boost for pinned memory. |
| `forget_deletes_vectors` | `true` | Forget removes vector points. |

## Graph

| Key | Default | Meaning |
|---|---|---|
| `enabled` | `true` | Enable graph candidate ingestion. |
| `candidate_confidence_floor` | `0.5` | Store candidate floor. |
| `auto_merge_confidence` | `0.9` | Merge threshold. |
| `evidence_retention_days` | `0` | `0` means while edge exists. |

## Artifacts

| Key | Default | Meaning |
|---|---|---|
| `retention_days` | `30` | Default transient artifact retention. |
| `max_inline_bytes` | `65536` | Inline response cap. |
| `max_artifact_bytes` | `1073741824` | Safety cap. |
| `write_warc_by_default` | `false` | WARC is opt-in. |

## Prune

| Key | Default | Meaning |
|---|---|---|
| `dry_run_default` | `true` | Prune previews first. |
| `require_confirm_for_destructive` | `true` | Human confirmation unless `--yes`. |
| `delete_batch_size` | `512` | Vector delete batch. |
| `cleanup_retry_secs` | `300` | Debt retry interval. |

## Observability

| Key | Default | Meaning |
|---|---|---|
| `log_level` | `"info"` | Default Axon log level. |
| `structured_logs` | `true` | JSON/structured logs where supported. |
| `progress_event_throttle_ms` | `500` | Human output throttle. |
| `metrics_enabled` | `true` | Prometheus metrics when server runs. |
| `redact_logs` | `true` | Redact sensitive fields. |

## Security

| Key | Default | Meaning |
|---|---|---|
| `allow_private_network_fetch` | `false` | SSRF private IP allowance. |
| `allow_local_paths` | `trusted-cli-only` | Local path policy. |
| `allow_tool_execution` | `false` | CLI/MCP tool source execution. |
| `max_tool_output_bytes` | `1048576` | Tool output cap. |
| `redaction_fail_closed` | `true` | Block writes on redaction failure. |

## Target Minimal Example

```toml
[server]
default_collection = "axon"

[providers.embedding]
batch_size = 128
max_concurrent_requests = 4
max_in_flight_inputs = 320
interactive_reserved_requests = 1

[providers.llm]
backend = "gemini-headless"
completion_concurrency = 4

[retrieval]
hybrid_candidates = 100
ask_hybrid_candidates = 150

[jobs]
heartbeat_secs = 15
stale_after_secs = 300
```

## Completion Gate

The final `config.example.toml` is acceptable only if:

- it is concise with sensible defaults
- it has no secrets
- it owns non-secret tuning currently scattered through env
- embedding/vector/LLM concurrency are coordinated with the unified job model
- unknown keys fail clearly
- `axon doctor` can explain every important effective value and override source
