# Axon Production Readiness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship Axon through one Docker Compose production install path shared by the CLI, one-line installer, Claude plugin, web panel, and remote SSH Compose deployment.

**Architecture:** `axon setup` becomes the single Rust-owned orchestration contract. Shell scripts, plugin hooks, web handlers, and remote deploy code delegate to focused setup services for config generation, Compose control, preflight checks, health checks, Qwen3 prewarm, first-run smoke, and reporting.

**Tech Stack:** Rust, Tokio, Clap, Axum, Docker Compose, GHCR, Qdrant, Hugging Face TEI, Qwen/Qwen3-Embedding-0.6B, Gemini CLI, SQLite, Next.js static export embedded with RustEmbed, Beads.

---

## Scope And Execution Order

This plan implements Beads epic `axon_rust-yke8` and its children:

- `axon_rust-yke8.1`: config/env contract
- `axon_rust-yke8.2`: production Docker image and Compose stack
- `axon_rust-yke8.3`: idempotent local and remote Docker setup
- `axon_rust-yke8.4`: Claude plugin hook
- `axon_rust-yke8.5`: one-line installer
- `axon_rust-yke8.6`: CLI help and graph removal
- `axon_rust-yke8.7`: web panel first-run/status UX
- `axon_rust-yke8.8`: README and active docs refresh
- `axon_rust-yke8.9`: stale runtime cleanup and CI release gates

Parallel waves from `bd swarm validate axon_rust-yke8`:

1. Config/env contract, Docker image/Compose, CLI help/graph removal.
2. Shared setup, CI/release cleanup.
3. Plugin hook, installer, web panel.
4. README and active docs.

Do not split `.monolith-allowlist` files as part of this plan unless CI directly blocks release.

## File Structure

Create or modify these files with the following responsibilities.

### Setup Contract

- Modify `src/cli/commands/setup.rs`: route `axon setup`, `setup check`, `setup repair`, `setup deploy`, and JSON output to setup services.
- Modify `src/services/setup.rs`: expose the public setup service API.
- Create `src/services/setup/local.rs`: local first-run and repair orchestration.
- Create `src/services/setup/report.rs`: typed phase reporting for human and JSON output.
- Create `src/services/setup/preflight.rs`: Docker, Compose, NVIDIA runtime, Gemini auth, OAuth config, port ownership checks.
- Create `src/services/setup/compose.rs`: local Docker Compose command wrapper and production compose file resolution.
- Create `src/services/setup/health.rs`: Qdrant, TEI, Chrome, Axon server health checks.
- Create `src/services/setup/prewarm.rs`: Qwen3 embed prewarm with model, dimension, and duration reporting.
- Create `src/services/setup/smoke.rs`: first crawl and first ask smoke helpers.
- Modify `src/services/setup/deploy.rs`: keep remote SSH Compose deploy, make public/local URL mode explicit, reuse health/prewarm where possible.
- Modify `src/services/setup/config_store.rs`: safe atomic no-follow writes for setup-owned config files.

### Config And Environment

- Modify `.env.example`: reduce to URLs, secrets, runtime/bootstrap variables, and Docker interpolation variables.
- Modify `config.example.toml`: own non-secret production tuning.
- Modify `src/core/config/parse/toml_config.rs`: parse newly documented TOML settings.
- Modify `src/core/config/parse/tuning.rs`: align defaults with docs and tests.
- Modify `src/core/config/types/config.rs`: add fields only when backed by parser and examples.
- Modify `src/core/config/types/overrides.rs`: keep CLI/env/TOML/default precedence predictable.
- Create or modify `tests/compose_env_contract.rs`: enforce allowed `.env.example` keys.

### Docker And Release

- Modify `docker-compose.yaml`: production image default, loopback infra ports, RTX 4070-safe TEI values, OAuth-compatible healthcheck.
- Modify `config/Dockerfile`: guarantee web assets are present before RustEmbed compilation.
- Modify `config/chrome/Dockerfile`: keep Chrome headless runtime compatible.
- Create `.github/workflows/docker-image.yml`: build and publish GHCR image.
- Modify `.github/workflows/ci.yml`: remove stale production gates and add contract checks.
- Create `.github/workflows/compose-smoke.yml`: production Compose smoke using released image or built local image.
- Create `.github/workflows/gpu-qwen3-smoke.yml`: self-hosted RTX 4070 smoke for Qwen3 cold/warm setup timing.

### Auth, CLI, And Graph Removal

- Modify `src/web/server.rs`: protect `/v1/ask` with the same bearer/OAuth policy as MCP/actions.
- Modify `src/mcp/auth.rs`: expose shared auth utilities needed by web routes without weakening MCP auth.
- Modify `src/core/config/cli.rs` and `src/core/config/cli/global_args.rs`: make help command-specific and hide compatibility flags.
- Modify `src/mcp/schema.rs` and `src/mcp/server/**`: remove or reject graph fields from MCP request surfaces.
- Modify `src/core/neo4j.rs` and graph call sites: remove production-reachable graph code or gate with tested compatibility errors.
- Modify tests such as `tests/mcp_contract_parity.rs`: assert no graph surface drift.

### Plugin And Installer

- Modify `scripts/plugin-setup.sh`: remove systemd and plugin-cache binary ownership; delegate to installer/setup.
- Modify `.claude-plugin/plugin.json`: reduce user config surface.
- Modify `plugins/hooks/hooks.json`: keep SessionStart hook thin.
- Modify `plugins/.mcp.json`: target shared server URL/token.
- Modify `plugins/README.md`: document Docker Compose setup.
- Create `install.sh` or `scripts/install.sh`: verified binary installer that delegates to `axon setup`.
- Modify release workflow assets to publish checksums/signatures consumed by the installer.

### Web Panel And Docs

- Modify `src/web/server.rs` and `src/web/actions.rs`: add typed status/setup endpoints if existing endpoints cannot express setup state.
- Modify `apps/web/app/page.tsx`: Docker stack status, first crawl, first ask, OAuth/Gemini/TEI status.
- Modify `apps/web/app/globals.css`: keep production panel readable and dense.
- Modify `README.md`: source-aligned front door.
- Create or update `docs/INSTALL.md`, `docs/CONFIG.md`, `docs/DOCKER.md`, `docs/FIRST-RUN.md`, `docs/GEMINI.md`, `docs/MCP.md`, `docs/CLI.md`, `docs/TROUBLESHOOTING.md`, `docs/DEVELOPMENT.md`, `docs/OPERATIONS.md`, and `docs/SECURITY.md`.

---

### Task 0: Baseline And Branch Safety

**Files:**
- Read: `docs/production-readiness-sprint-report-2026-05-12.md`
- Read: `README.md`
- Read: `docker-compose.yaml`
- Read: `src/cli/commands/setup.rs`
- Read: `scripts/plugin-setup.sh`
- Read: `.github/workflows/ci.yml`

- [ ] **Step 1: Confirm worktree state**

Run:

```bash
git status --short --branch
bd show axon_rust-yke8
bd list --parent axon_rust-yke8 --json
```

Expected:

```text
Branch is main or a feature branch based on main.
Existing unrelated dirty files are identified and preserved.
Epic axon_rust-yke8 and nine child beads are visible.
```

- [ ] **Step 2: Claim the first bead being implemented**

Run one command for the bead selected from the current wave:

```bash
bd update axon_rust-yke8.1 --claim
```

Expected:

```text
✓ Updated issue: axon_rust-yke8.1
```

- [ ] **Step 3: Capture current command behavior before changing code**

