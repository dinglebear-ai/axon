# Axon Env Config Drift Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Axon's live `~/.axon/.env`, live `~/.axon/config.toml`, repo examples, docs, and parser support agree on every implemented env/TOML lever, with `.env` restricted to URLs, secrets, and runtime/bootstrap.

**Architecture:** Treat the Rust config parser and env registry as the executable source of truth, then generate or test examples/docs against that source. Move non-secret tuning into typed TOML sections, keep URL/secret/bootstrap/Compose interpolation in `.env`, and migrate the live files only after tests prove precedence and parsing.

**Tech Stack:** Rust workspace config parser (`axon-core`), TOML via serde `deny_unknown_fields`, dotenv-style `.env`, `./scripts/axon config list`, `./scripts/axon doctor`, documentation in Markdown.

## Global Constraints

- Only URLs, secrets, and necessary runtime/bootstrap belong in `~/.axon/.env` and `.env.example`.
- All non-secret tuning knobs belong in `~/.axon/config.toml` and `config.toml.example`.
- Preserve existing secret values; never print or commit them.
- Environment variables may remain as compatibility overrides, but they must not be normal documented placement for non-secret tuning.
- Keep `config.example.toml` working for existing docs/scripts, but make `config.toml.example` the requested canonical example name.
- Validate live changes with redacted `./scripts/axon config list` and `./scripts/axon doctor`.

---

## Current Drift Found on 2026-06-26

- `config.toml.example` is missing. The repo currently has `.env.example` and `config.example.toml`.
- Live `.env` contains TOML-capable ask tuning that overrides `config.toml`: `AXON_ASK_FULL_DOCS=1`, `AXON_ASK_BACKFILL_CHUNKS=1`, `AXON_ASK_DOC_FETCH_CONCURRENCY=1`, `AXON_ASK_DOC_CHUNK_LIMIT=24`, and `AXON_ASK_CANDIDATE_LIMIT=120`.
- Live `.env` contains `TEI_MAX_CLIENT_BATCH_SIZE=256`, overriding `tei.max-client-batch-size = 128` in `config.toml`.
- `AXON_SEARXNG_URL` is read by `config_literal.rs` and appears in `.env.example`, but is not registered in `env_registry`.
- Several implemented non-secret knobs are env-only or unregistered and therefore cannot satisfy the desired TOML-first policy yet:
  - Embed/chunking: `AXON_TEI_MAX_CONCURRENT`, `AXON_TEI_MAX_IN_FLIGHT_INPUTS`, `AXON_EMBED_POOL_MAX_INPUTS`, `AXON_EMBED_PREP_CONCURRENCY`, `AXON_EMBED_MAX_CHUNKS_PER_DOC`, `AXON_EMBED_MAX_SOURCE_CHUNKS_PER_DOC`, `AXON_EMBED_DEDUPE_EXACT_CHUNKS`, `AXON_MARKDOWN_CHUNK_MIN_CHARS`, `AXON_MARKDOWN_CHUNK_MAX_CHARS`, `AXON_CHUNK_OVERLAP_CHARS`, `AXON_OPENAI_EMBED_*`.
  - Qdrant creation/upsert: `AXON_QDRANT_UPSERT_BATCH_SIZE`, `AXON_QDRANT_UPSERT_PARALLELISM`, `AXON_QDRANT_BULK_LOAD`, `AXON_QDRANT_*INDEXING_THRESHOLD_KB`, `AXON_QDRANT_HNSW_*`, `AXON_QDRANT_PAYLOAD_INDEX_*`, `AXON_QDRANT_HNSW_ON_DISK`, `AXON_QDRANT_QUANTIZATION_ALWAYS_RAM`.
  - Code search: `AXON_CODE_SEARCH_ALLOWED_ROOTS`, `AXON_CODE_SEARCH_FRESHNESS_TTL_SECS`, `AXON_CODE_SEARCH_REINDEX_TIMEOUT_SECS`, `AXON_CODE_SEARCH_MAX_FILE_BYTES`, `AXON_CODE_SEARCH_CHANGED_FILE_BATCH_SIZE`.
  - Watch/session/source operational tuning: `AXON_WATCH_TICK_SECS`, `AXON_WATCH_LEASE_SECS`, `AXON_SESSION_INGEST_MAX_BYTES`, `AXON_REFRESH_FACET_LIMIT`, `AXON_SOURCES_FACET_LIMIT`, `AXON_DOMAINS_FACET_LIMIT`.
  - Endpoint/MCP limits: `AXON_ENDPOINT_*_CONCURRENCY`, `AXON_MCP_EMBED_MAX_LOCAL_BYTES`, `AXON_MCP_EMBED_MAX_LOCAL_DEPTH`, `AXON_MCP_EMBED_MAX_LOCAL_ENTRIES`, `AXON_TASK_RESULT_WAIT_TIMEOUT_SECS`.
  - Logging knobs: `AXON_LOG_MAX_BYTES`, `AXON_LOG_MAX_FILES`, `AXON_LOG_LEVEL`, `AXON_LOG_FULL_QUERIES`.
