# Repository Structure Contract
Last Modified: 2026-06-30

## Contract

This is the desired end-state repository shape after pipeline unification. It is
the structural companion to `crate-structure.md`: crate structure defines Rust
ownership; this file defines the whole repo layout.

No legacy directories remain solely for compatibility. The root crate is a thin
binary/bootstrap crate.

## Top-Level Tree

```text
axon/
  Cargo.toml
  Cargo.lock
  README.md
  CLAUDE.md
  AGENTS.md -> CLAUDE.md
  GEMINI.md -> CLAUDE.md
  config.example.toml
  .env.example
  justfile
  docker-compose.yaml
  docker-compose.prod.yaml
  Dockerfile
  build.rs
  src/
    lib.rs
    main.rs
  crates/
    axon-error/
    axon-api/
    axon-core/
    axon-authz/
    axon-observe/
    axon-route/
    axon-adapters/
    axon-ledger/
    axon-parse/
    axon-graph/
    axon-memory/
    axon-document/
    axon-embedding/
    axon-vectors/
    axon-retrieval/
    axon-llm/
    axon-prune/
    axon-jobs/
    axon-services/
    axon-mcp/
    axon-web/
    axon-cli/
  docs/
    architecture/
    reference/
    guides/
    development/
    pipeline-unification/
  config/
    chrome/
    qdrant/
    tei/
  scripts/
    axon
    dev/
    ops/
  xtask/
    Cargo.toml
    src/
  tests/
    contract/
    integration/
    fixtures/
  fixtures/
    sources/
    parsers/
    sessions/
    providers/
  apps/
    web/
    android/
    chrome-extension/
    palette-tauri/
    desktop/
  examples/
    config/
    sources/
    providers/
```

## Root Crate Tree

```text
src/
  lib.rs
  main.rs
```

Root crate rules:

- `main.rs` loads bootstrap env and calls `axon_cli::run`
- `lib.rs` re-exports only the binary entrypoint required by integration tests
- root `build.rs`, when present, only embeds static assets or generated version
  metadata for the thin binary
- no domain logic lives under root `src/`
- no transport handlers live under root `src/`

## Standard Crate Tree

Every pipeline crate follows this baseline:

```text
crates/axon-name/
  Cargo.toml
  src/
    CLAUDE.md
    lib.rs
    error.rs          # only when crate-specific wrappers are needed
    testing.rs        # fake/test helpers behind test or feature gate
  tests/
    contract.rs
    integration.rs    # when crate has I/O or store/provider behavior
  fixtures/           # when crate needs golden fixtures
```

Rules:

- no `mod.rs`
- public modules are explicit in `lib.rs`
- fixtures are small, deterministic, and source-specific
- generated files live under `generated/` or transport-owned directories
- `testing.rs` does not leak into production API unless intentionally gated

## Crate-Specific Trees

The detailed per-crate contracts live in [../crates/](../crates/README.md).
This section mirrors their source-file layout so repository structure checks can
compare the target tree against one place.

### `axon-error`

```text
crates/axon-error/src/
  lib.rs
  api_error.rs
  code.rs
  stage.rs
  severity.rs
  retry.rs
  degradation.rs
  cooling.rs
  context.rs
  conversion.rs
  testing.rs
```

### `axon-api`

**Reflects the crate's actual shipped `pub mod` surface (synced 2026-07-12
from `crates/axon-api/src/lib.rs`), not this contract's minimal target list —
`axon-api` is one of five pre-existing crates that keep their
current-behavior module surface until the #298 cutover. See
`docs/pipeline-unification/foundation/crate-structure.md`'s `axon-api`
section and `xtask/src/checks/crate_contracts_spec.rs`.**

```text
crates/axon-api/src/
  lib.rs
  contract.rs
  diff.rs
  explain.rs
  ingest.rs
  job_dto.rs
  job_progress.rs
  job_status.rs
  mcp_schema.rs
  migration.rs
  purge.rs
  reset.rs
  result.rs
  schema_registry.rs
  service_job.rs
  source.rs
```