Run:

```bash
./target/debug/axon setup --json || true
./target/debug/axon setup --help || true
./target/debug/axon --help || true
docker compose --env-file .env.example -f docker-compose.yaml config --quiet
```

Expected:

```text
Current setup behavior and help output are captured in terminal scrollback.
Compose config either passes or reports the exact current failure.
```

- [ ] **Step 4: Commit only if the task makes code/doc changes**

Use this pattern after each task:

```bash
git status --short
git add <files changed by this task>
git commit -m "<type>: <short task summary>"
```

Expected:

```text
Commit contains only files owned by the current task.
Unrelated dirty files remain unstaged.
```

---

### Task 1: Define The Production Config And Env Contract

**Files:**
- Modify: `.env.example`
- Modify: `config.example.toml`
- Modify: `src/core/config/parse/toml_config.rs`
- Modify: `src/core/config/parse/tuning.rs`
- Modify: `src/core/config/types/config.rs`
- Modify: `src/core/config/types/overrides.rs`
- Create or modify: `tests/compose_env_contract.rs`

- [ ] **Step 1: Write the allowed environment key test**

Create or extend `tests/compose_env_contract.rs` with a test shaped like this:

```rust
use std::collections::BTreeSet;

fn env_example_keys() -> BTreeSet<String> {
    include_str!("../.env.example")
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            line.split_once('=')
                .map(|(key, _)| key.trim().to_string())
        })
        .collect()
}

#[test]
fn env_example_only_contains_production_runtime_keys() {
    let allowed: BTreeSet<&str> = [
        "AXON_HOME",
        "AXON_DATA_DIR",
        "AXON_ENV_FILE",
        "AXON_CONFIG_PATH",
        "AXON_SERVER_URL",
        "AXON_IMAGE",
        "AXON_MCP_HTTP_PUBLISH",
        "AXON_MCP_TRANSPORT",
        "AXON_MCP_HTTP_HOST",
        "AXON_MCP_HTTP_PORT",
        "AXON_MCP_HTTP_TOKEN",
        "AXON_MCP_AUTH_MODE",
        "AXON_MCP_PUBLIC_URL",
        "AXON_MCP_GOOGLE_CLIENT_ID",
        "AXON_MCP_GOOGLE_CLIENT_SECRET",
        "AXON_MCP_AUTH_ADMIN_EMAIL",
        "AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS",
        "AXON_MCP_ALLOWED_ORIGINS",
        "QDRANT_URL",
        "TEI_URL",
        "AXON_CHROME_REMOTE_URL",
        "AXON_CHROME_PROXY",
        "HF_TOKEN",
        "TAVILY_API_KEY",
        "GITHUB_TOKEN",
        "REDDIT_CLIENT_ID",
        "REDDIT_CLIENT_SECRET",
        "AXON_HEADLESS_GEMINI_CMD",
        "AXON_HEADLESS_GEMINI_HOME",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
        "GOOGLE_APPLICATION_CREDENTIALS",
        "GOOGLE_CLOUD_PROJECT",
        "GOOGLE_CLOUD_LOCATION",
        "GOOGLE_GENAI_USE_VERTEXAI",
        "RUST_LOG",
    ]
    .into_iter()
    .collect();

    let actual = env_example_keys();
    let unexpected: Vec<_> = actual
        .iter()
        .filter(|key| !allowed.contains(key.as_str()))
        .cloned()
        .collect();

    assert!(
        unexpected.is_empty(),
        "unexpected production env keys in .env.example: {unexpected:?}"
    );
}
```

- [ ] **Step 2: Run the env contract test and verify it fails before cleanup**

Run:

```bash
cargo test --test compose_env_contract env_example_only_contains_production_runtime_keys -- --nocapture
```

Expected:

```text
FAIL
unexpected production env keys in .env.example: [...]
```

- [ ] **Step 3: Move production tuning defaults into `config.example.toml`**

Update `config.example.toml` so it owns non-secret tuning sections. Use kebab-case keys consistently:

```toml
[collection]
name = "cortex"
hybrid-search = true

[search]
hybrid-candidates = 100
hnsw-ef-search = 128

[ask]
candidate-limit = 12
hybrid-candidates = 100
max-context-chars = 48000
doc-fetch-concurrency = 8
doc-chunk-limit = 32
full-docs = false
backfill-chunks = true
min-relevance-score = 0.0
authoritative-boost = 0.0
authoritative-domains = []
min-citations-nontrivial = 2

[tei]
max-retries = 4
request-timeout-ms = 30000
max-client-batch-size = 96
max-concurrent = 8

[qdrant]
point-buffer = 256
upsert-batch-size = 128

[workers]
ingest-lanes = 2
embed-lanes = 2
embed-doc-concurrency = 8
embed-doc-timeout-secs = 300

[jobs]
max-pending-crawl-jobs = 100
max-pending-embed-jobs = 100
max-pending-extract-jobs = 100
max-pending-ingest-jobs = 100
stale-timeout-secs = 300
stale-confirm-secs = 60
wait-timeout-secs = 300
queue-summary-secs = 10
inline-bytes-threshold = 1048576

[llm.gemini]
model = ""
completion-concurrency = 4
completion-timeout-secs = 300
```

- [ ] **Step 4: Remove non-secret tuning from `.env.example`**

Keep service URLs, secrets, auth/runtime bootstrap values, and Docker interpolation values. Remove keys such as:

```text
AXON_COLLECTION
AXON_HYBRID_SEARCH
AXON_ASK_HYBRID_CANDIDATES
AXON_HNSW_EF_SEARCH
AXON_ASK_MAX_CONTEXT_CHARS
AXON_ASK_CANDIDATE_LIMIT
AXON_INGEST_LANES
AXON_EMBED_LANES
AXON_JOB_STALE_TIMEOUT_SECS
AXON_LLM_COMPLETION_CONCURRENCY
AXON_LLM_COMPLETION_TIMEOUT_SECS
OPENAI_BASE_URL
OPENAI_API_KEY
OPENAI_MODEL
AXON_LITE
AXON_PG_URL
AXON_TEST_PG_URL
```

- [ ] **Step 5: Add parser coverage for moved TOML keys**

In `src/core/config/parse/toml_config.rs`, map the TOML sections into `Config` fields. Follow existing parser patterns and keep env override compatibility in the build/override layer.

Use a local helper pattern like this where optional TOML fields are already represented:

```rust
fn apply_u32_field(target: &mut u32, value: Option<u32>) {
    if let Some(value) = value {
        *target = value;
    }
}

fn apply_usize_field(target: &mut usize, value: Option<usize>) {
    if let Some(value) = value {
        *target = value;
    }
}
```

- [ ] **Step 6: Add config parsing and precedence tests**

Add tests near existing config tests under `src/core/config/**`:

```rust
#[test]
fn config_example_toml_parses() {
    let contents = include_str!("../../../../config.example.toml");
    let parsed = crate::core::config::parse::toml_config::parse_toml_config(contents)
        .expect("config.example.toml should parse");
    assert!(parsed.ask.is_some(), "ask section should be present");
    assert!(parsed.tei.is_some(), "tei section should be present");
}

#[test]
fn ask_hybrid_candidates_default_is_consistent() {
    let cfg = crate::core::config::Config::default();
    assert_eq!(cfg.ask_hybrid_candidates, 100);
}
```

If `Config::default()` is not available, use the repo’s existing test helper constructor in `src/core/config/types/config.rs`.

- [ ] **Step 7: Run config tests**

Run:

```bash
cargo test config_example_toml_parses ask_hybrid_candidates_default_is_consistent -- --nocapture
cargo test --test compose_env_contract -- --nocapture
```

Expected:

```text
PASS config_example_toml_parses
PASS ask_hybrid_candidates_default_is_consistent
PASS env_example_only_contains_production_runtime_keys
```

- [ ] **Step 8: Commit config/env contract**

Run:

```bash
git add .env.example config.example.toml src/core/config tests/compose_env_contract.rs
git commit -m "config: define production env and toml contract"
bd close axon_rust-yke8.1 --reason "Production env/TOML contract implemented and tested"
```

Expected:

```text
Commit created.
axon_rust-yke8.1 closed.
```

---

### Task 2: Publishable Docker Compose Runtime

**Files:**
- Modify: `docker-compose.yaml`
- Modify: `config/Dockerfile`
- Modify: `config/chrome/Dockerfile`
- Create: `.github/workflows/docker-image.yml`
- Create or modify: `.github/workflows/compose-smoke.yml`

- [ ] **Step 1: Change Compose to use a published image by default**

In `docker-compose.yaml`, change the Axon service image and local build behavior to this production-safe pattern:

```yaml
services:
  axon:
    <<: *common-service
    image: ${AXON_IMAGE:-ghcr.io/jmagar/axon:latest}
    container_name: axon
    profiles: ["prod"]
```

Move the local build definition into a separate override file named `docker-compose.dev.yaml`:

```yaml
name: axon

services:
  axon:
    image: axon:local
    build:
      context: .
      dockerfile: config/Dockerfile
```

- [ ] **Step 2: Keep infra ports loopback-only**

Verify and preserve these bindings:

```yaml
axon-qdrant:
  ports:
    - "127.0.0.1:53333:6333"
    - "127.0.0.1:53334:6334"

axon-tei:
  ports:
    - "127.0.0.1:${TEI_HTTP_PORT:-52000}:80"

axon-chrome:
  ports:
    - "127.0.0.1:9222:9222"
    - "127.0.0.1:9223:9223"
    - "127.0.0.1:6000:6000"
```

Do not introduce a default `0.0.0.0` binding for Chrome/CDP.

- [ ] **Step 3: Normalize TEI defaults for RTX 4070**

Set production TEI defaults in `docker-compose.yaml`:

```yaml
command:
  - --model-id
  - ${TEI_EMBEDDING_MODEL:-Qwen/Qwen3-Embedding-0.6B}
  - --dtype
  - float16
  - --max-concurrent-requests
  - "${TEI_MAX_CONCURRENT_REQUESTS:-32}"
  - --max-batch-tokens
  - "${TEI_MAX_BATCH_TOKENS:-65536}"
  - --max-batch-requests
  - "${TEI_MAX_BATCH_REQUESTS:-64}"
  - --max-client-batch-size
  - "${TEI_MAX_CLIENT_BATCH_SIZE:-96}"
  - --pooling
  - ${TEI_POOLING:-last-token}
```

Keep the Axon client-side concurrency default lower than the TEI server limit.

- [ ] **Step 4: Make the Axon healthcheck auth-mode aware**

Replace a healthcheck that only posts to `/mcp` with a script or endpoint that works in bearer and OAuth-only modes. Prefer a local unauthenticated `/healthz` endpoint if one exists; otherwise add one in Task 4.

Target Compose shape:

```yaml
healthcheck:
  test: ["CMD-SHELL", "curl -fsS --max-time 4 http://127.0.0.1:8001/healthz >/dev/null || exit 1"]
  interval: 30s
  timeout: 5s
  retries: 3
  start_period: 30s
```

- [ ] **Step 5: Guarantee web assets before RustEmbed**

In `config/Dockerfile`, build static web assets before `cargo build --release --bin axon`. The Dockerfile should contain an order equivalent to:

```dockerfile
WORKDIR /app/apps/web
RUN npm ci
RUN npm run build

WORKDIR /app
RUN test -d apps/web/out
RUN cargo build --release --bin axon
```

If the web project uses `pnpm`, use the package manager already pinned in `apps/web/package.json`.

- [ ] **Step 6: Add GHCR image workflow**

Create `.github/workflows/docker-image.yml`:

```yaml
name: Docker image

on:
  push:
    branches: [main]
    tags: ["v*"]
  workflow_dispatch:

permissions:
  contents: read
  packages: write

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-buildx-action@v3
      - uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
      - uses: docker/metadata-action@v5
        id: meta
        with:
          images: ghcr.io/${{ github.repository_owner }}/axon
          tags: |
            type=sha
            type=ref,event=tag
            type=raw,value=latest,enable={{is_default_branch}}
      - uses: docker/build-push-action@v6
        with:
          context: .
          file: config/Dockerfile
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha
          cache-to: type=gha,mode=max
```

- [ ] **Step 7: Run Compose config checks**

Run:

```bash
docker compose --env-file .env.example -f docker-compose.yaml config --quiet
docker compose --env-file .env.example -f docker-compose.yaml -f docker-compose.dev.yaml config --quiet
```

Expected:

```text
Both commands exit 0.
```

- [ ] **Step 8: Commit Docker runtime changes**

Run:

```bash
git add docker-compose.yaml docker-compose.dev.yaml config/Dockerfile .github/workflows/docker-image.yml .github/workflows/compose-smoke.yml
git commit -m "docker: prepare production compose and image workflow"
bd close axon_rust-yke8.2 --reason "Production Docker image and compose stack prepared"
```

---

### Task 3: Shared `axon setup` Contract

**Files:**
- Modify: `src/cli/commands/setup.rs`
- Modify: `src/services/setup.rs`
- Create: `src/services/setup/local.rs`
- Create: `src/services/setup/report.rs`
- Create: `src/services/setup/preflight.rs`
- Create: `src/services/setup/compose.rs`
- Create: `src/services/setup/health.rs`
- Create: `src/services/setup/prewarm.rs`
- Create: `src/services/setup/smoke.rs`
- Modify: `src/services/setup/deploy.rs`
- Modify: `src/services/setup/config_store.rs`

- [ ] **Step 1: Add setup report types**

Create `src/services/setup/report.rs`:

```rust
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SetupPhaseStatus {
    Ok,
    Warn,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetupPhase {
    pub name: &'static str,
    pub status: SetupPhaseStatus,
    pub detail: String,
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetupReport {
    pub mode: SetupMode,
    pub phases: Vec<SetupPhase>,
    pub web_url: String,
    pub mcp_url: String,
    pub token_path: String,
    pub met_two_minute_target: bool,
    pub exceeded_five_minute_max: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SetupMode {
    Local,
    Plugin,
    RemoteDeploy,
    Check,
    Repair,
}

impl SetupPhase {
    pub fn ok(name: &'static str, detail: impl Into<String>, duration: Duration) -> Self {
        Self {
            name,
            status: SetupPhaseStatus::Ok,
            detail: detail.into(),
            duration_ms: duration.as_millis(),
        }
    }

    pub fn failed(name: &'static str, detail: impl Into<String>, duration: Duration) -> Self {
        Self {
            name,
            status: SetupPhaseStatus::Failed,
            detail: detail.into(),
            duration_ms: duration.as_millis(),
        }
    }
}
```

- [ ] **Step 2: Wire new modules**

In `src/services/setup.rs`, expose the modules:

```rust
pub mod compose;
pub mod health;
pub mod local;
pub mod preflight;
pub mod prewarm;
pub mod report;
pub mod smoke;
```

Keep existing exports for `DeployRequest`, `deploy_remote`, and SSH target listing.

