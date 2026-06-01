# Axon Production Readiness Sprint Report

Date: 2026-05-12

Scope: historical information-gathering report from 2026-05-12. No source, configuration, workflow, hook, gitignore, or tracker changes were made as part of the original report.

Status note (updated 2026-05-19): the local setup surface has since changed. Current commands are `axon setup` (init + stack up + preflight), `axon setup init`, `axon preflight`, `axon stack up|down|restart|rebuild`, and `axon smoke`. Sections below that say "current" describe the 2026-05-12 state unless explicitly marked updated.

## Release Target

Initial production release constraints:

- Deployment path: Docker Compose only.
- Install methods:
  - One-line installer that gets a working Axon client and Docker stack onto the machine.
  - Claude Code plugin install that uses the same config home and the same Docker deployment.
- Runtime services:
  - Qdrant only for vector storage.
  - Hugging Face TEI only for embeddings.
  - Qwen/Qwen3-Embedding-0.6B only for the production embedding model.
  - Gemini CLI only for LLM operations, assuming the user already has a valid Gemini CLI subscription login.
- Auth: keep the existing OAuth/lab-auth implementation in the production surface, alongside static bearer token support where appropriate.
- Hardware target: local NVIDIA RTX 4070.
- Default operating mode: local client talks to long-running Axon server.
- Shared config home: `~/.axon/.env`, `~/.axon/config.toml`, and `~/.axon/` data/log/artifact directories regardless of install method.
- Release infrastructure: GitHub Actions should build and publish the Docker image.
- Target setup time: under 2 minutes from not installed to successful first crawl plus ask; 5 minutes is the absolute maximum acceptable cold-start target.

The setup path should aggressively pre-pull and prewarm everything it can, including the TEI Qwen3 model. A true cold start that pulls the Axon image, Qdrant image, Chrome image, TEI image, and Qwen3 model weights may still exceed 2 minutes on slower networks, but the product target remains under 2 minutes with 5 minutes as the hard ceiling.

## Executive Summary

The repo has the pieces needed for a strong production path, but they are currently split across conflicting install stories:

- `docker-compose.yaml` already defines the desired runtime shape: Axon server, Qdrant, TEI, and Chrome.
- `axon setup` now has a local bootstrap wrapper; any remaining remote deployment story should share the same Docker-only production contract.
- The Claude plugin hook currently writes `~/.axon/.env`, installs a systemd user service, restarts `axon serve mcp`, and symlinks a plugin-cache binary into `~/.local/bin/axon`. This directly conflicts with the new Docker-only target.
- `.env.example` exposes too many tuning knobs. Many of them belong in `config.toml`; some should be removed from the production surface entirely.
- CLI help is noisy because broad global flags are flattened into almost every subcommand, help output leaks current environment values, and the top-level custom help omits several real commands.
- README and docs are extensive, but they are not source-of-truth safe today. There is known stale content around systemd, unclear remote Docker deploy positioning, OpenAI-compatible LLMs, worker subcommands, removed infra, and old path layouts.
- There is no GitHub workflow that builds and publishes the production Docker image.
- `.monolith-allowlist` cleanup is intentionally out of scope for this production sprint. Do not spend sprint capacity splitting allowlisted files unless CI blocks production release work.

Recommended sprint shape:

1. Freeze the production contract: Docker Compose, shared `~/.axon`, Gemini CLI, Qdrant, TEI, Qwen3.
2. Fix config/env boundaries before rewriting docs.
3. Unify local, plugin, and remote SSH-orchestrated deployment around one idempotent Docker Compose setup flow, with no systemd-managed Axon binary service.
4. Make the Claude plugin hook delegate to the same setup flow.
5. Redesign CLI help around production-first commands and hidden advanced flags.
6. Refresh README and active docs from the final code contract, not from current stale docs.
7. Remove dead code and stale CI paths after the Docker/config contract is merged.

## Evidence Collected

Commands and inspections used during this pass:

- `git status --short --branch`
- `git ls-files .monolith-allowlist`
- `git check-ignore -v .monolith-allowlist`
- `bd prime`
- `bd ready --json`
- `python3 scripts/enforce_monoliths.py --whole-repo --include-allowlisted`
- `./target/debug/axon --version`
- `./target/debug/axon --help`
- `./target/debug/axon setup --help`
- `./target/debug/axon crawl --help`
- `./target/debug/axon serve --help`
- `./target/debug/axon mcp --help`
- `./target/debug/axon setup --json`
- `docker compose --env-file .env.example -f docker-compose.yaml config --quiet`
- `rg` sweeps over `README.md`, `docs/`, `.claude-plugin/`, `plugins/`, `scripts/`, `src/`, `.github/`, and config examples for stale runtime, env, setup, systemd, OpenAI, Postgres, Redis, AMQP, worker, and graph references.

Not run:

- Full build/test gates were not run because this was not an implementation pass.
- `cargo machete` was attempted but is not installed, so unused dependency detection still needs a real run with the tool installed or a manual dependency pass.

Current git context at inspection time:

- Branch: `main`, ahead of `origin/main` by 6 commits.
- Dirty worktree already existed before this report. Those changes were left untouched.
- New report path should be the only intended artifact from this pass.