### `axon-core`

```text
crates/axon-core/src/
  lib.rs
  config.rs
  paths.rs
  ids.rs
  time.rs
  redact.rs
  http_safety.rs
  artifact.rs
  fs.rs
  diagnostics.rs
  testing.rs
```

### `axon-authz`

```text
crates/axon-authz/src/
  lib.rs
  caller.rs
  scope.rs
  policy.rs
  decision.rs
  visibility.rs
  affinity.rs
  testing.rs
```

### `axon-observe`

```text
crates/axon-observe/src/
  lib.rs
  event.rs
  phase.rs
  heartbeat.rs
  progress.rs
  metric.rs
  span.rs
  log.rs
  collector.rs
  testing.rs
```

### `axon-route`

```text
crates/axon-route/src/
  lib.rs
  resolver.rs
  router.rs
  canonical.rs
  source_id.rs
  scope.rs
  authority.rs
  alias.rs
  capability.rs
  testing.rs
```

### `axon-adapters`

```text
crates/axon-adapters/src/
  lib.rs
  adapter.rs
  registry.rs
  capability.rs
  acquisition.rs
  manifest.rs
  web.rs
  local.rs
  git.rs
  registry_sources.rs
  feed.rs
  youtube.rs
  reddit.rs
  sessions.rs
  cli_tool.rs
  mcp_tool.rs
  testing.rs
```

### `axon-ledger`

```text
crates/axon-ledger/src/
  lib.rs
  store.rs
  sqlite.rs
  migration.rs
  source.rs
  item.rs
  manifest.rs
  diff.rs
  generation.rs
  document_status.rs
  lease.rs
  cleanup_debt.rs
  transaction.rs
  testing.rs
```

### `axon-parse`

```text
crates/axon-parse/src/
  lib.rs
  parser.rs
  registry.rs
  facts.rs
  graph_candidate.rs
  code.rs
  manifest.rs
  schema.rs
  session.rs
  tool.rs
  env.rs
  docker.rs
  config.rs
  testing.rs
```

### `axon-graph`

```text
crates/axon-graph/src/
  lib.rs
  store.rs
  sqlite.rs
  migration.rs
  node.rs
  edge.rs
  evidence.rs
  candidate.rs
  authority.rs
  merge.rs
  query.rs
  testing.rs
```

### `axon-memory`

```text
crates/axon-memory/src/
  lib.rs
  store.rs
  sqlite.rs
  migration.rs
  record.rs
  link.rs
  decay.rs
  review.rs
  recall.rs
  context.rs
  graph.rs
  testing.rs
```

### Remaining Crates

```text
crates/axon-document/src/{lib.rs,preparer.rs,chunk_router.rs,profile.rs,prepared.rs,chunk.rs,metadata.rs,code.rs,markdown.rs,transcript.rs,session.rs,schema.rs,text.rs,testing.rs}
crates/axon-embedding/src/{lib.rs,provider.rs,batch.rs,capability.rs,reservation.rs,tei.rs,openai_compat.rs,fake.rs,testing.rs}
crates/axon-vectors/src/{lib.rs,store.rs,qdrant.rs,collection.rs,point.rs,payload.rs,filter.rs,query.rs,health.rs,testing.rs}
crates/axon-retrieval/src/{lib.rs,engine.rs,plan.rs,query.rs,filter.rs,rank.rs,context.rs,citation.rs,memory.rs,graph.rs,testing.rs}
crates/axon-llm/src/{lib.rs,provider.rs,capability.rs,completion.rs,stream.rs,prompt.rs,openai_compat.rs,codex.rs,gemini.rs,fake.rs,testing.rs}
crates/axon-prune/src/{lib.rs,plan.rs,executor.rs,debt.rs,generation.rs,orphan.rs,dedupe.rs,receipt.rs,safety.rs,testing.rs}
crates/axon-jobs/src/{lib.rs,store.rs,sqlite.rs,migration.rs,runtime.rs,job.rs,attempt.rs,event.rs,heartbeat.rs,scheduler.rs,watch.rs,worker.rs,reservation.rs,recovery.rs,testing.rs}
```