- Live `.env` also contains Compose/container tuning that should stay env-layer only because it configures external containers: `TEI_EMBEDDING_MODEL`, `TEI_HTTP_PORT`, `TEI_MAX_BATCH_REQUESTS`, `TEI_MAX_BATCH_TOKENS`, `TEI_MAX_CONCURRENT_REQUESTS`, `TEI_SERVER_MAX_CLIENT_BATCH_SIZE`, `TEI_TOKENIZATION_WORKERS`, `TEI_POOLING`, `NVIDIA_VISIBLE_DEVICES`, `CUDA_VISIBLE_DEVICES`, `NVIDIA_REQUIRE_CUDA`, `HF_HUB_ENABLE_HF_TRANSFER`, `TOKENIZERS_PARALLELISM`, and related container runtime variables.
- `./scripts/axon doctor` currently passes for SQLite, TEI, Qdrant, Chrome, Gemini headless, crawl, extract, embed, and ingest.

## File Structure

- Modify `crates/axon-core/src/config/parse/toml_config.rs`: add typed TOML sections/fields for non-secret tuning currently env-only.
- Modify `crates/axon-core/src/config/parse/tuning.rs`: resolve new TOML fields with env compatibility overrides.
- Modify `crates/axon-core/src/config/parse/build_config/config_literal.rs`: move remaining direct non-secret env reads behind TOML-aware helpers where appropriate.
- Modify `crates/axon-core/src/config/parse/env_registry/{runtime,advanced,migration}.rs`: register all implemented keys and classify them according to the boundary.
- Modify affected implementation modules that currently read env directly: `crates/axon-vector/src/ops/input.rs`, `crates/axon-vector/src/ops/input/code.rs`, `crates/axon-vector/src/ops/tei/tei_client.rs`, `crates/axon-vector/src/ops/tei/pipeline.rs`, `crates/axon-vector/src/ops/tei/qdrant_store.rs`, `crates/axon-code-index/src/config.rs`, `crates/axon-jobs/src/watch.rs`, `crates/axon-jobs/src/workers/watch_scheduler.rs`, `crates/axon-services/src/endpoints.rs`, `crates/axon-services/src/endpoints/{probe,verify}.rs`, and `crates/axon-mcp/src/server/tasks.rs`.
- Modify tests under `crates/axon-core/src/config/parse/` plus module-local tests for moved env reads.
- Modify `.env.example`: keep only URL/secret/auth/bootstrap/Compose interpolation keys.
- Create `config.toml.example`: canonical complete TOML example for all non-secret tuning keys.
- Modify `config.example.toml`: keep as compatibility copy or symlink target to `config.toml.example`.
- Modify `docs/guides/configuration.md`, `README.md`, and `CLAUDE.md`: update configuration reference and examples.
- Modify live `/home/jmagar/.axon/.env` and `/home/jmagar/.axon/config.toml`: preserve secrets, remove migrated env overrides, and add all current TOML knobs.

### Task 1: Add a Config Surface Audit Test

**Files:**
- Modify: `crates/axon-core/src/config/parse/env_registry_tests.rs`
- Modify: `crates/axon-core/src/config/parse/env_registry.rs`

**Interfaces:**
- Consumes: `env_registry::all_specs()`
- Produces: a test helper that fails when implementation-known env keys are missing from the registry.

- [ ] **Step 1: Write the failing registry completeness test**

Add a focused test with a curated list of currently implemented keys discovered in this audit:

```rust
#[test]
fn implemented_env_keys_are_registered() {
    let required = [
        "AXON_SEARXNG_URL",
        "AXON_TEI_MAX_CONCURRENT",
        "AXON_TEI_MAX_IN_FLIGHT_INPUTS",
        "AXON_EMBED_POOL_MAX_INPUTS",
        "AXON_EMBED_PREP_CONCURRENCY",
        "AXON_EMBED_MAX_CHUNKS_PER_DOC",
        "AXON_EMBED_MAX_SOURCE_CHUNKS_PER_DOC",
        "AXON_EMBED_DEDUPE_EXACT_CHUNKS",
        "AXON_MARKDOWN_CHUNK_MIN_CHARS",
        "AXON_MARKDOWN_CHUNK_MAX_CHARS",
        "AXON_CHUNK_OVERLAP_CHARS",
        "AXON_QDRANT_UPSERT_BATCH_SIZE",
        "AXON_QDRANT_UPSERT_PARALLELISM",
        "AXON_QDRANT_BULK_LOAD",
        "AXON_QDRANT_BULK_INDEXING_THRESHOLD_KB",
        "AXON_QDRANT_INDEXING_THRESHOLD_KB",
        "AXON_QDRANT_HNSW_M",
        "AXON_QDRANT_HNSW_EF_CONSTRUCT",
        "AXON_QDRANT_PAYLOAD_INDEX_PROFILE",
        "AXON_QDRANT_PAYLOAD_INDEX_PARALLELISM",
        "AXON_CODE_SEARCH_ALLOWED_ROOTS",
        "AXON_CODE_SEARCH_FRESHNESS_TTL_SECS",
        "AXON_CODE_SEARCH_REINDEX_TIMEOUT_SECS",
        "AXON_CODE_SEARCH_MAX_FILE_BYTES",
        "AXON_CODE_SEARCH_CHANGED_FILE_BATCH_SIZE",
        "AXON_WATCH_TICK_SECS",
        "AXON_WATCH_LEASE_SECS",
        "AXON_MCP_EMBED_MAX_LOCAL_BYTES",
        "AXON_MCP_EMBED_MAX_LOCAL_DEPTH",
        "AXON_MCP_EMBED_MAX_LOCAL_ENTRIES",
    ];

    let registered: std::collections::BTreeSet<_> =
        crate::config::parse::env_registry::all_specs()
            .map(|spec| spec.key)
            .collect();

    let missing: Vec<_> = required
        .iter()
        .copied()
        .filter(|key| !registered.contains(key))
        .collect();

    assert!(missing.is_empty(), "missing env_registry entries: {missing:?}");
}
```

- [ ] **Step 2: Run the test and confirm it fails**

Run:

```bash
cargo test -p axon-core env_registry
```

Expected: FAIL listing at least `AXON_SEARXNG_URL` and the unregistered embed/Qdrant/code-search keys.

- [ ] **Step 3: Register missing keys with correct placement**

Add entries to `runtime.rs`, `advanced.rs`, or `migration.rs`:

```rust
spec("AXON_SEARXNG_URL", KeepEnv, Both, None, Canonical, false),
spec("AXON_TEI_MAX_CONCURRENT", MoveToml, NotRuntime, Some("embed.tei-max-concurrent"), WarnEnvOverride, false),
spec("AXON_TEI_MAX_IN_FLIGHT_INPUTS", MoveToml, NotRuntime, Some("embed.tei-max-in-flight-inputs"), WarnEnvOverride, false),
spec("AXON_EMBED_POOL_MAX_INPUTS", MoveToml, NotRuntime, Some("embed.pool-max-inputs"), WarnEnvOverride, false),
spec("AXON_EMBED_PREP_CONCURRENCY", MoveToml, NotRuntime, Some("embed.prep-concurrency"), WarnEnvOverride, false),
spec("AXON_EMBED_MAX_CHUNKS_PER_DOC", MoveToml, NotRuntime, Some("embed.max-chunks-per-doc"), WarnEnvOverride, false),
spec("AXON_EMBED_MAX_SOURCE_CHUNKS_PER_DOC", MoveToml, NotRuntime, Some("embed.max-source-chunks-per-doc"), WarnEnvOverride, false),
spec("AXON_EMBED_DEDUPE_EXACT_CHUNKS", MoveToml, NotRuntime, Some("embed.dedupe-exact-chunks"), WarnEnvOverride, false),
spec("AXON_QDRANT_UPSERT_BATCH_SIZE", MoveToml, NotRuntime, Some("qdrant.upsert-batch-size"), WarnEnvOverride, false),
spec("AXON_QDRANT_UPSERT_PARALLELISM", MoveToml, NotRuntime, Some("qdrant.upsert-parallelism"), WarnEnvOverride, false),
spec("AXON_CODE_SEARCH_ALLOWED_ROOTS", TrustedOperatorBootstrap, HostOnly, None, Advanced, false),
spec("AXON_WATCH_TICK_SECS", MoveToml, NotRuntime, Some("watch.tick-secs"), WarnEnvOverride, false),
spec("AXON_WATCH_LEASE_SECS", MoveToml, NotRuntime, Some("watch.lease-secs"), WarnEnvOverride, false),
```