## Target Production Architecture

The production install should converge on one model:

```text
Host
  ~/.axon/
    .env                 # URLs, secrets, runtime/bootstrap-only env
    config.toml          # non-secret tuning and behavior
    jobs.db
    output/
    logs/
    artifacts/
    screenshots/
    chrome-diagnostics/
  ~/.gemini/             # existing Gemini CLI auth, mounted read-only into container
  ~/.local/bin/axon      # host client binary

Docker Compose
  axon                   # long-running server, web panel, MCP HTTP, action API
  axon-qdrant             # Qdrant
  axon-tei                # HF TEI with Qwen/Qwen3-Embedding-0.6B
  axon-chrome             # browser rendering service
```

Host client behavior:

- Host `axon` defaults to `AXON_SERVER_URL=http://127.0.0.1:8001`.
- Server-routable commands call the server by default.
- `--local` remains available for explicit in-process debugging.
- The container must clear `AXON_SERVER_URL` internally so the server never calls itself through the client path.
- MCP HTTP auth should retain the implemented OAuth/lab-auth path and static bearer-token compatibility.

Install flow:

1. Verify prerequisites: Docker, Docker Compose, NVIDIA runtime, reachable GPU, writable `~/.axon`, Gemini CLI auth.
2. Download or install the host `axon` client binary.
3. Generate `~/.axon/.env` and `~/.axon/config.toml` from templates.
4. Generate an MCP/API token if missing.
5. Pull the published Axon image from GHCR plus Qdrant/TEI/Chrome images.
6. Prewarm the TEI Qwen3 model cache.
7. Start the Docker Compose stack.
8. Wait for Qdrant, TEI, Chrome, Axon server, and auth health.
9. Run `axon doctor`.
10. Run a first crawl smoke.
11. Run a first ask smoke.
12. Print the web panel URL, MCP URL, token location, OAuth status, and fastest diagnostic command.

Claude plugin flow:

- Plugin SessionStart hook should not own a separate deployment mechanism.
- It should detect `~/.axon` and the Docker Compose stack.
- If Axon is absent, it should invoke the same `axon setup` / installer path.
- If Axon is present but down, it should start the compose stack.
- If config exists, it should preserve it and only fill missing keys.
- It should configure Claude to talk to the same `http://127.0.0.1:8001/mcp` endpoint and token.

## Current Install And Setup State

### CLI Setup Command

Historical 2026-05-12 state: `axon setup` was a remote Docker deployment helper, not a local install/setup wizard. Updated 2026-05-19: local setup is split into `setup init`, `preflight`, `stack`, and `smoke`, with `setup` as a convenience wrapper.

Observed behavior:

- Historical: `axon setup` required a subcommand.
- Historical: `axon setup --json` failed before command handling when no subcommand was provided.
- Existing subcommands are remote deployment oriented:
  - `axon setup targets`
  - `axon setup deploy <ssh-alias> [--remote-dir ...] [--accept-new-host-key] [--public-exposure]`

Current implementation paths:

- `src/cli/commands/setup.rs`
- `src/services/setup/deploy.rs`
- `src/services/setup/assets.rs`
- `src/services/setup/config_store.rs`

What it does today:

- Enumerates SSH targets from `~/.ssh/config`.
- Connects via SSH.
- Validates remote Docker, curl, Docker Compose, and daemon access.
- Uploads compose assets and env files.
- Starts remote Docker infrastructure services.
- Writes service URLs back into local `~/.axon/config.toml`.

Production issue:

- The SSH orchestration itself is not the problem if it deploys Docker Compose.
- The problem is any path that deploys or supervises the Axon binary through systemd instead of the Docker stack.
- The current setup shape includes a local bootstrap path, because `axon setup` with no subcommand is useful on the machine being installed.
- The remote Docker deploy path should stay, but it should be made part of the same production setup contract as local setup.

Required action:

- Keep and harden the current SSH-driven Docker deployment path.
- Remove any systemd-managed binary deployment assumptions from setup, plugin hooks, Justfile recipes, and docs.
- Add a local default setup path.
- Ensure local setup and remote setup generate compatible `~/.axon/.env` and `~/.axon/config.toml` files.
- Ensure remote Docker deploy either starts the full production stack or clearly explains when it is infrastructure-only.
- Preferred production commands:

```bash
axon setup
axon setup init
axon preflight
axon stack up
axon smoke
axon doctor
axon setup deploy <ssh-alias>
```

### One-Line Installer

There is no production one-line installer today.

Recommended design:

```bash
curl -fsSL https://raw.githubusercontent.com/<owner>/<repo>/main/install.sh | sh
```

The script should:

- Download a release binary or small bootstrap binary.
- Verify checksum/signature when release artifacts are available.
- Install `axon` to `~/.local/bin` or a user-selected prefix.
- Run `axon setup`.
- Avoid duplicating setup logic in shell. Shell should bootstrap, Rust should own the real checks and config writes.

For a stable release, prefer:

```bash
curl -fsSL https://install.axon.dev | sh
```

That endpoint can resolve the latest release and keep README instructions short.

### Wrapper Script