- [ ] **Step 3: Add setup CLI parsing tests**

Add tests near the existing setup command tests or create `src/cli/commands/setup/tests.rs`:

```rust
#[test]
fn setup_without_subcommand_runs_local_mode() {
    let args = vec!["axon".to_string(), "setup".to_string(), "--json".to_string()];
    let cfg = crate::core::config::Config::from_args_for_test(args)
        .expect("config should parse");
    assert_eq!(cfg.command.as_deref(), Some("setup"));
    assert!(cfg.positional.is_empty());
    assert!(cfg.json_output);
}

#[test]
fn setup_deploy_still_accepts_target() {
    let args = vec![
        "axon".to_string(),
        "setup".to_string(),
        "deploy".to_string(),
        "lab".to_string(),
        "--remote-dir".to_string(),
        "axon-deploy".to_string(),
    ];
    let cfg = crate::core::config::Config::from_args_for_test(args)
        .expect("config should parse");
    assert_eq!(cfg.positional.first().map(String::as_str), Some("deploy"));
    assert_eq!(cfg.positional.get(1).map(String::as_str), Some("lab"));
}
```

If `Config::from_args_for_test` does not exist, use the repo’s existing CLI parse helper and keep the assertions identical.

- [ ] **Step 4: Implement local setup orchestration skeleton**

Create `src/services/setup/local.rs`:

```rust
use super::report::{SetupMode, SetupPhase, SetupReport};
use crate::core::config::Config;
use std::error::Error;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalSetupMode {
    FirstRun,
    Check,
    Repair,
    Plugin,
}

pub async fn run_local_setup(
    cfg: &Config,
    mode: LocalSetupMode,
) -> Result<SetupReport, Box<dyn Error>> {
    let started = Instant::now();
    let mut phases = Vec::new();

    let phase_started = Instant::now();
    super::preflight::check_prerequisites(cfg).await?;
    phases.push(SetupPhase::ok(
        "preflight",
        "Docker, Compose, NVIDIA runtime, Gemini auth, and config paths checked",
        phase_started.elapsed(),
    ));

    let phase_started = Instant::now();
    super::compose::ensure_stack_running(cfg).await?;
    phases.push(SetupPhase::ok(
        "compose_up",
        "Docker Compose stack is running",
        phase_started.elapsed(),
    ));

    let phase_started = Instant::now();
    super::health::wait_for_stack(cfg).await?;
    phases.push(SetupPhase::ok(
        "health",
        "Axon, Qdrant, TEI, and Chrome health checks passed",
        phase_started.elapsed(),
    ));

    let phase_started = Instant::now();
    let prewarm = super::prewarm::prewarm_qwen3(cfg).await?;
    phases.push(SetupPhase::ok(
        "qwen3_prewarm",
        format!("model={}, dimension={}", prewarm.model, prewarm.dimension),
        phase_started.elapsed(),
    ));

    let total = started.elapsed();
    Ok(SetupReport {
        mode: match mode {
            LocalSetupMode::FirstRun => SetupMode::Local,
            LocalSetupMode::Check => SetupMode::Check,
            LocalSetupMode::Repair => SetupMode::Repair,
            LocalSetupMode::Plugin => SetupMode::Plugin,
        },
        phases,
        web_url: "http://127.0.0.1:8001".to_string(),
        mcp_url: "http://127.0.0.1:8001/mcp".to_string(),
        token_path: "~/.axon/.env".to_string(),
        met_two_minute_target: total <= Duration::from_secs(120),
        exceeded_five_minute_max: total > Duration::from_secs(300),
    })
}
```

- [ ] **Step 5: Implement TEI prewarm**

Create `src/services/setup/prewarm.rs`:

```rust
use crate::core::config::Config;
use serde::Deserialize;
use std::error::Error;

#[derive(Debug, Clone)]
pub struct PrewarmResult {
    pub model: String,
    pub dimension: usize,
}

#[derive(Debug, Deserialize)]
struct TeiEmbeddingResponse(Vec<Vec<f32>>);

pub async fn prewarm_qwen3(cfg: &Config) -> Result<PrewarmResult, Box<dyn Error>> {
    let url = format!("{}/embed", cfg.tei_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "inputs": [
            "Axon production setup document embedding prewarm.",
            "Axon production setup query embedding prewarm."
        ]
    });
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .json(&body)
        .send()
        .await?
        .error_for_status()?;
    let vectors: Vec<Vec<f32>> = response.json().await?;
    let dimension = vectors
        .first()
        .map(Vec::len)
        .ok_or("TEI prewarm returned no vectors")?;
    Ok(PrewarmResult {
        model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
        dimension,
    })
}
```

Adjust the response parsing to the exact TEI JSON shape already used in `src/vector/ops/tei/**`.

- [ ] **Step 6: Route CLI setup to local setup**

Modify `src/cli/commands/setup.rs` so the default branch calls local setup instead of printing usage:

```rust
Some("deploy") => {
    // existing deploy branch stays intact
}
Some("check") => {
    let result = setup::local::run_local_setup(cfg, setup::local::LocalSetupMode::Check).await?;
    print_setup_report(cfg, result)?;
    Ok(())
}
Some("repair") => {
    let result = setup::local::run_local_setup(cfg, setup::local::LocalSetupMode::Repair).await?;
    print_setup_report(cfg, result)?;
    Ok(())
}
None => {
    let result = setup::local::run_local_setup(cfg, setup::local::LocalSetupMode::FirstRun).await?;
    print_setup_report(cfg, result)?;
    Ok(())
}
Some(other) => Err(format!("unknown setup subcommand: {other}").into()),
```

Add this helper:

```rust
fn print_setup_report(
    cfg: &Config,
    result: crate::services::setup::report::SetupReport,
) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    println!("Axon setup: {:?}", result.mode);
    for phase in &result.phases {
        println!(
            "{:?}\t{}\t{}ms\t{}",
            phase.status, phase.name, phase.duration_ms, phase.detail
        );
    }
    println!("Web: {}", result.web_url);
    println!("MCP: {}", result.mcp_url);
    println!("Token/config: {}", result.token_path);
    Ok(())
}
```

- [ ] **Step 7: Add failure-path tests**

Add tests that mock or isolate:

```rust
#[test]
fn setup_report_marks_five_minute_exceeded() {
    use crate::services::setup::report::{SetupMode, SetupReport};
    let report = SetupReport {
        mode: SetupMode::Local,
        phases: Vec::new(),
        web_url: "http://127.0.0.1:8001".to_string(),
        mcp_url: "http://127.0.0.1:8001/mcp".to_string(),
        token_path: "~/.axon/.env".to_string(),
        met_two_minute_target: false,
        exceeded_five_minute_max: true,
    };
    assert!(report.exceeded_five_minute_max);
}
```

- [ ] **Step 8: Run setup tests**

Run:

```bash
cargo test setup -- --nocapture
cargo check --bin axon
```

Expected:

```text
All setup unit tests pass.
cargo check exits 0.
```

- [ ] **Step 9: Commit setup contract**

Run:

```bash
git add src/cli/commands/setup.rs src/services/setup.rs src/services/setup
git commit -m "setup: add shared production setup contract"
bd close axon_rust-yke8.3 --reason "Local and remote Docker setup share a typed setup contract"
```

---

### Task 4: Auth Parity And Graph Surface Removal