Use `TrustedOperatorBootstrap` instead of `MoveToml` only for filesystem allowlists or host-specific paths that are necessary runtime bootstrap.

- [ ] **Step 4: Run the test and confirm it passes**

Run:

```bash
cargo test -p axon-core env_registry
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/axon-core/src/config/parse/env_registry.rs crates/axon-core/src/config/parse/env_registry/*.rs crates/axon-core/src/config/parse/env_registry_tests.rs
git commit -m "test: guard axon env registry completeness"
```

### Task 2: Move Env-Only Non-Secret Tuning Into Typed TOML

**Files:**
- Modify: `crates/axon-core/src/config/parse/toml_config.rs`
- Modify: `crates/axon-core/src/config/parse/tuning.rs`
- Modify: implementation modules that currently read tuning env directly
- Test: `crates/axon-core/src/config/parse/tuning_tests.rs`

**Interfaces:**
- Produces: new TOML sections `[embed]`, `[chunking]`, `[qdrant]`, `[code-search]`, `[watch]`, `[endpoints]`, `[mcp.embed]`, and `[logging]` where fields are non-secret tuning.
- Preserves: env compatibility override behavior.

- [ ] **Step 1: Write failing TOML parse tests for new sections**

Add a test to `tuning_tests.rs`:

```rust
#[test]
fn extended_toml_tuning_sections_parse() {
    let raw = r#"
[embed]
tei-max-concurrent = 8
tei-max-in-flight-inputs = 512
pool-max-inputs = 1024
prep-concurrency = 12
max-chunks-per-doc = 0
max-source-chunks-per-doc = 0
dedupe-exact-chunks = true

[chunking]
markdown-min-chars = 500
markdown-max-chars = 2000
overlap-chars = 200

[qdrant]
upsert-batch-size = 1024
upsert-parallelism = 1
bulk-load = false
bulk-indexing-threshold-kb = 10485760
indexing-threshold-kb = 20000
hnsw-m = 32
hnsw-ef-construct = 256
payload-index-profile = "full"
payload-index-parallelism = 16
hnsw-on-disk = false
quantization-always-ram = true

[watch]
tick-secs = 15
lease-secs = 300

[endpoints]
bundle-concurrency = 8
chrome-concurrency = 2
verify-concurrency = 16
probe-concurrency = 16

[mcp.embed]
max-local-bytes = 10485760
max-local-depth = 16
max-local-entries = 10000
"#;

    crate::config::parse::validate_toml_config_text(raw).unwrap();
}
```

- [ ] **Step 2: Run the test and confirm it fails on unknown fields**

Run:

```bash
cargo test -p axon-core extended_toml_tuning_sections_parse
```

Expected: FAIL with unknown TOML field/section errors.

- [ ] **Step 3: Add typed TOML structs**

In `toml_config.rs`, add fields to `TomlConfig` and structs like:

```rust
#[serde(default)]
pub embed: TomlEmbedSection,
#[serde(default)]
pub chunking: TomlChunkingSection,
#[serde(default)]
pub qdrant: TomlQdrantSection,
#[serde(default)]
pub watch: TomlWatchSection,
#[serde(default)]
pub endpoints: TomlEndpointsSection,
#[serde(default)]
pub mcp: TomlMcpSection,

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub(super) struct TomlEmbedSection {
    pub tei_max_concurrent: Option<usize>,
    pub tei_max_in_flight_inputs: Option<usize>,
    pub pool_max_inputs: Option<usize>,
    pub prep_concurrency: Option<usize>,
    pub max_chunks_per_doc: Option<usize>,
    pub max_source_chunks_per_doc: Option<usize>,
    pub dedupe_exact_chunks: Option<bool>,
}
```