`scripts/axon` is useful for development because it sources env and runs `cargo run`, but it should not be used as production proof:

- It exercises `target/debug/axon`.
- It makes setup timing look much worse.
- It bypasses the installed release binary path that users will actually use.

Production docs should distinguish:

- Development: `./scripts/axon ...`
- Installed runtime: `axon ...`

## Current Claude Plugin Install Flow

Current plugin files:

- `.claude-plugin/plugin.json`
- `plugins/hooks/hooks.json`
- `plugins/.mcp.json`
- `scripts/plugin-setup.sh`
- `plugins/README.md`

Current flow:

1. Claude plugin manifest prompts for user config.
2. `SessionStart` hook runs `${CLAUDE_PLUGIN_ROOT}/scripts/plugin-setup.sh`.
3. The setup script requires `CLAUDE_PLUGIN_OPTION_API_TOKEN`.
4. It creates `~/.axon/.env`.
5. It writes or updates managed env keys.
6. It creates `~/.config/systemd/user/axon-mcp.service`.
7. The systemd user service runs:

```bash
${CLAUDE_PLUGIN_ROOT}/bin/axon serve mcp
```

8. The hook runs `systemctl --user daemon-reload`.
9. It enables/restarts/starts `axon-mcp.service`.
10. It symlinks the plugin binary into `~/.local/bin/axon`.
11. `.mcp.json` points Claude at `${server_url}/mcp` using bearer auth.

Current managed env keys include:

- `QDRANT_URL`
- `TEI_URL`
- `AXON_COLLECTION`
- `AXON_HOME`
- `AXON_DATA_DIR`
- `OPENAI_BASE_URL`
- `OPENAI_API_KEY`
- `OPENAI_MODEL`
- `TAVILY_API_KEY`
- `AXON_CHROME_REMOTE_URL`
- `AXON_MCP_HTTP_HOST`
- `AXON_MCP_HTTP_PORT`
- `AXON_MCP_HTTP_TOKEN`
- Optional OAuth keys

Current issues:

- It deploys systemd, which is out of scope for the production release.
- It does not start or validate the Docker Compose stack.
- It prompts for and writes too many values that should be defaults or TOML config.
- It exposes OpenAI-compatible settings even though the release target is Gemini CLI only.
- It writes `AXON_COLLECTION` to `.env`; this belongs in `config.toml`.
- It defaults `mcp_host` inconsistently: plugin manifest says `127.0.0.1`, script fallback uses `0.0.0.0`.
- It writes a plugin-cache binary symlink to `~/.local/bin/axon`, which has already caused stale binary/server drift in prior runtime work.
- It preserves unknown `.env` keys, which is good, but lacks a typed config migration story.
- Plugin docs describe systemd as the deployment path.

Target plugin behavior:

- No systemd unit.
- No plugin-cache binary as the canonical host binary.
- No separate env/config layout.
- No OpenAI prompts in first-run UX.
- Hook should call:

```bash
axon setup plugin-hook
```

or equivalent.

If `axon` is not installed, the hook should use the same released installer used by the one-line path.

The plugin should ask for the minimum possible config:

- Required only if missing: MCP/API token, or permission to generate one.
- Optional: server URL override for non-local deployments.
- Optional: Tavily key for search/research.
- Optional: GitHub/Reddit credentials for richer ingest.

Everything else should be defaulted and documented.

## Docker Compose State

Current `docker-compose.yaml` already mostly matches the desired production runtime:

- `axon`
- `axon-qdrant`
- `axon-tei`
- `axon-chrome`

Current strengths:

- Axon server container mounts `${AXON_HOME:-${HOME}/.axon}` to `/home/axon/.axon`.
- Gemini auth is mounted read-only from host `~/.gemini`.
- Qdrant, TEI, and Chrome are on the compose network.
- The app container clears `AXON_SERVER_URL`, which is correct for avoiding self-calls.
- `docker compose --env-file .env.example -f docker-compose.yaml config --quiet` passes.

Current issues:

- Axon image is `axon:local` with local build context. Production needs a published image.
- There is no GitHub workflow to publish the image.
- TEI concurrency differs across compose and env examples. Normalize for RTX 4070.
- `.env.example` is too large and mixes runtime URLs/secrets with tuning knobs.
- Production docs still conflate remote Docker deployment with systemd/binary deployment in places.

Required production compose decisions:

- Published image name, for example `ghcr.io/jmagar/axon:<version>`.
- Tag policy: `latest`, semver, git SHA.
- Whether `docker-compose.yaml` remains local-build oriented and production uses `docker-compose.prod.yaml`, or one compose file supports both with `AXON_IMAGE`.
- Where model cache lives. If TEI downloads Qwen on first run, cold setup may exceed 2 minutes.
- Whether to include a preflight check for NVIDIA Container Toolkit.

Recommended compose contract:

```env
AXON_IMAGE=ghcr.io/jmagar/axon:latest
AXON_HOME=/home/user/.axon
AXON_MCP_HTTP_PUBLISH=127.0.0.1:8001
HF_TOKEN=
```

Everything else should be in `config.toml` or hardcoded production defaults unless it is a secret, URL, or Docker runtime setting.

## Config And Environment Contract

