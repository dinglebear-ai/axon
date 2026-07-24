## Goal
Full rewrite of `README.md` for the post-unification runtime, with deployment reframed around **Incus (preferred) and bare-metal systemd (supported) for the axon binary itself** — containerized Qdrant/TEI/Chrome infra is fine to mention. Plus a committed, portable systemd unit template lifted from the production-validated Incus heredoc, and a fix to the CLI markdown generator so the README can defer CLI detail to a generated artifact instead of hand-maintaining a command map (which is what drifted stale).

## Step 1 — Fill the stubbed CLI markdown renderer (makes "generate, don't hand-roll" real)

The user asked "can we generate the CLI map instead of hand-rolling?" — the right instinct, since the hand-maintained map is exactly what went stale. Today `docs/reference/cli/commands.json` is live and authoritative (110 commands, 49 groups, drift-checked against clap), but `docs/reference/cli/commands.md` is a **stub**: `xtask/src/schemas/families/markdown.rs` (`markdown()` / `registry_markdown()`) never iterates command records.

- Add a renderer in `xtask/src/schemas/families/markdown.rs` that iterates `command_records()` into a grouped table (group → command path → summary), mirroring the existing `api_markdown()` pattern. Keep functions under the 80/120-line monolith limits.
- Verify the mislabeled header (`commands.md` claims `cargo xtask docs generate --family cli`; real command is `cargo xtask schemas cli`) and correct it if it's a generator constant.
- Run `cargo xtask schemas cli` to regenerate `commands.md`. Confirm `git diff` shows `commands.md` going from ~68 stub lines to a populated table, with `commands.json` unchanged.

## Step 2 — Commit a portable bare-metal systemd unit (new asset)

Lift the production-validated unit body from `deploy/incus/bootstrap.sh:335-355` into a reusable template, de-Incused for a bare host.