Repeat this exact pattern for chunking, qdrant, watch, endpoints, and `mcp.embed`.

- [ ] **Step 4: Add resolver helpers that preserve env override precedence**

In `tuning.rs`, add helpers with the same clamp/defaults as the current implementation:

```rust
pub(crate) fn embed_tei_max_concurrent(toml: &TomlConfig) -> usize {
    resolve_clamped_usize("AXON_TEI_MAX_CONCURRENT", toml.embed.tei_max_concurrent, 8, 1, 64)
}

pub(crate) fn embed_tei_max_in_flight_inputs(toml: &TomlConfig) -> usize {
    resolve_clamped_usize(
        "AXON_TEI_MAX_IN_FLIGHT_INPUTS",
        toml.embed.tei_max_in_flight_inputs,
        320,
        1,
        4096,
    )
}
```

Use these helpers from the current env-reading modules instead of reading env directly.

- [ ] **Step 5: Update module-local tests**

For each module changed from direct env reads to config/TOML helpers, add one test that proves TOML works and env still wins. Example expected pattern:

```rust
#[test]
fn env_override_still_wins_over_toml_for_embed_concurrency() {
    // Set temp AXON_CONFIG_PATH to a TOML file with embed.tei-max-concurrent = 4.
    // Set AXON_TEI_MAX_CONCURRENT=9.
    // Build config and assert resolved value is 9.
}
```

- [ ] **Step 6: Run focused tests**

Run:

```bash
cargo test -p axon-core config::parse
cargo test -p axon-vector tei_client
cargo test -p axon-code-index config
cargo test -p axon-jobs watch
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/axon-core crates/axon-vector crates/axon-code-index crates/axon-jobs crates/axon-services crates/axon-mcp
git commit -m "feat: support toml for axon tuning knobs"
```

### Task 3: Update Examples and Live Config Files

**Files:**
- Modify: `.env.example`
- Create: `config.toml.example`
- Modify: `config.example.toml`
- Modify locally only: `/home/jmagar/.axon/.env`
- Modify locally only: `/home/jmagar/.axon/config.toml`

**Interfaces:**
- Consumes: env registry and TOML schema from Tasks 1 and 2.
- Produces: examples and live files that match the boundary.

- [ ] **Step 1: Back up live files outside git**

Run:

```bash
install -m 600 /home/jmagar/.axon/.env /home/jmagar/.axon/.env.bak-2026-06-26
install -m 600 /home/jmagar/.axon/config.toml /home/jmagar/.axon/config.toml.bak-2026-06-26
```

Expected: backup files exist and remain mode `600`.

- [ ] **Step 2: Rewrite `.env.example` to env-only categories**

Keep these categories only:

```dotenv
# Data + service URLs
AXON_DATA_DIR=
AXON_HOME=
QDRANT_URL=http://127.0.0.1:53333
TEI_URL=http://127.0.0.1:52000
AXON_CHROME_REMOTE_URL=http://127.0.0.1:6000

# Search/ingest URLs and secrets
AXON_SEARXNG_URL=
TAVILY_API_KEY=
GITHUB_TOKEN=
GITLAB_TOKEN=
GITEA_TOKEN=
REDDIT_CLIENT_ID=
REDDIT_CLIENT_SECRET=
HF_TOKEN=

# LLM runtime, URLs, commands, auth
AXON_LLM_BACKEND=
AXON_OPENAI_BASE_URL=
AXON_OPENAI_API_KEY=
AXON_CODEX_CMD=
AXON_CODEX_HOME=
GEMINI_API_KEY=
GOOGLE_API_KEY=
GEMINI_HOME=
AXON_HEADLESS_GEMINI_HOME=
AXON_HEADLESS_GEMINI_CMD=

# MCP/auth/bootstrap
AXON_MCP_HTTP_HOST=127.0.0.1
AXON_MCP_HTTP_PORT=8001
AXON_MCP_HTTP_PUBLISH=8001
AXON_MCP_HTTP_TOKEN=
AXON_MCP_AUTH_MODE=bearer
AXON_MCP_PUBLIC_URL=
AXON_MCP_GOOGLE_CLIENT_ID=
AXON_MCP_GOOGLE_CLIENT_SECRET=
AXON_MCP_AUTH_ADMIN_EMAIL=
AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS=
AXON_MCP_ALLOWED_ORIGINS=
AXON_WEB_API_TOKEN=

# Compose interpolation / external container runtime
AXON_QDRANT_URL=
AXON_IMAGE=
QDRANT_HTTP_PORT=53333
QDRANT_GRPC_PORT=53334
TEI_EMBEDDING_MODEL=Qwen/Qwen3-Embedding-0.6B
TEI_HTTP_PORT=52000
TEI_SERVER_MAX_CLIENT_BATCH_SIZE=256
TEI_MAX_CONCURRENT_REQUESTS=512
TEI_MAX_BATCH_TOKENS=196608
TEI_MAX_BATCH_REQUESTS=512
TEI_POOLING=last-token
TEI_TOKENIZATION_WORKERS=20
NVIDIA_VISIBLE_DEVICES=0
CUDA_VISIBLE_DEVICES=0
```