Current config loading is a good base:

- `src/main.rs` loads env from `AXON_ENV_FILE`, then `~/.axon/.env`, then nearest `.env`.
- `~/.axon/.env` symlinks are rejected.
- Config priority is CLI flags > env vars > `~/.axon/config.toml` > defaults.
- `config.example.toml` already has sections for services, search, ask, TEI, and workers.

Current issue:

- `.env.example` is treated as the place for too many knobs.
- Many env vars are operational tuning, not secrets or runtime bootstrap.
- Some vars are compatibility or stale release surfaces.

### Keep In `.env`

These are appropriate as environment variables because they are URLs, secrets, auth state pointers, runtime bootstrap values, or Docker Compose interpolation inputs.

| Category | Vars |
| --- | --- |
| Service URLs | `QDRANT_URL`, `TEI_URL`, `AXON_SERVER_URL`, `AXON_CHROME_REMOTE_URL`, `AXON_CHROME_PROXY` |
| Data paths needed before config is loaded | `AXON_HOME`, `AXON_DATA_DIR`, `AXON_SQLITE_PATH`, `AXON_ENV_FILE`, `AXON_CONFIG_PATH` |
| Docker/compose publishing | `AXON_IMAGE`, `AXON_MCP_HTTP_PUBLISH`, GPU/compose-specific vars |
| MCP HTTP runtime | `AXON_MCP_TRANSPORT`, `AXON_MCP_HTTP_HOST`, `AXON_MCP_HTTP_PORT`, `AXON_MCP_HTTP_TOKEN` |
| OAuth/lab-auth | `AXON_MCP_AUTH_MODE`, `AXON_MCP_PUBLIC_URL`, `AXON_MCP_GOOGLE_CLIENT_ID`, `AXON_MCP_GOOGLE_CLIENT_SECRET`, `AXON_MCP_AUTH_ADMIN_EMAIL`, `AXON_MCP_AUTH_ALLOWED_REDIRECT_URIS`, `AXON_MCP_ALLOWED_ORIGINS` |
| External service secrets | `HF_TOKEN`, `TAVILY_API_KEY`, `GITHUB_TOKEN`, `REDDIT_CLIENT_ID`, `REDDIT_CLIENT_SECRET` |
| Gemini auth/runtime | `AXON_HEADLESS_GEMINI_CMD`, `AXON_HEADLESS_GEMINI_HOME`, `GEMINI_API_KEY`, `GOOGLE_API_KEY`, `GOOGLE_APPLICATION_CREDENTIALS`, `GOOGLE_CLOUD_PROJECT`, `GOOGLE_CLOUD_LOCATION`, `GOOGLE_GENAI_USE_VERTEXAI` |
| Filesystem output overrides | `AXON_MCP_ARTIFACT_DIR`, `AXON_OUTPUT_DIR`, `SCREENSHOT_DIRECTORY`, `AXON_LOG_DIR`, `AXON_LOG_FILE` |
| Process logging | `RUST_LOG` |

Some path vars can be either env or TOML. Keep env support for bootstrap and container use, but document `config.toml` as the normal user-facing place when possible.

### Move To `config.toml`

These should move out of `.env.example` and become documented TOML settings with env compatibility retained only where needed.

| Proposed section | Current env vars / settings |
| --- | --- |
| `[collection]` | `AXON_COLLECTION`, `AXON_HYBRID_SEARCH` |
| `[search]` | `AXON_HYBRID_CANDIDATES`, `AXON_ASK_HYBRID_CANDIDATES`, `AXON_HNSW_EF_SEARCH`, `AXON_HNSW_EF_SEARCH_LEGACY` |
| `[ask]` | `AXON_ASK_MAX_CONTEXT_CHARS`, `AXON_ASK_CANDIDATE_LIMIT`, `AXON_ASK_DOC_FETCH_CONCURRENCY`, `AXON_ASK_DOC_CHUNK_LIMIT`, `AXON_ASK_CHUNK_LIMIT`, `AXON_ASK_FULL_DOCS`, `AXON_ASK_BACKFILL_CHUNKS`, `AXON_ASK_MIN_RELEVANCE_SCORE`, `AXON_ASK_AUTHORITATIVE_BOOST`, `AXON_ASK_AUTHORITATIVE_DOMAINS`, `AXON_ASK_MIN_CITATIONS_NONTRIVIAL` |
| `[suggest]` | `AXON_SUGGEST_*` |
| `[tei]` | `TEI_MAX_RETRIES`, `TEI_REQUEST_TIMEOUT_MS`, `TEI_MAX_CLIENT_BATCH_SIZE`, `AXON_TEI_MAX_CONCURRENT` |
| `[qdrant]` | `AXON_QDRANT_POINT_BUFFER`, `AXON_QDRANT_UPSERT_BATCH_SIZE` |
| `[workers]` | `AXON_EMBED_LANES`, `AXON_INGEST_LANES`, `AXON_EMBED_DOC_CONCURRENCY`, `AXON_EMBED_DOC_TIMEOUT_SECS` |
| `[jobs]` | `AXON_MAX_PENDING_CRAWL_JOBS`, `AXON_MAX_PENDING_EMBED_JOBS`, `AXON_MAX_PENDING_EXTRACT_JOBS`, `AXON_MAX_PENDING_INGEST_JOBS`, `AXON_JOB_STALE_TIMEOUT_SECS`, `AXON_JOB_STALE_CONFIRM_SECS`, `AXON_JOB_WAIT_TIMEOUT_SECS`, `AXON_QUEUE_SUMMARY_SECS`, `AXON_INLINE_BYTES_THRESHOLD` |
| `[mcp.embed]` | `AXON_MCP_EMBED_ALLOWED_ROOTS`, `AXON_MCP_EMBED_MAX_LOCAL_BYTES` |
| `[llm.gemini]` | `AXON_HEADLESS_GEMINI_MODEL`, `AXON_LLM_COMPLETION_CONCURRENCY`, `AXON_LLM_COMPLETION_TIMEOUT_SECS` |
| `[ingest.github]` | `GITHUB_MAX_ISSUES`, `GITHUB_MAX_PRS` |
| `[sessions]` | `AXON_SESSION_INGEST_MAX_BYTES` |
| `[chrome]` | Chrome user-agent, diagnostics, and browser behavior flags |
| `[logging]` | `AXON_LOG_LEVEL`, `AXON_LOG_MAX_BYTES`, `AXON_LOG_MAX_FILES`, `AXON_LOG_FULL_QUERIES`, `AXON_NO_COLOR` |
| `[ui]` or `[output]` | `AXON_NO_WIPE`, `AXON_DOMAINS_DETAILED`, `AXON_DOMAINS_DETAILED_LIMIT`, `AXON_SOURCES_FACET_LIMIT`, `AXON_DOMAINS_FACET_LIMIT` |

