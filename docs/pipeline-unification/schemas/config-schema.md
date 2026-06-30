# Config Schema Contract
Last Modified: 2026-06-30

## Contract

Configuration schemas define the desired `.env` and `config.toml` shape. The
runtime config loader is the source for `config.toml`; bootstrap/env metadata is
the source for `.env`.

## Generated Artifacts

```text
docs/reference/config/config.schema.json
docs/reference/config/env.schema.json
docs/reference/config/config-toml.md
docs/reference/config/env.md
```

Generator:

```bash
cargo xtask schemas config
cargo xtask schemas config --check
```

## Config TOML Schema

Required metadata per setting:

- TOML path
- type
- default
- min/max or enum values
- env override, if any
- restart requirement
- secret status
- owning crate
- description
- removed/replacement key metadata
- example value
- validation fixture path

## Root Artifact Shape

`docs/reference/config/config.schema.json`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://axon.local/schemas/config/config.schema.json",
  "title": "AxonConfigToml",
  "x-axon": {
    "contract_version": "2026-06-30",
    "generated_by": "cargo xtask schemas config",
    "owner_crates": ["axon-core"],
    "source_inputs": ["crates/axon-core/src/config"]
  },
  "type": "object",
  "required": ["server", "sources", "pipeline", "jobs", "providers"],
  "properties": {},
  "additionalProperties": false
}
```

`docs/reference/config/env.schema.json`:

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://axon.local/schemas/config/env.schema.json",
  "title": "AxonEnv",
  "x-axon": {
    "contract_version": "2026-06-30",
    "generated_by": "cargo xtask schemas config",
    "owner_crates": ["axon-core"],
    "source_inputs": ["crates/axon-core/src/config/env.rs", ".env.example"]
  },
  "type": "object",
  "properties": {},
  "additionalProperties": false
}
```

## Config Setting Shape

```json
{
  "path": "providers.embedding.batch_size",
  "type": "integer",
  "default": 64,
  "minimum": 1,
  "maximum": 512,
  "env_override": "AXON_EMBEDDING_BATCH_SIZE",
  "restart_required": false,
  "secret": false,
  "owner_crate": "axon-embedding",
  "description": "Maximum chunks per embedding request.",
  "example": 128,
  "removed": false,
  "replacement": null
}
```

Required top-level config sections:

- `server`
- `sources`
- `pipeline`
- `watch`
- `jobs`
- `providers`
- `retrieval`
- `ask`
- `crawl`
- `memory`
- `graph`
- `artifacts`
- `prune`
- `observability`
- `security`

## Required Config Keys

The target `config.toml` schema must include these keys at minimum:

| Key | Type | Default | Owner |
|---|---|---|---|
| `server.default_collection` | string | `axon` | `axon-web` |
| `server.json_pretty` | bool | `false` | `axon-web` |
| `pipeline.max_active_source_jobs` | integer | `4` | `axon-services` |
| `pipeline.max_active_interactive_jobs` | integer | `8` | `axon-services` |
| `jobs.heartbeat_secs` | integer | `15` | `axon-jobs` |
| `jobs.provider_reservation_timeout_secs` | integer | `30` | `axon-jobs` |
| `sources.embed_by_default` | bool | `true` | `axon-services` |
| `sources.default_scope_web` | enum | `site` | `axon-services` |
| `sources.default_scope_local` | enum | `directory` | `axon-services` |
| `watch.tick_secs` | integer | `15` | `axon-jobs` |
| `watch.lease_secs` | integer | `300` | `axon-jobs` |
| `providers.embedding.batch_size` | integer | `128` | `axon-embedding` |
| `providers.embedding.max_concurrent_requests` | integer | `4` | `axon-embedding` |
| `providers.embedding.interactive_reserved_requests` | integer | `1` | `axon-jobs` |
| `providers.vector.write_concurrency` | integer | `4` | `axon-vectors` |
| `providers.vector.read_concurrency` | integer | `16` | `axon-vectors` |
| `providers.llm.completion_concurrency` | integer | `4` | `axon-llm` |
| `providers.search.default` | enum | `searxng-then-tavily` | `axon-adapters` |
| `retrieval.limit` | integer | `10` | `axon-retrieval` |
| `retrieval.hybrid_candidates` | integer | `150` | `axon-retrieval` |
| `crawl.max_pages_default` | integer | `2000` | `axon-adapters` |
| `crawl.respect_robots` | bool | `false` | `axon-adapters` |
| `memory.decay_enabled` | bool | `true` | `axon-memory` |
| `memory.review_interval_days` | integer | `30` | `axon-memory` |
| `graph.enabled` | bool | `true` | `axon-graph` |
| `prune.retention_days.jobs` | integer | `14` | `axon-prune` |
| `observability.log_level` | enum | `info` | `axon-observe` |
| `security.allow_private_networks` | bool | `false` | `axon-authz` |

This table is intentionally compact. Power-user knobs belong here only when
they materially affect throughput, safety, cost, freshness, or retrieval quality.

## Env Schema

Required metadata per variable:

- env var name
- required/optional
- secret/non-secret
- default when applicable
- owning crate
- compose usage
- validation rule
- replacement when removed
- whether it may appear in `.env.example`