**Files:**
- Modify: `src/web/server.rs`
- Modify: `src/mcp/auth.rs`
- Modify: `src/core/config/cli.rs`
- Modify: `src/core/config/cli/global_args.rs`
- Modify: `src/mcp/schema.rs`
- Modify: `src/mcp/server/**`
- Modify or remove: `src/core/neo4j.rs`
- Modify: `tests/mcp_contract_parity.rs`

- [ ] **Step 1: Write auth parity tests for `/v1/ask`**

Add web/server auth tests that prove OAuth-only mode does not make `/v1/ask` public. Use existing test harness patterns for `src/web/server.rs`.

Target assertions:

```rust
#[test]
fn v1_ask_rejects_without_auth_when_oauth_mode_is_enabled() {
    // Set AXON_MCP_AUTH_MODE=oauth with a non-loopback public URL using the repo's
    // existing isolated env test helper.
    // Call ask authorization with empty headers.
    // Assert unauthorized.
}

#[test]
fn v1_ask_accepts_static_bearer_token_when_configured() {
    // Set AXON_MCP_HTTP_TOKEN=secret.
    // Call ask authorization with Authorization: Bearer secret.
    // Assert authorized.
}
```

Use the project’s serialized env test guard instead of raw `std::env::set_var` in parallel tests, because Rust 2024 makes env mutation unsafe in tests.

- [ ] **Step 2: Share MCP/action auth policy with `/v1/ask`**

Replace `ask_authorized()` with a call path that enforces the same policy used by `/v1/actions` and MCP HTTP. The function should fail closed when:

```text
AXON_MCP_AUTH_MODE=oauth and no valid OAuth/JWT auth is present.
AXON_MCP_HTTP_TOKEN is set and missing/wrong.
AXON_MCP_HTTP_TOKEN is set to whitespace.
```

Target shape:

```rust
fn ask_authorized(state: &AppState, headers: &HeaderMap) -> bool {
    crate::mcp::auth::authorize_http_request(headers, state.auth_policy()).is_ok()
}
```

Use the exact existing `AuthPolicy` API rather than inventing a second auth type.

- [ ] **Step 3: Add `/healthz` if Compose needs unauthenticated health**

In `src/web/server.rs`, add a local health endpoint that does not expose secrets:

```rust
async fn healthz() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}
```

Register it in the router:

```rust
.route("/healthz", get(healthz))
```

Do not return config values, tokens, or environment-derived paths.

- [ ] **Step 4: Write graph absence contract tests**

In `tests/mcp_contract_parity.rs` or a new CLI help snapshot test:

```rust
#[test]
fn mcp_schema_does_not_expose_graph() {
    let schema = axon::mcp::schema::tool_schema_json();
    let text = serde_json::to_string(&schema).expect("schema serializes");
    assert!(!text.contains("\"graph\""));
    assert!(!text.to_lowercase().contains("neo4j"));
}

#[test]
fn cli_help_does_not_expose_graph() {
    let help = axon::core::config::cli::render_help_for_test();
    assert!(!help.contains("--graph"));
    assert!(!help.to_lowercase().contains("neo4j"));
}
```

Use existing schema/help helpers if names differ.

- [ ] **Step 5: Remove graph from CLI/MCP request surfaces**

Remove or reject fields named:

```text
graph
neo4j
graph_mode
use_graph
```

Search first:

```bash
rg -n "graph|Neo4j|neo4j" src tests docs README.md
```

For production request structs, delete fields. For migration-only compatibility, return an explicit error:

```rust
return Err("graph retrieval is not supported in the production Docker release".into());
```

- [ ] **Step 6: Clean help output**

In `src/core/config/cli/global_args.rs`, hide compatibility flags:

```rust
#[arg(long, hide = true)]
pub lite: bool,
```

Remove env value display from help by removing `default_value` strings derived from current env. Help should show static defaults only.

- [ ] **Step 7: Run auth/help/schema tests**

Run:

```bash
cargo test v1_ask_rejects_without_auth_when_oauth_mode_is_enabled -- --nocapture
cargo test mcp_schema_does_not_expose_graph cli_help_does_not_expose_graph -- --nocapture
./target/debug/axon --help | rg -i "graph|neo4j|OPENAI_BASE_URL|AXON_SERVER_URL" && exit 1 || true
```

Expected:

```text
Auth tests pass.
Graph/schema/help tests pass.
The rg command finds no stale production help output.
```

- [ ] **Step 8: Commit auth and graph cleanup**

Run:

```bash
git add src/web/server.rs src/mcp src/core/config src/core/neo4j.rs tests
git commit -m "fix: align ask auth and remove graph surface"
bd close axon_rust-yke8.6 --reason "CLI help cleaned and graph surface removed"
```

---

### Task 5: Claude Plugin Hook Delegates To Setup

**Files:**
- Modify: `scripts/plugin-setup.sh`
- Modify: `.claude-plugin/plugin.json`
- Modify: `plugins/hooks/hooks.json`
- Modify: `plugins/.mcp.json`
- Modify: `plugins/README.md`

- [ ] **Step 1: Add plugin hook shell test**

Create `scripts/tests/plugin_setup_test.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
script="$repo_root/scripts/plugin-setup.sh"

if rg -n "systemctl|systemd|axon-mcp.service|ln -sf .*bin/axon" "$script"; then
  echo "plugin setup must not create systemd services or canonical plugin-cache axon symlinks" >&2
  exit 1
fi

if ! rg -n "axon setup|install.sh" "$script" >/dev/null; then
  echo "plugin setup must delegate to axon setup or the shared installer" >&2
  exit 1
fi
```

Make it executable:

```bash
chmod +x scripts/tests/plugin_setup_test.sh
```

- [ ] **Step 2: Run the test and verify it fails before hook rewrite**

Run:

```bash
scripts/tests/plugin_setup_test.sh
```

Expected:

```text
FAIL because current script references systemctl/systemd/service or plugin-cache symlink.
```

- [ ] **Step 3: Replace plugin hook with a thin delegator**

Rewrite `scripts/plugin-setup.sh` around this structure:

```bash
#!/usr/bin/env bash
set -euo pipefail

log() {
  printf '[axon-plugin] %s\n' "$*" >&2
}

require_safe_value() {
  local name="$1"
  local value="${2:-}"
  if printf '%s' "$value" | grep -q '[[:cntrl:]]'; then
    printf 'invalid control character in %s\n' "$name" >&2
    exit 2
  fi
}

if command -v axon >/dev/null 2>&1; then
  log "using installed axon: $(command -v axon)"
else
  log "axon not found; installing release binary"
  curl -fsSL "${AXON_INSTALL_URL:-https://raw.githubusercontent.com/jmagar/axon/main/install.sh}" | sh
fi

if [ -n "${CLAUDE_PLUGIN_OPTION_API_TOKEN:-}" ]; then
  require_safe_value "CLAUDE_PLUGIN_OPTION_API_TOKEN" "$CLAUDE_PLUGIN_OPTION_API_TOKEN"
fi

exec axon setup --plugin --ensure-running
```

If final CLI flags differ, use the exact setup flags implemented in Task 3 and keep the script thin.

- [ ] **Step 4: Reduce plugin manifest user config**

In `.claude-plugin/plugin.json`, keep only:

```json
{
  "server_url": {
    "type": "string",
    "default": "http://127.0.0.1:8001",
    "description": "Axon server URL"
  },
  "api_token": {
    "type": "password",
    "description": "Existing Axon MCP/API token. Leave blank to let setup generate one."
  },
  "tavily_api_key": {
    "type": "password",
    "description": "Optional Tavily API key for web search and research."
  },
  "github_token": {
    "type": "password",
    "description": "Optional GitHub token for higher ingest rate limits."
  },
  "reddit_client_id": {
    "type": "string",
    "description": "Optional Reddit client ID for Reddit ingest."
  },
  "reddit_client_secret": {
    "type": "password",
    "description": "Optional Reddit client secret for Reddit ingest."
  }
}
```