The four transport-adjacent crates below (`axon-services`, `axon-mcp`,
`axon-web`, `axon-cli`) are, along with `axon-api` above, the five
pre-existing crates that keep their current-behavior module surface until
the #298 cutover — these lines reflect their actual shipped `pub mod`
surface (synced 2026-07-12 from each crate's `src/lib.rs`), not this
contract's minimal target list. See
`docs/pipeline-unification/foundation/crate-structure.md`'s per-crate
sections and `xtask/src/checks/crate_contracts_spec.rs`. `pub(crate) mod`
internals in `axon-services` (`web_source`, `git_source`, `feed_source`,
`reddit_source`, `registry_source`, `sessions_source`, `local_source`,
`contract_write`) and the private `cors` module in `axon-mcp` are
intentionally excluded — they aren't public API.

```text
crates/axon-services/src/{lib.rs,action_api.rs,artifacts.rs,brand.rs,client_contract.rs,code_search_watch.rs,config.rs,config_snapshot_hash.rs,context.rs,crawl.rs,crawl_sync.rs,debug.rs,diff.rs,document.rs,embed.rs,endpoints.rs,events.rs,extract.rs,feed_acquire.rs,feed_target.rs,freshness.rs,git_acquire.rs,graph.rs,ingest.rs,jobs.rs,map.rs,memory.rs,migrate.rs,mobile_sessions.rs,prune.rs,query.rs,reddit_acquire.rs,reddit_target.rs,refresh.rs,registry_acquire.rs,reset.rs,runtime.rs,scrape.rs,screenshot.rs,search.rs,search_crawl.rs,service_traits.rs,sessions.rs,sessions_legacy.rs,sessions_target.rs,setup.rs,source.rs,source_jobs.rs,summarize.rs,sync.rs,system.rs,transport.rs,types.rs,watch.rs,youtube_acquire.rs,youtube_target.rs}
crates/axon-mcp/src/{lib.rs,auth.rs,schema.rs,schema_registry.rs,server.rs}
crates/axon-web/src/{lib.rs,auth.rs,health.rs,metrics.rs,panel_first_run.rs,panel_stack.rs,schema_registry.rs,security.rs,server.rs,static_assets.rs}
crates/axon-cli/src/{lib.rs,commands.rs,json.rs,schema_registry.rs,ui.rs}
```

## Generated and Fixture Directories

Generated outputs:

```text
target/generated-docs/      # transient generated docs before check/copy
crates/axon-web/src/generated/
crates/axon-mcp/src/generated/
crates/axon-api/src/generated/
```

Fixtures:

```text
fixtures/sources/web/
fixtures/sources/git/
fixtures/sources/local/
fixtures/parsers/rust/
fixtures/parsers/python/
fixtures/parsers/node/
fixtures/parsers/docker/
fixtures/parsers/sessions/
fixtures/providers/tei/
fixtures/providers/qdrant/
```

## Validation

Required checks:

```bash
cargo xtask check-repo-structure
cargo xtask check-layering
```

Planned generated-doc freshness check:

```bash
cargo xtask docs generate --check
```

This command is part of the desired end-state documentation toolchain. Until it
exists, `cargo xtask check-repo-structure` and `cargo xtask check-layering` are
the enforced repository-shape checks.

Checks must prove:

- every workspace member exists
- every non-trivial crate has `src/CLAUDE.md`
- no crate uses `mod.rs`
- no removed crate remains in `Cargo.toml`
- generated docs are fresh
- repo tree matches this contract or this contract has been intentionally
  updated