Do not include ask/search/embed/Qdrant/client tuning keys in `.env.example`.

- [ ] **Step 3: Create `config.toml.example` with all TOML keys**

Start from current `config.example.toml`, add the new sections from Task 2, and ensure every non-secret tuning key has a commented default and env compatibility note.

- [ ] **Step 4: Keep `config.example.toml` compatible**

Use one of these approaches:

```bash
ln -sf config.toml.example config.example.toml
```

or keep `config.example.toml` as a byte-for-byte copy if symlinks are awkward for release packaging. If using a copy, add a test in Task 5 to compare the files.

- [ ] **Step 5: Migrate live `.env`**

Remove these live env keys after their values are represented in TOML or intentionally superseded by TOML:

```text
AXON_ASK_BACKFILL_CHUNKS
AXON_ASK_CANDIDATE_LIMIT
AXON_ASK_DOC_CHUNK_LIMIT
AXON_ASK_DOC_FETCH_CONCURRENCY
AXON_ASK_FULL_DOCS
TEI_MAX_CLIENT_BATCH_SIZE
AXON_TEI_MAX_CONCURRENT
AXON_TEI_MAX_IN_FLIGHT_INPUTS
AXON_EMBED_POOL_MAX_INPUTS
AXON_EMBED_PREP_CONCURRENCY
AXON_EMBED_MAX_CHUNKS_PER_DOC
AXON_EMBED_MAX_SOURCE_CHUNKS_PER_DOC
AXON_EMBED_DEDUPE_EXACT_CHUNKS
AXON_QDRANT_UPSERT_BATCH_SIZE
AXON_QDRANT_UPSERT_PARALLELISM
```

Preserve all secret values and all URL/auth/bootstrap/Compose variables.

- [ ] **Step 6: Migrate live `config.toml`**

Add current operational values for all current features. Use existing TOML values where they are more capable than stale env overrides:

```toml
[ask]
candidate-limit = 250
full-docs = 6
backfill-chunks = 5
doc-fetch-concurrency = 4
doc-chunk-limit = 96

[embed]
tei-max-concurrent = 8
tei-max-in-flight-inputs = 512
pool-max-inputs = 1024
prep-concurrency = 12
max-chunks-per-doc = 0
max-source-chunks-per-doc = 0
dedupe-exact-chunks = true

[qdrant]
upsert-batch-size = 1024
upsert-parallelism = 1
```

Also add defaults for newly supported sections so all available knobs are visible.

- [ ] **Step 7: Validate live config**

Run:

```bash
./scripts/axon config list
./scripts/axon doctor
```

Expected: `config list` shows migrated values under `config.toml` and no migrated keys under `.env`; `doctor` remains overall successful.

- [ ] **Step 8: Commit repo examples only**

```bash
git add .env.example config.toml.example config.example.toml
git commit -m "docs: align axon config examples"
```

Do not commit `/home/jmagar/.axon/*`.

### Task 4: Update Documentation

**Files:**
- Modify: `docs/guides/configuration.md`
- Modify: `README.md`
- Modify: `CLAUDE.md`

**Interfaces:**
- Consumes: examples from Task 3.
- Produces: current operator docs for all available levers.

- [ ] **Step 1: Update the configuration guide tables**

In `docs/guides/configuration.md`, add tables for `[embed]`, `[chunking]`, `[qdrant]`, `[code-search]`, `[watch]`, `[endpoints]`, `[mcp.embed]`, and `[logging]`.

- [ ] **Step 2: Clarify env-only versus TOML**

Add this exact rule near the top:

```markdown
The normal placement rule is strict: `.env` is for endpoint URLs, credentials,
auth/bootstrap, host paths that must exist before config loading, and Docker
Compose interpolation. Non-secret tuning belongs in `config.toml`. Env
compatibility overrides still exist for scripts and one-off debugging, but
they should not be stored in `~/.axon/.env`.
```

- [ ] **Step 3: Update root README configuration summary**

Point Quick Start to:

```bash
cp .env.example ~/.axon/.env
cp config.toml.example ~/.axon/config.toml
```

Mention `config.example.toml` only as a compatibility alias.

- [ ] **Step 4: Update `CLAUDE.md`**

Refresh the project-doc configuration sections so future agents do not reintroduce tuning env drift.

- [ ] **Step 5: Commit docs**

```bash
git add docs/guides/configuration.md README.md CLAUDE.md
git commit -m "docs: document axon env and toml surfaces"
```

### Task 5: Add Drift Gates

**Files:**
- Modify: `crates/axon-core/src/config/parse/parse_tests.rs`
- Modify: `tests/compose_env_contract.rs`
- Optional create: `tests/config_examples_contract.rs`

**Interfaces:**
- Produces: CI failures when examples/docs drift from parser and env registry.

- [ ] **Step 1: Assert both TOML example names parse**

Add:

```rust
#[test]
fn config_toml_example_parses() {
    let raw = std::fs::read_to_string("config.toml.example").unwrap();
    axon_core::config::parse::validate_toml_config_text(&raw).unwrap();
}

#[test]
fn legacy_config_example_toml_parses() {
    let raw = std::fs::read_to_string("config.example.toml").unwrap();
    axon_core::config::parse::validate_toml_config_text(&raw).unwrap();
}
```

- [ ] **Step 2: Assert `.env.example` has no MoveToml keys**

Add a test that parses `.env.example`, looks up each key in `env_registry::spec_for`, and fails if `classification == MoveToml`.

- [ ] **Step 3: Assert `config.toml.example` covers all TOML destinations**

Add a test that walks `env_registry::all_specs()`, collects `toml_destination`, and verifies each destination string appears as a commented or uncommented key in `config.toml.example`.

- [ ] **Step 4: Run full config tests**

Run:

```bash
cargo test -p axon-core config
cargo test --test compose_env_contract
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/axon-core/src/config/parse/parse_tests.rs tests/compose_env_contract.rs tests/config_examples_contract.rs
git commit -m "test: guard axon config example drift"
```

### Task 6: Final Live Verification

**Files:**
- No source edits unless a verification failure points to a bug.

**Interfaces:**
- Consumes: all previous tasks.
- Produces: proof that live config supports current features.

- [ ] **Step 1: Show git state**

Run:

```bash
git status --short --branch
```

Expected: clean except any intentional uncommitted live-only notes.

- [ ] **Step 2: Show redacted live config**

Run:

```bash
./scripts/axon config list
```

Expected:
- `.env` has URLs, secrets, auth/bootstrap, and Compose interpolation.
- `config.toml` has ask/search/TEI client/workers/embed/chunking/Qdrant/code-search/watch/endpoints/MCP tuning.
- No `AXON_ASK_*` tuning keys remain in `.env`.
- No `TEI_MAX_CLIENT_BATCH_SIZE` remains in `.env`.

- [ ] **Step 3: Run live doctor**

Run:

```bash
./scripts/axon doctor
```

Expected: overall successful, including SQLite, TEI, Qdrant, Chrome, and pipeline checks.

- [ ] **Step 4: Smoke key feature config paths**

Run:

```bash
./scripts/axon query "config drift" --limit 1 --json
./scripts/axon search "Axon RAG configuration" --limit 1 --json
./scripts/axon scrape https://example.com --wait true --json
```

Expected: all commands exit 0; no missing env/config errors.

- [ ] **Step 5: Final commit if verification caused source/doc edits**

```bash
git status --short
git add <changed-source-doc-test-files>
git commit -m "chore: verify axon config alignment"
```

Do not commit live `~/.axon` files.

## Self-Review

- Spec coverage: The plan covers live `.env`, live `config.toml`, `.env.example`, `config.toml.example`, compatibility `config.example.toml`, docs, parser/schema support, registry drift, and live validation.
- Placeholder scan: No task uses TBD/fill-in/later language.
- Type consistency: TOML destination names match the proposed kebab-case sections and registry `toml_destination` values.