Remove OpenAI, Qdrant, TEI, collection, and systemd-oriented prompts.

- [ ] **Step 5: Update plugin MCP config**

Ensure `plugins/.mcp.json` targets the shared server:

```json
{
  "mcpServers": {
    "axon": {
      "url": "${CLAUDE_PLUGIN_OPTION_SERVER_URL}/mcp",
      "headers": {
        "Authorization": "Bearer ${CLAUDE_PLUGIN_OPTION_API_TOKEN}"
      }
    }
  }
}
```

If plugin interpolation requires the existing syntax, preserve syntax but not semantics: URL plus bearer token only.

- [ ] **Step 6: Run plugin checks**

Run:

```bash
scripts/tests/plugin_setup_test.sh
rg -n "systemd|systemctl|axon-mcp.service|OPENAI_BASE_URL|OPENAI_API_KEY|OPENAI_MODEL" scripts/plugin-setup.sh .claude-plugin plugins
```

Expected:

```text
plugin_setup_test.sh exits 0.
rg finds no production plugin references to systemd or OpenAI first-run config.
```

- [ ] **Step 7: Commit plugin hook changes**

Run:

```bash
git add scripts/plugin-setup.sh scripts/tests/plugin_setup_test.sh .claude-plugin/plugin.json plugins/hooks/hooks.json plugins/.mcp.json plugins/README.md
git commit -m "plugin: delegate install to shared docker setup"
bd close axon_rust-yke8.4 --reason "Claude plugin setup delegates to shared Docker setup without systemd"
```

---

### Task 6: Verified One-Line Installer

**Files:**
- Create: `install.sh` or `scripts/install.sh`
- Modify: `.github/workflows/docker-image.yml` or release workflow for checksums
- Modify: `README.md`
- Modify: `docs/INSTALL.md`
- Modify: `docs/FIRST-RUN.md`

- [ ] **Step 1: Write installer shell test**

Create `scripts/tests/install_script_test.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
script="$repo_root/install.sh"

test -f "$script"

required_patterns=(
  "sha256sum"
  "axon setup"
  "mktemp"
  "chmod"
)

for pattern in "${required_patterns[@]}"; do
  if ! rg -n "$pattern" "$script" >/dev/null; then
    echo "install.sh missing required pattern: $pattern" >&2
    exit 1
  fi
done

if rg -n "systemctl|systemd|curl .*\\|.*sh .*axon setup" "$script"; then
  echo "install.sh must not install systemd services or pipe nested setup scripts blindly" >&2
  exit 1
fi
```

Run:

```bash
chmod +x scripts/tests/install_script_test.sh
scripts/tests/install_script_test.sh
```

Expected before implementation:

```text
FAIL because install.sh does not exist.
```

- [ ] **Step 2: Add installer**

Create `install.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

repo="${AXON_REPO:-jmagar/axon}"
version="${AXON_VERSION:-latest}"
prefix="${AXON_PREFIX:-$HOME/.local}"
bin_dir="$prefix/bin"
tmp_dir="$(mktemp -d)"

cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

die() {
  printf 'axon install: %s\n' "$*" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

need curl
need sha256sum
need uname

arch="$(uname -m)"
case "$arch" in
  x86_64|amd64) target="x86_64-unknown-linux-gnu" ;;
  *) die "unsupported architecture: $arch" ;;
esac

if ! command -v docker >/dev/null 2>&1; then
  die "Docker is required before installing Axon"
fi

if ! docker compose version >/dev/null 2>&1; then
  die "Docker Compose v2 is required before installing Axon"
fi

if ! command -v gemini >/dev/null 2>&1; then
  die "Gemini CLI must be installed and authenticated before Axon setup"
fi

mkdir -p "$bin_dir"

base="https://github.com/$repo/releases"
if [ "$version" = "latest" ]; then
  url="$base/latest/download/axon-$target.tar.gz"
  sums="$base/latest/download/SHA256SUMS"
else
  url="$base/download/$version/axon-$target.tar.gz"
  sums="$base/download/$version/SHA256SUMS"
fi

archive="$tmp_dir/axon.tar.gz"
checksums="$tmp_dir/SHA256SUMS"

curl -fsSL "$url" -o "$archive"
curl -fsSL "$sums" -o "$checksums"

expected_line="$(grep "axon-$target.tar.gz" "$checksums" || true)"
[ -n "$expected_line" ] || die "checksum for axon-$target.tar.gz not found"

printf '%s\n' "$expected_line" | (cd "$tmp_dir" && sha256sum -c -)

tar -xzf "$archive" -C "$tmp_dir"
test -x "$tmp_dir/axon" || die "release archive did not contain executable axon"

install -m 0755 "$tmp_dir/axon" "$bin_dir/axon"

export PATH="$bin_dir:$PATH"
exec "$bin_dir/axon" setup
```

- [ ] **Step 3: Add release checksum generation**

In the release workflow that builds host binaries, add:

```yaml
- name: Generate checksums
  run: |
    cd dist
    sha256sum axon-*.tar.gz > SHA256SUMS
```

Upload `SHA256SUMS` alongside release archives.

- [ ] **Step 4: Run installer static test**

Run:

```bash
scripts/tests/install_script_test.sh
shellcheck install.sh scripts/plugin-setup.sh
```

Expected:

```text
Both scripts pass static checks.
```

- [ ] **Step 5: Run installer dry path with local fake release**

Use a local test harness or temp HTTP server if available. The test must verify:

```text
Checksum mismatch fails before install.
Wrong architecture fails before install.
Existing ~/.axon is not deleted.
Successful install calls axon setup.
```

If no shell test harness exists, create `scripts/tests/install_fake_release_test.sh` with temp directories and a fake `axon` script that records `setup` invocation.

- [ ] **Step 6: Commit installer**

Run:

```bash
git add install.sh scripts/tests/install_script_test.sh .github/workflows README.md docs/INSTALL.md docs/FIRST-RUN.md
git commit -m "install: add verified one-line setup bootstrap"
bd close axon_rust-yke8.5 --reason "Verified one-line installer delegates to shared setup"
```

---

### Task 7: Web Panel First-Run And Stack Status UX

**Files:**
- Modify: `src/web/server.rs`
- Modify: `src/web/actions.rs`
- Modify: `apps/web/app/page.tsx`
- Modify: `apps/web/app/globals.css`

- [ ] **Step 1: Add typed status response**

In `src/web/server.rs` or a new web status module, add:

```rust
use serde::Serialize;

#[derive(Debug, Serialize)]
struct StackStatusResponse {
    docker: CheckState,
    nvidia: CheckState,
    qdrant: CheckState,
    tei: TeiStatus,
    chrome: CheckState,
    gemini: CheckState,
    oauth: CheckState,
    mcp_token: CheckState,
    server_url: String,
}

#[derive(Debug, Serialize)]
struct CheckState {
    state: &'static str,
    detail: String,
}

#[derive(Debug, Serialize)]
struct TeiStatus {
    state: &'static str,
    model: String,
    dimension: Option<usize>,
    prewarmed: bool,
    detail: String,
}
```

- [ ] **Step 2: Add status endpoint test**

Add a test that serializes a sample status:

```rust
#[test]
fn stack_status_response_serializes_expected_fields() {
    let response = StackStatusResponse {
        docker: CheckState { state: "healthy", detail: "Docker daemon reachable".to_string() },
        nvidia: CheckState { state: "healthy", detail: "NVIDIA runtime available".to_string() },
        qdrant: CheckState { state: "healthy", detail: "readyz ok".to_string() },
        tei: TeiStatus {
            state: "healthy",
            model: "Qwen/Qwen3-Embedding-0.6B".to_string(),
            dimension: Some(1024),
            prewarmed: true,
            detail: "embed prewarm ok".to_string(),
        },
        chrome: CheckState { state: "healthy", detail: "CDP reachable".to_string() },
        gemini: CheckState { state: "healthy", detail: "Gemini CLI auth detected".to_string() },
        oauth: CheckState { state: "configured", detail: "OAuth configured".to_string() },
        mcp_token: CheckState { state: "configured", detail: "token present".to_string() },
        server_url: "http://127.0.0.1:8001".to_string(),
    };
    let text = serde_json::to_string(&response).expect("serializes");
    assert!(text.contains("Qwen/Qwen3-Embedding-0.6B"));
    assert!(text.contains("prewarmed"));
}
```

- [ ] **Step 3: Implement status endpoint through setup services**

Add route:

```rust
.route("/api/panel/stack/status", get(stack_status))
```

Handler:

```rust
async fn stack_status(
    State((state, cfg)): State<(AppState, Arc<crate::core::config::Config>)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match crate::services::setup::health::collect_stack_status(&cfg).await {
        Ok(status) => Json(status).into_response(),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": err.to_string(),
                "diagnostic_command": "axon setup check --json"
            })),
        )
            .into_response(),
    }
}
```

- [ ] **Step 4: Update panel UI**

In `apps/web/app/page.tsx`, render:

```tsx
const checks = [
  ["Docker", status?.docker],
  ["NVIDIA", status?.nvidia],
  ["Qdrant", status?.qdrant],
  ["TEI", status?.tei],
  ["Chrome", status?.chrome],
  ["Gemini", status?.gemini],
  ["OAuth", status?.oauth],
  ["MCP Token", status?.mcp_token],
];
```

Each row should show:

```tsx
<span className={`statusDot ${check.state}`} />
<strong>{label}</strong>
<span>{check.detail}</span>
```

Do not show raw tokens.

- [ ] **Step 5: Add first crawl/ask controls**

Use existing `/v1/actions` or action endpoints for crawl and `/v1/ask` for ask. UI labels:

```text
URL
Start crawl
Question
Ask
```

The error panel should print:

```text
Diagnostic: axon setup check --json
Logs: ~/.axon/logs/
```

- [ ] **Step 6: Run frontend and Rust checks**

Run:

```bash
(cd apps/web && npm run build)
cargo test stack_status_response_serializes_expected_fields -- --nocapture
cargo check --bin axon
```

Expected:

```text
Next/static export succeeds.
Rust status test passes.
cargo check exits 0.
```

- [ ] **Step 7: Commit web panel**

Run:

```bash
git add src/web apps/web
git commit -m "web: add production stack status and first-run panel"
bd close axon_rust-yke8.7 --reason "Web panel exposes Docker stack status and first-run workflow"
```

---

### Task 8: CI And Release Gates

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `.github/workflows/docker-image.yml`
- Create or modify: `.github/workflows/compose-smoke.yml`
- Create: `.github/workflows/gpu-qwen3-smoke.yml`
- Modify: `scripts/**` CI helper scripts

- [ ] **Step 1: Add stale runtime CI guard**

Create `scripts/check-production-docs-and-ci.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

blocked_regex='postgres|redis|rabbitmq|amqp|neo4j|systemd|OPENAI_BASE_URL|OPENAI_API_KEY'

if rg -n -i "$blocked_regex" README.md docs/INSTALL.md docs/DOCKER.md docs/CONFIG.md docs/MCP.md docs/SECURITY.md .github/workflows; then
  echo "production docs/workflows contain blocked runtime references" >&2
  exit 1
fi
```

Make executable:

```bash
chmod +x scripts/check-production-docs-and-ci.sh
```

- [ ] **Step 2: Remove stale production services from CI**

In `.github/workflows/ci.yml`, remove service blocks for:

```yaml
postgres:
redis:
rabbitmq:
```

Keep only tests that are still source-relevant. If a test still needs one of those services, move it out of production CI and label it legacy/internal.

- [ ] **Step 3: Add compose smoke workflow**

Create `.github/workflows/compose-smoke.yml`:

```yaml
name: Compose smoke

on:
  pull_request:
  workflow_dispatch:

jobs:
  compose-smoke:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Compose config
        run: docker compose --env-file .env.example -f docker-compose.yaml config --quiet
      - name: Build local image for smoke
        run: docker build -f config/Dockerfile -t axon:smoke .
      - name: Start stack
        run: |
          AXON_IMAGE=axon:smoke docker compose --env-file .env.example -f docker-compose.yaml up -d axon-qdrant axon-chrome axon
      - name: Check server health
        run: curl -fsS http://127.0.0.1:8001/healthz
      - name: Logs on failure
        if: failure()
        run: docker compose -f docker-compose.yaml logs --no-color
```

If TEI cannot run on GitHub-hosted CPU with production Qwen3, keep TEI out of this hosted smoke and make the GPU workflow release-blocking.

- [ ] **Step 4: Add GPU Qwen3 workflow**

Create `.github/workflows/gpu-qwen3-smoke.yml`:

```yaml
name: GPU Qwen3 smoke

on:
  workflow_dispatch:
  push:
    tags: ["v*"]

jobs:
  gpu-smoke:
    runs-on: [self-hosted, linux, x64, rtx-4070]
    timeout-minutes: 20
    steps:
      - uses: actions/checkout@v4
      - name: Build image
        run: docker build -f config/Dockerfile -t axon:gpu-smoke .
      - name: Run setup smoke
        run: |
          AXON_IMAGE=axon:gpu-smoke ./target/release/axon setup --json | tee setup-report.json
      - name: Enforce setup target
        run: |
          jq -e '.exceeded_five_minute_max == false' setup-report.json
```

- [ ] **Step 5: Add release gate commands to CI**

In `.github/workflows/ci.yml`, add:

```yaml
- name: Production docs and CI stale surface check
  run: scripts/check-production-docs-and-ci.sh

- name: Compose config
  run: docker compose --env-file .env.example -f docker-compose.yaml config --quiet
```

- [ ] **Step 6: Run CI checks locally**

Run:

```bash
scripts/check-production-docs-and-ci.sh
docker compose --env-file .env.example -f docker-compose.yaml config --quiet
cargo test --all
```

Expected:

```text
Stale runtime guard passes after docs are updated.
Compose config passes.
Rust tests pass.
```

- [ ] **Step 7: Commit CI release gates**

Run:

```bash
git add .github/workflows scripts/check-production-docs-and-ci.sh
git commit -m "ci: gate production docker release path"
bd close axon_rust-yke8.9 --reason "CI release gates prove production Docker path"
```

---

### Task 9: README And Active Production Docs

**Files:**
- Modify: `README.md`
- Create or modify: `docs/INSTALL.md`
- Create or modify: `docs/CONFIG.md`
- Create or modify: `docs/DOCKER.md`
- Create or modify: `docs/FIRST-RUN.md`
- Create or modify: `docs/GEMINI.md`
- Create or modify: `docs/MCP.md`
- Create or modify: `docs/CLI.md`
- Create or modify: `docs/TROUBLESHOOTING.md`
- Create or modify: `docs/DEVELOPMENT.md`
- Create or modify: `docs/OPERATIONS.md`
- Create or modify: `docs/SECURITY.md`