Known config inconsistency:

- `config.example.toml` documents `ask-hybrid-candidates = 100`.
- Current code path inspected in `src/core/config/parse/tuning.rs` defaults to 150.
- A config test in `src/core/config/types/config.rs` expects 100.
- Pick one value and make code, tests, `config.example.toml`, README, and docs agree.

### Remove Or Deprecate From Production Surface

| Vars / surface | Reason |
| --- | --- |
| `OPENAI_BASE_URL`, `OPENAI_API_KEY`, `OPENAI_MODEL` | Release target is Gemini CLI only. If compatibility remains internally, hide it from first-run docs and plugin prompts. |
| `AXON_LITE` | Runtime is already SQLite/in-process. Keep as accepted compatibility only; remove from production docs. |
| `AXON_PG_URL`, `AXON_TEST_PG_URL`, Redis, RabbitMQ, AMQP vars | Removed runtime path. CI and docs still contain old references. |
| Neo4j / graph flags | Graph retrieval is not production-supported; MCP rejects graph mode. Remove the code, flags, docs, and tests unless a migration compatibility shim is explicitly needed. |
| Systemd service env | Docker-only release should not require systemd. |
| Plugin-cache binary path assumptions | Prior stale binary/server drift came from this pattern. |

## CLI Help UX

Current help issues:

- Top-level `axon --help` is custom and incomplete.
- It omits real commands including `serve`, `setup`, `watch`, `sessions`, `screenshot`, `dedupe`, `migrate`, and `completions`.
- Subcommand help includes unrelated global flags because `GlobalArgs` is flattened everywhere.
- Historical: `axon setup --help` showed crawl, Chrome, ask, vector, server, and tuning flags.
- Help output shows current env values, for example `AXON_SERVER_URL`, `AXON_COLLECTION`, and `AXON_CHROME_REMOTE_URL`. This is confusing and can leak sensitive or machine-local details if applied broadly.
- `--lite` remains visible as if it changes behavior.
- `--graph` advertises a Neo4j path that is not production-supported.
- `stats` source text still references Postgres job statistics.
- MCP wording is inconsistent: `axon mcp` is described as HTTP in places even though transport can be stdio or HTTP.

Required changes:

1. Stop flattening all global flags into every command help.
2. Split flags into:
   - always-visible common flags: `--json`, `--yes`, `--config`, `--env-file`, `--server-url`, `--local`.
   - command-specific flags.
   - advanced hidden flags.
3. Disable environment value display in help output.
4. Hide compatibility-only flags such as `--lite`.
5. Remove graph flags and help text from the production CLI surface.
6. Make top-level help generated from the command registry or keep the custom help in lockstep with tests.
7. Add help snapshot tests for top-level help and representative subcommands.
8. Add a clean `axon setup` default path.

Recommended top-level command groups:

- First run: `setup`, `doctor`, `serve`
- Core RAG: `crawl`, `scrape`, `embed`, `query`, `ask`, `retrieve`
- Discovery: `map`, `search`, `research`, `suggest`
- Ingest: `ingest`, `sessions`
- Operations: `status`, lifecycle subcommands, `sources`, `domains`, `stats`, `dedupe`, `migrate`
- Integrations: `mcp`, `completions`
- Diagnostics: `debug`, `screenshot`

## Documentation State

README is long and useful, but not reliable enough to be the production source of truth yet.

Observed doc inventory:

- Total markdown/mdx files under docs: 785.
- Active docs excluding obvious archive/plans/reports/sessions buckets: 112.
- Existing stale-doc audit: `docs/reports/2026-05-06-stale-docs-audit/00-MASTER-REPORT.md`.

The May 6 stale-doc audit remains useful but is partly stale itself because it references older path layouts. It should be rerun after the Docker/config contract is implemented.

Current README/doc drift examples:

- README still needs to clearly position SSH deployment as Docker Compose orchestration, not as a separate non-Docker deployment model.
- README configuration section still mixes Docker/systemd server state.
- README and docs mention worker lifecycle commands in ways that no longer match the in-process worker model.
- Docs still reference OpenAI-compatible LLMs in active areas despite the Gemini CLI release target.
- Some docs still mention removed Postgres/Redis/AMQP paths, or over-explain their removal in user-facing docs.
- GitHub workflows still contain old Postgres/Redis/RabbitMQ service setup.
- Several tracker descriptions still refer to old `crates/` paths, which creates issue/docs drift.

README production rewrite target:

- README should become a complete map, not a dumping ground for every detail.
- It should include:
  - What Axon is.
  - Production quick start.
  - Two supported install methods.
  - Docker-only deployment model.
  - Config home and file boundaries.
  - First crawl plus ask workflow.
  - CLI command map.
  - MCP/plugin integration summary.
  - Web panel summary.
  - Troubleshooting quick paths.
  - Links to detailed docs.

Detailed docs that should exist and be current:

- `docs/INSTALL.md`: one-line installer and plugin install.
- `docs/guides/configuration.md`: exact `.env` vs `config.toml` contract.
- `docs/DOCKER.md`: compose services, GPU, ports, volumes, upgrades.
- `docs/GEMINI.md`: required Gemini CLI auth, mount behavior, checks.
- `docs/reference/mcp/overview.md`: transports, auth, Claude plugin integration.
- `docs/CLI.md`: generated command reference or generated links.
- `docs/FIRST-RUN.md`: successful first crawl plus ask in under 2 minutes when cached.
- `docs/TROUBLESHOOTING.md`: fastest commands and common failures.
- `docs/DEVELOPMENT.md`: dev setup, local build, test gates.
- `docs/architecture/overview.md`: source-aligned architecture after removing systemd-managed binary deployment while retaining Docker Compose deployment, including remote SSH orchestration when supported.
- `docs/operations/operations.md`: logs, backups, upgrades, health checks.
- `docs/operations/security.md`: token auth, URL validation, secrets, local binds, destructive operations.

Docs policy:

- Active docs should be audited as a finite set.
- Historical plans, reports, and sessions should be marked archive-only and excluded from freshness claims.
- CLI docs should be generated from Clap or tested against Clap snapshots.
- MCP schema docs should be generated from the source schema or contract tests.

## Web Panel And UX Ergonomics

Current web panel setup UX:

- Login with panel password stored in localStorage.
- Operations cards for collection, Qdrant, TEI, MCP HTTP.
- Raw TOML editor.
- Remote Deploy panel driven by SSH targets.

Production issue:

- This is not aligned with the unified Docker setup contract yet. The panel can remain valuable if it becomes a remote Docker Compose deploy surface and does not imply systemd or binary supervision.

Required UX changes:

- Keep remote deploy, but make it explicit that it is remote Docker Compose deployment. It should not imply systemd or binary supervision.
- Add local Docker Stack status as a first-class view.
- Show:
  - Docker available
  - NVIDIA runtime available
  - Qdrant health
  - TEI health and model
  - Chrome health
  - Gemini CLI auth available
  - MCP token configured
  - Server URL
- Add first-run workflow:
  - Crawl URL input
  - Crawl progress/status
  - Ask input
  - Answer plus citations
  - Clear failure messages
- Avoid raw TOML as the primary UX. Keep advanced edit mode, but provide structured fields for common settings.
- Provide copyable MCP endpoint/token guidance without exposing secrets by default.
- Surface exact log paths and diagnostic commands.
- Include OAuth/lab-auth readiness and redirect/public URL validation in the setup/status surface.

CLI UX ergonomics beyond help:

- `axon setup` should be idempotent and bootstrap-oriented.
- Failure messages should say which file to edit and which command to rerun.
- `doctor` should check the production contract, not old optional backends.
- The default command path should favor client/server mode once `AXON_SERVER_URL` exists.
- Commands should never require users to know whether workers are running; the server owns workers.

## Dead Code And Cleanup Candidates

These are candidates, not deletion instructions. Each needs a small verification pass before removal.