Create `deploy/systemd/axon.service`:
```ini
[Unit]
Description=Axon unified server (native binary, bare-metal)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=/etc/axon/axon.env
Environment=AXON_HOME=/var/lib/axon
Environment=AXON_DATA_DIR=/var/lib/axon
WorkingDirectory=/var/lib/axon
ExecStart=/usr/local/bin/axon serve
Restart=always
RestartSec=5
User=axon

[Install]
WantedBy=multi-user.target
```
Differences from the Incus unit (all intentional for bare-metal): drop `After=docker.service` (axon doesn't depend on Docker); `/var/lib/axon` + `/etc/axon/axon.env` instead of `/mnt/axon-data`; dedicated `axon` user instead of root; no forced `AXON_HTTP_HOST=0.0.0.0` (loopback by default; operators open it via reverse proxy or override in `axon.env`).

Create `deploy/systemd/README.md` — short install walkthrough: create `axon` user, install binary to `/usr/local/bin/axon`, create `/var/lib/axon` + `/etc/axon/axon.env`, drop in the unit, `systemctl enable --now axon`. Note that Qdrant/TEI/Chrome typically run as containers (point at `docker-compose.prod.yaml` for canonical image versions/ports) or on external hosts; axon reaches them via `QDRANT_URL`/`TEI_URL`/`AXON_CHROME_REMOTE_URL`.

## Step 3 — Rewrite `README.md` (full coverage, fresh, deployment reframed)

Replace the file entirely. Structure, top to bottom:

1. **Title + version (7.1.4) + pitch** — self-hosted RAG stack; one unified source pipeline; **deployed as a native binary under systemd (Incus container or bare metal)**; backed by Qdrant, TEI (Qwen3), Chrome.
2. **The unified source pipeline** — `SourceRequest → resolve → route → acquire → ledger/manifest → prepare → embed → publish → graph → cleanup`; one `job_id` across all stages. Link `docs/pipeline-unification/` as the historical design-contract packet (NOT future work — per "drop future framing").
3. **Deployment** (replaces "Production Contract" + "Docker Stack", reframed):
   - **Supported paths for the axon binary:** Incus system container (preferred — see `deploy/incus/`), bare-metal systemd (supported — see `deploy/systemd/`). Both run `/usr/local/bin/axon serve` under a systemd unit.
   - **Infrastructure (Qdrant/TEI/Chrome):** typically containerized; the Incus bootstrap runs them as nested containers, bare-metal operators can use the infra portions of `docker-compose.prod.yaml` or run them externally. Point at `deploy/incus/README.md` for the split-model rationale.
   - **Not supported as an axon deployment path:** running the axon binary itself in a Docker container as the production deployment. (Container images are still published to GHCR for users who want them, and `docker-compose.prod.yaml` remains the canonical infra reference.)
   - **Not supported at all:** Postgres/Redis/RabbitMQ/AMQP, Neo4j graph retrieval, multiple competing `.env`/`config.toml` locations.
   - Target hardware: NVIDIA RTX 4070 + NVIDIA Container Toolkit (for TEI GPU).
4. **Install the binary** — Linux one-liner (`install.sh` → `~/.local/bin/axon`) + Windows (`install.ps1` → `%USERPROFILE%\.local\bin`); `AXON_INSTALL_*` controls; Claude plugin install note. Binary-only — deployment is separate (Step 3 above).
5. **Deploy** — two subsections:
   - **Incus (preferred):** `deploy/incus/bootstrap.sh` brings up the system container, builds axon from the repo's runtime stage, installs `axon-native.service`, brings up Qdrant/TEI/Chrome as nested containers, optional `mcp-publish` proxy device. Pointer to `deploy/incus/README.md` for profile/storage/GPU details.
   - **Bare-metal systemd:** install binary, create `axon` user + `/var/lib/axon` + `/etc/axon/axon.env`, drop in `deploy/systemd/axon.service`, `systemctl enable --now axon`. Pointer to `deploy/systemd/README.md`.
   - Both: `axon serve` hosts HTTP (`127.0.0.1:8001`), MCP (`/mcp`), web panel, and in-process workers in one process.
6. **Setup and configuration home** — `axon setup init` creates `~/.axon` (config.toml, .env, data dir); two-layer config (`.env` URLs/secrets/bootstrap, `config.toml` tuning); precedence chain; flat `~/.axon/` layout tree; `AXON_DATA_DIR` default + `~/.local/share/axon` migration note. Note that `axon setup`'s Compose-asset writing is for the infra-containerization convenience, not the axon deployment path. Strip the old "compose up is provisioning" framing.
7. **Quick start** — canonical post-unification flow:
   ```bash
   axon doctor
   axon https://example.com --scope site --wait true
   axon query "provider cooling" --content-kind code
   axon ask "What did we index?"
   ```
8. **Sources and scopes** — `axon <source> --scope page|site|docs|repo|package|subreddit`; adapter families (web, local, github/gitlab/gitea/git, crates/npm/pypi/docker, reddit, youtube, feed, sessions, cli_tool, mcp_tool); `scrape` as the one-page projection; `map` for discovery; `--refresh`/`--watch`/`--no-embed`/`--wait`.
9. **Jobs, watches, and cleanup** — one durable job model; `--wait true` foreground vs detached (workers must run — `axon serve` hosts them); `axon jobs list/get/events/stream/cancel/retry/recover/cleanup/clear/worker`; `axon watch create/.../history`; `axon prune plan` + `axon prune exec --confirm`; `axon reset plan/exec`. Consolidates "detached work needs workers" and "cleanup is plan-first" gotchas.
10. **Retrieval and analysis** — `query`, `retrieve`, `ask`, `summarize`, `evaluate`, `train`, `suggest`, `search`, `research`, `extract` (+ lifecycle), `brand`, `diff`, `endpoints`, `screenshot`, `chat`.
11. **Memory** — `axon memory remember/list/search/show/link/supersede/context`; decay model + dedicated Qdrant collection.
12. **CLI** — short curated headline command list by group, then: "for the authoritative, always-current full command registry, see the generated `docs/reference/cli/commands.md` (valid because of Step 1) and `commands.json`." `axon --help` / `axon <cmd> --help` note.
13. **MCP** — `axon mcp` (stdio default, `--transport http|both`, `axon serve mcp`); single `axon` tool; `action`/`subaction` routing; example calls; response modes; removed legacy actions rejected → use `source`/`prune`.
14. **HTTP API and surfaces** — brief surfaces section per user's earlier answer: `axon serve` (Axum on `127.0.0.1:8001`), `/v1/*` REST, `/mcp`, OpenAPI at `/docs`, Aurora control panel. One-line each for companion apps (Palette Tauri, Android, Chrome extension) with pointers to `apps/*/README.md`.
15. **Configuration reference** — pointer to `config.example.toml` (tuning) and `.env.example` (URLs/secrets); LLM backends (`gemini-headless`/`openai-compat`/`codex-app-server`); search (SearXNG/Tavily); adapter creds (GitHub/GitLab/Gitea/Reddit).
16. **Development** — `cargo build --bin axon`; test/lint; `just verify/fix/precommit`; module layout (no `mod.rs`); `_tests.rs` sidecar convention; monolith policy. `docker-compose.yaml` dev stack mentioned here (runs debug binary + infra for local dev) — this is dev, not axon deployment.
17. **Release gates** — refreshed; `cli` manual-bump, others release-please-managed; `cargo xtask check-release-versions` PR gate.
18. **Troubleshooting** — `preflight`/`doctor`/`systemctl status axon`/`journalctl -u axon`; for Incus, `incus exec axon -- systemctl status axon-native` / `journalctl`; important paths under `~/.axon` (bare-metal) and `~/.axon-incus` (Incus); common failures. Strip removed-command references.
19. **Related files / related servers / License** — keep, refresh to include `deploy/incus/`, `deploy/systemd/`, generated registries.

## Verification (scoped, per AGENTS.md guard)

This change is prose README + dev-only `xtask` renderer + two new `deploy/systemd/` files. `xtask` and `deploy/` are NOT in any component's shipping paths, so no version bump and no release gate. Smallest checks that prove the change:

- `cargo check -p xtask` — compiles the new renderer.
- `cargo xtask schemas cli` — regenerates `commands.md`; proves the renderer works.
- `git diff --stat docs/reference/cli/` — confirms `commands.md` populated, `commands.json` unchanged.
- `cargo test -p xtask` — runs the `markdown_and_json_drift_together` drift test.
- README structural check: confirm no remaining references to removed commands (`crawl`/`embed`/`ingest`/`fresh`/`dedupe`/`refresh`/`code-search-watch`/`purge`) outside a "removed" context, and that axon deployment is framed as Incus/systemd (not "Docker Compose first").
- `systemd-analyze verify deploy/systemd/axon.service` — confirms the new unit parses.

NOT running full `cargo test --workspace` — overkill for docs + dev-tooling + a unit template; the guard explicitly discourages it. No Rust runtime code changes.

## Out of scope
- No changes to runtime crates, schemas, migrations, or shipping paths.
- No CLI/MCP/REST contract changes. No version bump.
- `deploy/incus/bootstrap.sh` is left as-is (it works); only the new `deploy/systemd/` template is added.
- `docker-compose.prod.yaml`, `axon compose` CLI commands, and AGENTS.md's Docker section are left intact — they remain the canonical infra reference and dev convenience; only the README's *axon deployment* framing changes.