- [ ] **Step 1: Rewrite README first-run section**

README must open with:

```markdown
## Production Quick Start

Axon production setup uses Docker Compose only. The host installs a small `axon`
client binary; the long-running server, Qdrant, TEI/Qwen3, and Chrome run in
Docker Compose. Both the one-line installer and Claude plugin use the same
`~/.axon/.env` and `~/.axon/config.toml`.

Prerequisites:

- Linux x86_64
- Docker with Compose v2
- NVIDIA Container Toolkit with an RTX 4070-class GPU
- Gemini CLI installed and already authenticated

Install:

```bash
curl -fsSL https://raw.githubusercontent.com/jmagar/axon/main/install.sh | sh
```
```

- [ ] **Step 2: Document config boundary**

In `docs/CONFIG.md`, include:

```markdown
| File | Owns | Examples |
| --- | --- | --- |
| `~/.axon/.env` | URLs, secrets, auth/runtime bootstrap, Docker interpolation | `QDRANT_URL`, `TEI_URL`, `AXON_MCP_HTTP_TOKEN`, `HF_TOKEN` |
| `~/.axon/config.toml` | Non-secret behavior and tuning | collection, ask/search, workers, jobs, TEI client limits |
```

Add:

```markdown
Precedence is CLI flag > environment variable > `~/.axon/config.toml` > built-in default.
```

- [ ] **Step 3: Document Docker security**

In `docs/DOCKER.md`, include:

```markdown
Qdrant, TEI, and Chrome/CDP are bound to `127.0.0.1` by default. Do not expose
Chrome/CDP directly on a public interface; Chrome DevTools Protocol has no Axon
authentication layer. Remote deployments should use SSH tunnels or an
authenticated reverse proxy.
```

- [ ] **Step 4: Document first-run timing honestly**

In `docs/FIRST-RUN.md`, include:

```markdown
The target is under 2 minutes from installer start to first crawl plus ask when
the Axon image, service images, and Qwen3 model cache are warm or download
quickly. The cold path has a 5 minute maximum; setup reports the phase that
exceeded the budget.
```

- [ ] **Step 5: Document Gemini CLI requirement**

In `docs/GEMINI.md`, include:

```markdown
Axon production LLM operations use Gemini CLI only. The installer and setup do
not create a Gemini subscription or perform browser login. Run `gemini` on the
host first and complete authentication before `axon setup`.
```

- [ ] **Step 6: Document MCP/auth parity**

In `docs/MCP.md` and `docs/SECURITY.md`, include:

```markdown
MCP, `/v1/actions`, and `/v1/ask` use the same production auth policy. Static
bearer token mode uses `AXON_MCP_HTTP_TOKEN`. OAuth mode uses lab-auth and
requires a valid public URL and redirect configuration for non-loopback
deployments.
```

- [ ] **Step 7: Add stale docs guard**

Run:

```bash
scripts/check-production-docs-and-ci.sh
```

If it fails, remove production-facing references to blocked runtime paths from active docs. Archive docs may retain history only when clearly marked non-authoritative and excluded from the guard.

- [ ] **Step 8: Run docs and help checks**

Run:

```bash
./target/debug/axon --help > /tmp/axon-help.txt
rg -n "systemd|Postgres|Redis|RabbitMQ|AMQP|OPENAI_BASE_URL|Neo4j|--graph" README.md docs/INSTALL.md docs/CONFIG.md docs/DOCKER.md docs/MCP.md docs/SECURITY.md /tmp/axon-help.txt
```

Expected:

```text
No production-facing stale runtime references are found.
```

- [ ] **Step 9: Commit docs**

Run:

```bash
git add README.md docs scripts/check-production-docs-and-ci.sh
git commit -m "docs: refresh production install and operations guide"
bd close axon_rust-yke8.8 --reason "README and active docs match production contract"
```

---

### Task 10: Final Production Verification

**Files:**
- No planned source changes
- Reads all files changed by prior tasks

- [ ] **Step 1: Run full local quality gate**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all --all-features
docker compose --env-file .env.example -f docker-compose.yaml config --quiet
scripts/check-production-docs-and-ci.sh
scripts/tests/plugin_setup_test.sh
scripts/tests/install_script_test.sh
```

Expected:

```text
All commands exit 0.
```

- [ ] **Step 2: Run setup smoke on target hardware**

On the RTX 4070 host with Docker, NVIDIA runtime, and Gemini auth:

```bash
axon setup --json | tee /tmp/axon-setup-report.json
jq '.met_two_minute_target, .exceeded_five_minute_max, .phases[] | {name, status, duration_ms, detail}' /tmp/axon-setup-report.json
```

Expected:

```text
exceeded_five_minute_max is false.
qwen3_prewarm phase is ok and includes model/dimension.
first crawl and first ask phases are ok when setup runs full first-run smoke.
```

- [ ] **Step 3: Run first crawl and ask manually**

Run:

```bash
axon crawl https://example.com --wait true
axon ask "What did we crawl?"
```

Expected:

```text
Crawl completes.
Ask returns an answer through server mode with citations or stored-source references.
```

- [ ] **Step 4: Verify plugin path uses same server**

Run:

```bash
rg -n "systemd|systemctl|axon-mcp.service|plugin-cache" scripts/plugin-setup.sh plugins .claude-plugin && exit 1 || true
rg -n "127.0.0.1:8001|/mcp" plugins/.mcp.json .claude-plugin/plugin.json
```

Expected:

```text
No systemd/plugin-cache runtime ownership remains.
Plugin MCP config points to the shared server endpoint.
```

- [ ] **Step 5: Close epic and push**

Run:

```bash
bd swarm validate axon_rust-yke8
bd close axon_rust-yke8 --reason "Production Docker Compose install, setup, docs, and release gates complete"
bd dolt push
git status --short --branch
git push
```

Expected:

```text
Swarm validation passes.
Epic is closed.
Beads push completes.
Git branch is pushed.
```

---

## Self-Review

### Spec Coverage

- Docker Compose-only deployment is covered by Tasks 2, 3, 8, and 9.
- Remote SSH deploy as Compose orchestration is preserved in Task 3.
- Systemd binary deployment removal is covered by Tasks 3, 5, 6, and 9.
- Shared `~/.axon/.env` and `~/.axon/config.toml` is covered by Tasks 1, 3, 5, and 6.
- OAuth/lab-auth is covered by Tasks 1, 3, 4, 7, and 9.
- Gemini CLI-only production LLM path is covered by Tasks 1, 3, 6, and 9.
- Qdrant-only and TEI/Qwen3-only production paths are covered by Tasks 1, 2, 3, 8, and 9.
- Qwen3 prewarm and timing targets are covered by Tasks 2, 3, 8, and 10.
- CLI help cleanup and graph removal are covered by Task 4.
- Web panel first-run and status UX are covered by Task 7.
- Docs refresh is covered by Task 9.
- CI and Docker image publishing are covered by Tasks 2 and 8.
- `.monolith-allowlist` cleanup is intentionally excluded except as a release blocker.

### Placeholder Scan

The plan was scanned for placeholder language and open-ended task text. Each implementation task names files, commands, expected outcomes, and concrete snippets.

### Type Consistency

Setup report types introduced in Task 3 are used consistently by setup, web status, and final verification. Auth parity is tracked through `AuthPolicy`-style shared authorization rather than a second token-only mechanism.