| Area | Evidence / reason | Recommended action |
| --- | --- | --- |
| Remote Docker setup deploy | `src/services/setup/deploy.rs`, setup CLI, web panel remote deploy | Keep and harden as Docker Compose orchestration. Remove systemd/binary assumptions and align generated config with local setup. |
| Systemd plugin setup | `scripts/plugin-setup.sh`, `plugins/README.md` | Replace with Docker setup hook. Remove systemd unit generation. |
| OpenAI-compatible first-run surface | `.env.example`, plugin manifest, docs | Remove from production docs/prompts. Keep hidden compatibility only if code still needs it. |
| `AXON_LITE` | Runtime is always SQLite/in-process | Keep parser compatibility, hide from docs/help. |
| Postgres/Redis/RabbitMQ CI services | `.github/workflows/ci.yml` | Remove unless specific legacy tests still require them. |
| `src/core/neo4j.rs` and graph flags | Graph retrieval not wired; MCP rejects graph mode | Remove from production code, CLI, MCP schema, docs, and tests. |
| `src/vector/ops/stats/pg.rs` naming | SQLite-backed but named `pg` / `PostgresMetrics` | Rename to SQLite/job metrics. |
| `src/jobs/migrations/*graph*` / refresh jobs | Tables exist without clear production runner path | Remove graph-specific paths where safe; keep only explicit SQLite migration compatibility if existing user DBs require it. |
| `src/core/config/types/subconfigs.rs` queue config | AMQP-style queue names remain | Remove if unused after code search. |
| `scripts/mcporter-axon` | Exports old `AXON_LITE` / `AXON_PG_URL` | Update or delete. |
| `scripts/reingest.py` | Mentions AMQP hammering | Update or delete. |
| `scripts/time-query-gen`, `scripts/searxng-research`, `scripts/mcp_schema_models.py` | Still use OpenAI vars | Remove from production path or update for Gemini. |
| `Justfile install` | Hard-coded plugin-cache path and systemd restart | Replace with release install/dev install split. |
| `apps/web/out` tracked files | 28 tracked generated files; `src/web/static_assets.rs` uses `rust-embed` with `#[folder = "apps/web/out/"]` | RustEmbed embeds files into the Axon binary at compile time. Keep tracked output only until Docker/CI builds the web export before Rust compilation; then untrack generated assets and make web build a required release step. |

Deferred monolith findings:

- Scanned 358 files.
- Allowlist entries: 20.
- Allowlisted and still oversized: 0.
- Oversized files not covered by allowlist: 7.
- Hard function limit failures: 2.
- Warn-band functions: 51.
- This is not part of the production sprint plan unless it blocks CI or release packaging.

Files over the current size policy and not covered by allowlist:

- `vendor/lab-auth/src/authorize.rs`
- `vendor/lab-auth/src/sqlite.rs`
- `src/services/types/service.rs`
- `vendor/lab-auth/src/google.rs`
- `src/jobs/config_snapshot.rs`
- `src/services/action_api/commands.rs`
- `src/services/llm_backend/headless/gemini.rs`

Function hard-limit failures:

- `vendor/lab-auth/src/middleware.rs:263 authenticate()`
- `src/jobs/ops/enqueue.rs:99 enqueue_job_inner()`

`.monolith-allowlist` state:

- It is tracked.
- It is not ignored.
- Do not change it in this sprint unless CI requires a minimal unblock.
- This was not modified in this information-gathering pass.

Out-of-scope note:

- Splitting all oversized files and removing the allowlist is a valid cleanup goal, but it is not production-critical for this plan. Leave it alone unless it directly blocks CI/release.

## CI And Release Gaps

Current GitHub workflow gaps:

- No workflow builds and publishes the Axon Docker image.
- CI still provisions Postgres/Redis/RabbitMQ in places.
- Advisory lock policy searches old `crates` layout.
- MCP smoke workflow uses CPU TEI and `BAAI/bge-small-en-v1.5`, not Qwen3.
- GPU TEI cannot be fully validated on normal GitHub-hosted runners.
- CI/release must account for RustEmbed: `apps/web/out` has to exist before the Rust binary is compiled, either because CI builds the web app first or because tracked generated assets remain temporarily.

Required workflows:

1. `ci.yml`
   - fmt
   - clippy
   - cargo check/test
   - monolith policy
   - docs/schema/help drift checks
2. `docker-image.yml`
   - build production image
   - push to GHCR
   - tag with SHA, semver, and latest
   - generate SBOM/provenance if desired
3. `compose-smoke.yml`
   - run Docker Compose smoke with CPU-compatible fallback where necessary
   - verify server starts, `/health` or equivalent responds, MCP endpoint responds with expected status
4. Self-hosted GPU smoke, if available
   - RTX 4070
   - Qwen3 TEI
   - prewarmed TEI model cache
   - first crawl
   - first ask
   - measure cold and warm setup timing against the 2-minute target and 5-minute maximum

Release artifacts:

- Host binaries for Linux x86_64 at minimum.
- Checksums.
- Published Docker image.
- Versioned compose file or installer-resolved compose template.
- Plugin bundle that delegates to the shared setup path.

## Sprint Plan

### Phase 0: Freeze Production Contract

Outcome:

- A short committed production contract doc that says Docker Compose, Qdrant, TEI/Qwen3, Chrome, Gemini CLI, shared `~/.axon`, host client plus server.

Acceptance criteria:

- No README/doc page advertises systemd or non-Docker deployment as production setup. SSH orchestration is acceptable only when it deploys the Docker Compose stack.
- OAuth/lab-auth is treated as in-scope for initial production because it is already implemented.
- Graph/Neo4j is treated as out-of-scope and scheduled for removal.

### Phase 1: Config And Env Boundary

Outcome:

- `.env.example` contains only URLs, secrets, runtime/bootstrap settings, and Docker interpolation values.
- `config.example.toml` contains all non-secret tuning knobs.
- Config parser supports every moved setting.

Acceptance criteria:

- A test enumerates `.env.example` keys and classifies each as allowed env.
- A test verifies `config.example.toml` parses.
- README and `docs/guides/configuration.md` describe one clear precedence model.
- `OPENAI_*`, `AXON_LITE`, and PG/Redis/AMQP settings are removed or hidden from production docs; graph settings are removed.

### Phase 2: Docker Compose Setup

Outcome:

- `axon setup` becomes the first-run production setup command.

Acceptance criteria:

- Fresh `~/.axon` can be created idempotently.
- Existing `~/.axon` is refreshed without clobbering secrets.
- Docker Compose stack starts from published image.
- Gemini auth is checked clearly.
- OAuth/lab-auth config is checked clearly.
- TEI Qwen3 model cache is prewarmed.
- `axon doctor` passes after setup.
- First crawl plus ask is run or offered directly.

### Phase 3: One-Line Installer

Outcome:

- One command installs the host client and invokes setup.

Acceptance criteria:

- Installer has no large duplicated logic.
- Installer works on a machine with Docker, NVIDIA runtime, and Gemini auth.
- Installer failure messages point to exact remediation.
- Warm path target is under 2 minutes.
- Cold path must stay under 5 minutes or fail with clear bottleneck reporting.

### Phase 4: Claude Plugin Hook

Outcome:

- Plugin install uses the same setup and config as the one-line installer.

Acceptance criteria:

- No systemd unit is created.
- No plugin-cache binary becomes canonical.
- Plugin hook runs preflight and invokes setup when needed.
- Plugin uses existing `~/.axon/.env` and `~/.axon/config.toml`.
- Claude MCP config points to the same server URL and token.

### Phase 5: CLI Help And UX

Outcome:

- Help output becomes production-oriented and command-specific.

Acceptance criteria:

- `axon --help` lists every supported command.
- `axon setup --help` does not show crawl/ask/vector internals.
- Help does not display current env values.
- Compatibility flags are hidden.
- Help snapshot tests catch regressions.

### Phase 6: Docs Refresh

Outcome:

- README and active docs match the production contract.

Acceptance criteria:

- README is accurate and points to detailed docs.
- `docs/guides/configuration.md`, `docs/DOCKER.md`, `docs/INSTALL.md`, `docs/reference/mcp/overview.md`, `docs/CLI.md`, `docs/TROUBLESHOOTING.md`, and `docs/DEVELOPMENT.md` are source-aligned.
- Active docs inventory is explicit.
- Archive/plans/reports/sessions are marked non-authoritative.
- Stale-doc audit rerun shows no production-blocking drift.

### Phase 7: Cleanup And CI

Outcome:

- Dead code and stale workflows are removed after docs/setup stabilize.

Acceptance criteria:

- CI no longer starts removed infra.
- Docker image publishes on release.
- Existing monolith policy does not block the production release path.
- `cargo machete` or equivalent dependency audit is run and findings are resolved.

## Production Acceptance Criteria

Minimum release checklist:

- Fresh machine with Docker and NVIDIA runtime can run the one-line installer.
- Existing Gemini CLI auth is detected.
- OAuth/lab-auth remains supported and has a verified setup/status path.
- `~/.axon/.env` and `~/.axon/config.toml` are created once and shared by installer/plugin.
- Docker Compose stack starts with Axon, Qdrant, TEI/Qwen3, and Chrome.
- TEI Qwen3 model cache is prewarmed during setup.
- Host `axon doctor` passes.
- Host `axon crawl https://example.com --wait true` succeeds.
- Host `axon ask "What did we crawl?"` succeeds through server mode.
- Claude plugin connects to the same MCP endpoint.
- Web panel shows stack status and first-run progress.
- README first-run instructions match reality.
- No production docs require systemd, non-Docker deployment, Postgres, Redis, RabbitMQ, OpenAI-compatible APIs, or Neo4j graph retrieval. Remote SSH deployment is documented only as Docker Compose orchestration.
- CI publishes the Docker image.
- Release docs explain cold-start image/model download caveats, with under 2 minutes as the target and 5 minutes as the maximum acceptable cold path.

## Resolved Decisions And Follow-Up Notes

- OAuth/lab-auth is in scope for initial production because it is already implemented. The sprint work is to make setup, docs, status checks, and smoke tests reflect the real auth path.
- Remote SSH deployment stays. The important boundary is that it deploys Docker Compose only; it must not install or supervise an Axon binary through systemd.
- Graph/Neo4j should be removed from the production code and docs.
- RustEmbed is the crate currently embedding `apps/web/out/` into the Axon binary at compile time via `src/web/static_assets.rs`. Recommendation: keep tracked `apps/web/out` only until Docker/CI reliably runs the web export before Rust compilation; then untrack generated assets and make the web build part of the release pipeline.
- TEI model cache should be prewarmed by setup.
- Setup timing target remains under 2 minutes; 5 minutes is the absolute maximum acceptable cold path.
- `.monolith-allowlist` cleanup is out of scope for this sprint. Leave it as-is unless it blocks CI/release.