## Env Variable Shape

```json
{
  "name": "QDRANT_URL",
  "required": true,
  "secret": false,
  "default": "http://127.0.0.1:6333",
  "owner_crate": "axon-vectors",
  "compose_usage": true,
  "validation": "url",
  "example_allowed": true,
  "removed": false,
  "replacement": null
}
```

Only URLs, secrets, runtime/bootstrap values, and compose interpolation values
belong in `.env`. Tuning belongs in `config.toml`.

## Required Env Variables

The target `.env` schema includes these keys at minimum:

| Name | Required | Secret | Owner | Notes |
|---|---:|---:|---|---|
| `AXON_DATA_DIR` | no | no | `axon-core` | bootstrap path override |
| `QDRANT_URL` | yes | no | `axon-vectors` | vector store URL |
| `TEI_URL` | yes | no | `axon-embedding` | embedding provider URL |
| `AXON_CHROME_REMOTE_URL` | no | no | `axon-adapters` | render provider URL |
| `AXON_HTTP_HOST` | no | no | `axon-web` | unified server bind host |
| `AXON_HTTP_PORT` | no | no | `axon-web` | unified server bind port |
| `AXON_PUBLIC_URL` | no | no | `axon-web` | OAuth/public URL |
| `AXON_HTTP_TOKEN` | no | yes | `axon-web` | static bearer auth |
| `AXON_AUTH_MODE` | no | no | `axon-authz` | `none`, `bearer`, or `oauth` |
| `AXON_GOOGLE_CLIENT_ID` | no | no | `axon-authz` | OAuth |
| `AXON_GOOGLE_CLIENT_SECRET` | no | yes | `axon-authz` | OAuth |
| `GITHUB_TOKEN` | no | yes | `axon-adapters` | private/rate-limited git |
| `GITLAB_TOKEN` | no | yes | `axon-adapters` | private/rate-limited git |
| `GITEA_TOKEN` | no | yes | `axon-adapters` | private/rate-limited git |
| `REDDIT_CLIENT_ID` | no | no | `axon-adapters` | reddit source |
| `REDDIT_CLIENT_SECRET` | no | yes | `axon-adapters` | reddit source |
| `TAVILY_API_KEY` | no | yes | `axon-adapters` | search fallback |
| `AXON_SEARXNG_URL` | no | no | `axon-adapters` | search provider URL |
| `AXON_OPENAI_API_KEY` | no | yes | `axon-llm` | openai-compatible |
| `AXON_OPENAI_BASE_URL` | no | no | `axon-llm` | openai-compatible |
| `AXON_CODEX_HOME` | no | no | `axon-llm` | codex provider |

Env schema must reject non-secret tuning keys that belong in `config.toml`
unless they are explicitly marked as env overrides for container deployment.

## Removed Key Contract

Clean-break config still needs clear failure for stale local files.

Removed keys are represented in a generated removed-key registry, not accepted
by the active schema:

```json
{
  "key": "AXON_MCP_HTTP_TOKEN",
  "kind": "env",
  "replacement": "AXON_HTTP_TOKEN",
  "message": "Unified server auth token key changed.",
  "fail_startup": true
}
```

Rules:

- removed keys are absent from valid schemas
- `axon doctor --config` reports removed keys with replacements
- normal startup fails on removed keys that could change auth/provider behavior
- setup rewrite may transform removed keys only with explicit user confirmation

## Generated Example Files

The generator writes:

```text
.env.example
config.example.toml
docs/reference/config/env.md
docs/reference/config/config-toml.md
```

Examples are sorted by section, include comments, and never include real
secrets.

## Drift Checks

Fail when:

- config struct field has no schema metadata
- config contract lists key absent from loader
- env example contains unknown key
- secret appears in `config.toml`
- tuning-only key appears only in `.env`
- defaults differ between code and docs
- removed key registry differs from doctor/setup rewrite behavior
- `.env.example` includes tuning-only keys
- `config.example.toml` includes secrets or deployment URLs

## Validation Fixtures

Required fixtures:

```text
crates/axon-core/tests/fixtures/config/minimal.valid.toml
crates/axon-core/tests/fixtures/config/full.valid.toml
crates/axon-core/tests/fixtures/config/secret-in-toml.invalid.toml
crates/axon-core/tests/fixtures/config/unknown-key.invalid.toml
crates/axon-core/tests/fixtures/env/minimal.valid.env
crates/axon-core/tests/fixtures/env/full.valid.env
crates/axon-core/tests/fixtures/env/tuning-only.invalid.env
crates/axon-core/tests/fixtures/env/removed-key.invalid.env
crates/axon-core/tests/fixtures/config/url-in-toml.invalid.toml
```

## Acceptance Criteria

- generated `config.example.toml` validates against config schema
- generated `.env.example` validates against env schema
- all code defaults match generated docs
- every env override is linked to exactly one config key or bootstrap secret
- no secret-bearing key is allowed in `config.toml`
- no tuning-only key is required in `.env`
- removed keys fail with actionable replacement guidance
- generated examples validate against the generated schemas
