---
date: 2026-05-16 21:58:05 EST
repo: git@github.com:jmagar/axon.git
branch: refactor/remove-lite-shim-and-env-cleanup
head: 72b01c88
agent: Claude (sonnet-4.6 + opus-4.7)
working directory: /home/jmagar/workspace/axon_rust
pr: 96 — refactor: remove AXON_LITE compat shim + env/config hardening (v2.2.1) — https://github.com/jmagar/axon/pull/96
---

# Docker Compose Streamline + Desktop Palette Polish

## User Request

User asked to clean up the "ridiculous" docker-compose.yaml, starting with deduplicating `axon-chrome` against the `*common-service` anchor and progressing through several rounds of progressively deeper cleanup. Then asked to polish the GPUI desktop palette's UI/UX after seeing a real scrape failure render.

## Session Overview

Two distinct work streams ran end-to-end:

1. **Docker Compose Streamlining (epic axon_rust-1v8a)** — Reduced the axon service `environment:` block from ~22 lines to 5 by leveraging existing config code (`normalize_local_service_url()`), moving container-baked vars to the Dockerfile, and consolidating container detection via a new `running_in_container()` helper.

2. **Desktop Palette Polish** — Three UI polish wins on `apps/desktop` (the GPUI command palette): inline status dot on output title, clean exit-status formatter (replaces ugly `exit code: 0xc000013a`), full-width output lines.

A live P1 security bug (secret spillage to qdrant/tei via the `*common-service` env_file anchor) was discovered mid-session by engineering review and fixed before continuing.

## Sequence of Events

1. User asked to make `axon-chrome` use `*common-service` anchor — done, removed 12 lines of duplication.
2. User asked about merging multiple anchors — explained YAML `<<: [*a, *b]` syntax.
3. User asked whether qdrant should also use GPU — answered no, Qdrant search is CPU-bound; only TEI benefits.
4. User asked about adding `env_file` to anchor — done; introduced `*gpu-service` second anchor for TEI's GPU `deploy.resources`. **Inadvertently introduced secret-spillage bug.**
5. User asked why all environment overrides were necessary — explained each one's purpose (DNS, host-bleed guards, port).
6. User asked to move container-only vars into the Dockerfile — moved `AXON_HOME`, `AXON_IN_CONTAINER`, `AXON_MCP_HTTP_HOST`, `CLICOLOR_FORCE` to Dockerfile ENV.
7. User asked to research code and create a plan via `/lavra-plan` — produced epic `axon_rust-1v8a` with 4 children.
8. User asked about TZ — folded TZ into bead 1v8a.1, made epic target an empty `environment:` block.
9. User invoked `/lavra-eng-review` — discovered the **live P1 secret spillage** plus several silent-failure mode warnings on beads 1v8a.2/1v8a.3.
10. User asked to apply all feedback — closed beads 1v8a.2/1v8a.3 as deferred, narrowed 1v8a.1, added 1v8a.5 (P1 secret fix) and 1v8a.6 (running_in_container helper).
11. User asked to explain the secret leak — explained `env_file` on `*common-service` gave qdrant/tei full access to MCP/Google/Reddit/GitHub/Tavily/HF secrets.
12. User said "k.." — fixed the secret spillage immediately by moving `env_file` onto axon service only.
13. User invoked `/lavra-work axon_rust-1v8a` — executed wave 1 (1v8a.1 + 1v8a.6) and wave 2 (1v8a.4) with 3 contract test fixes including a host-bleed `AXON_DATA_DIR` issue caught after deploy.
14. User ran `axon crawl` against deployed stack, got server-mode error — investigated, found container was crashlooping due to **second host-bleed: `AXON_DATA_DIR` from .env was overriding Dockerfile ENV inside the container**.
15. Fixed `AXON_DATA_DIR` override in compose, added contract test assertion.
16. Container hit second issue: SQLite migration mismatch (registry image v2.0.0 vs jobs.db from v2.2.1) — rebuilt local image, set `AXON_IMAGE=ghcr.io/jmagar/axon:local` in `~/.axon/.env`, container came up healthy.
17. User shared screenshot of palette showing "Scrape URL failed" with `exit code: 0xc000013a`, asked for UI/UX polish.
18. Shipped three palette polish improvements: title status dot, exit-status formatter, full-width output lines.

## Key Findings

- **`normalize_local_service_url()` at `src/core/config/parse/docker.rs:31-61`** already transparently rewrites Docker DNS names to `127.0.0.1:PORT` when `/.dockerenv` is absent. CLAUDE.md documents this: ".env can safely use container-internal DNS." This eliminated 3 service-URL overrides without code changes.
- **Three container detection dialects in codebase before consolidation:**
  - `docker.rs` used `*IN_DOCKER` LazyLock (filesystem only, no test seam)
  - `panel_stack.rs` used dual `AXON_IN_CONTAINER` + `/.dockerenv`
  - `config_snapshot/errors.rs` used `AXON_IN_CONTAINER` only
- **Test `env_example_host_urls_are_loopback_not_container_dns` at `src/services/setup/local/env_tests.rs:98`** explicitly blocks any change to `.env.example` URLs — this is the reason `.env.example` must stay with `127.0.0.1` URLs even though `~/.axon/.env` can use Docker DNS.
- **Compose env_file values override Dockerfile ENV.** This is the root mechanism behind the `AXON_DATA_DIR` host-bleed: even with `AXON_DATA_DIR=/home/axon/.axon` baked into the Dockerfile, the host's `~/.axon/.env` value (`/home/jmagar/.axon`) wins because env_file is loaded later.
- **`std::process::ExitStatus::Display`** can produce ugly output like `exit code: 0xc000013a` for signal-killed processes; replaced with custom formatter producing `ok` / `exit N` / `killed by SIGKILL (9)`.

## Technical Decisions

- **Defer beads 1v8a.2 + 1v8a.3 (Rust container-aware guards)** — engineering review found 3 unrescued silent failure modes (sidecar deployments routed wrong, Podman containers not protected, operator overrides silently ignored). Compose blanks (`AXON_CONFIG_PATH: ""`, `AXON_SERVER_URL: ""`) are explicit and debuggable; Rust guards are invisible. Kept compose blanks.
- **`env_file` only on `axon` service, never on infra services** — third-party containers (`qdrant/qdrant`, `ghcr.io/huggingface/text-embeddings-inference`) must not see MCP/OAuth/API secrets even if an attacker RCEs them.
- **Keep `AXON_HOME` and `AXON_DATA_DIR` in compose environment block** — these are host-bleed vars that `env_file` would override the Dockerfile ENV for. Both are documented in the contract test.
- **Reject `.env.example` Docker DNS rewrite** — `env_example_host_urls_are_loopback_not_container_dns` test enforces the host-facing template uses `127.0.0.1`. Only `~/.axon/.env` (user's personal file) was updated.
- **`AXON_IN_CONTAINER` env var checked first in `running_in_container()`** — Dockerfile-baked, testable without filesystem mocking. Falls through to `/.dockerenv` (Docker) then `/run/.containerenv` (Podman rootless).

## Files Modified

| File | Purpose |
|---|---|
| `docker-compose.yaml` | Removed `env_file` from `*common-service`, added to `axon` service only; stripped 3 service URL overrides + `AXON_MCP_HTTP_PORT`; kept 5 explicit-policy overrides; added `*gpu-service` anchor |
| `config/Dockerfile` | Added `AXON_HOME`, `AXON_IN_CONTAINER=1`, `AXON_MCP_HTTP_HOST=0.0.0.0`, `CLICOLOR_FORCE=1` to ENV block |
| `src/core/config/parse/docker.rs` | Replaced `IN_DOCKER` LazyLock with `pub(crate) fn running_in_container()` checking `AXON_IN_CONTAINER` → `/.dockerenv` → `/run/.containerenv` |
| `src/core/config/parse/docker_tests.rs` | New sidecar with 4 tests (one ignored for CI-in-Docker) |
| `src/jobs/lite/config_snapshot/errors.rs` | Delegate `running_in_container()` to `docker.rs` |
| `src/web/panel_stack.rs` | Migrate `StackRuntimeMode::detect()` to use `docker::running_in_container()` |
| `tests/compose_env_contract.rs` | Added `AXON_DATA_DIR` assertion; updated allowlist (added GPU vars and new keys; removed GOOGLE_API_KEY/GOOGLE_APPLICATION_CREDENTIALS) |
| `README.md` | Bumped version 2.1.0 → 2.2.1 |
| `~/.axon/.env` (outside repo) | Service URLs to Docker DNS names; added `AXON_IMAGE=ghcr.io/jmagar/axon:local` |
| `apps/desktop/src/output.rs` | Added `format_exit_status()` replacing raw `ExitStatus::Display` |
| `apps/desktop/src/render.rs` | Inline status dot on output title; full-width output body lines |
| `apps/desktop/src/ui.rs` | Pre-existing `axon_command()` Windows console-hide helper (committed with this batch) |

## Commands Executed

| Command | Result |
|---|---|
| `cargo test docker --locked` | 11 passed, 1 ignored |
| `cargo test --test compose_env_contract` | After fixes: 12 passed |
| `docker build -t ghcr.io/jmagar/axon:local -f config/Dockerfile .` | Exit 0 |
| `docker compose up -d axon --force-recreate` | Container healthy on `0.0.0.0:8001` |
| `bd swarm validate axon_rust-1v8a` | 2 waves, max parallelism 3, no cycles |

## Errors Encountered

- **Initial container crashloop after wave 2** — `data_dir=/home/jmagar/.axon` (host path) leaked into container. Root cause: `env_file` values override Dockerfile ENV in compose precedence. Fix: added `AXON_DATA_DIR: /home/axon/.axon` to compose environment block.
- **Container crashloop with migration mismatch** — Registry image `:latest` was v2.0.0 but local jobs.db was v2.2.1. Fix: built local image and set `AXON_IMAGE` in `~/.axon/.env`.
- **rustfmt pre-commit fail on docker_tests.rs** — Ran `cargo fmt` and re-committed.
- **`cargo check -p axon-desktop` failed** — Desktop is a standalone workspace, not part of root workspace. Ran from `apps/desktop/` directly.

## Behavior Changes (Before/After)

| Surface | Before | After |
|---|---|---|
| Compose env block size | ~22 lines on axon service | 5 lines (AXON_HOME, AXON_DATA_DIR, AXON_SERVER_URL, AXON_ENV_FILE, AXON_CONFIG_PATH, TZ) |
| Secret exposure | qdrant + tei + chrome inherited all secrets via env_file anchor | Only axon service sees secrets |
| Container detection | 3 inconsistent code patterns | 1 helper (`running_in_container()`) covering Docker + Podman + explicit env var |
| Host CLI service URLs | `~/.axon/.env` had `127.0.0.1` URLs | `~/.axon/.env` uses Docker DNS names; host CLI rewrites transparently |
| Palette: failed scrape title | Plain text "Scrape URL failed" | Pink dot + "Scrape URL failed" — error obvious at first glance |
| Palette: exit status | `exit code: 0xc000013a` (raw ExitStatus on some signal exits) | `ok` / `exit N` / `killed by SIGKILL (9)` |

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo test --test compose_env_contract` | all pass | 12 passed | ✓ |
| `cargo test docker --locked` | all pass | 11 passed, 1 ignored | ✓ |
| `cargo check` (apps/desktop) | no errors | only dead_code warnings on unused theme constants | ✓ |
| `docker compose ps axon` after rebuild | Up healthy | Up 11 seconds (healthy) on 0.0.0.0:8001 | ✓ |
| `grep AXON_HOME ~/.axon/.env` after fix | host path | `/home/jmagar/.axon` | ✓ |
| `data_dir` in container logs after compose fix | `/home/axon/.axon` | `/home/axon/.axon` | ✓ |

## Risks and Rollback

- **Removing compose overrides risks host-bleed for any var the .env sets to a host path.** We've covered `AXON_HOME` and `AXON_DATA_DIR` explicitly; other vars like `AXON_LOG_PATH`, `AXON_HEADLESS_GEMINI_HOME` could leak if users set them. Rollback: re-add the `environment:` line for the affected var.
- **`running_in_container()` semantics changed from "Docker only" to "Docker + Podman + env var".** Any code path that depended on Docker-specific detection (none currently exist) would now also fire on Podman. Rollback: revert to LazyLock with `/.dockerenv` only.
- **Registry `:latest` image still v2.0.0.** Users who run `docker compose pull` without local builds will hit the migration mismatch. Mitigation: push v2.2.1 image to ghcr.io or pin AXON_IMAGE in .env. Already documented in user's local `.env`.

## Decisions Not Taken

- **Did not add `IN_DOCKER` guards to `resolve_config_path()` and `resolve_server_url()`** (deferred beads 1v8a.2/1v8a.3). Engineering review flagged 3 unrescued silent failure modes (sidecar misrouting, Podman not protected, operator override silently ignored). Compose blanks are explicit and debuggable.
- **Did not change `.env.example`** to use Docker DNS names. Blocked by `env_example_host_urls_are_loopback_not_container_dns` test which is intentional policy.
- **Did not add Docker DNS rewriter for `axon` service itself** (host map only covers qdrant/tei/chrome). Would need to handle `AXON_SERVER_URL` separately if user wants to route host CLI to container axon serve via DNS name.
- **Did not migrate `panel_stack.rs::StackRuntimeMode::detect()` to a single-purpose helper** — left existing dual-check pattern in favor of delegating to `running_in_container()`. Behavior identical; code shorter.

## References

- Beads: axon_rust-1v8a, axon_rust-1v8a.1, axon_rust-1v8a.4, axon_rust-1v8a.5, axon_rust-1v8a.6 (closed); 1v8a.2, 1v8a.3 (deferred)
- PR: https://github.com/jmagar/axon/pull/96
- Test that blocked .env.example change: `src/services/setup/local/env_tests.rs:98` `env_example_host_urls_are_loopback_not_container_dns`
- Contract test: `tests/compose_env_contract.rs:9-49` `services_compose_reads_canonical_axon_home_env`

## Open Questions

- Does the `0xc000013a` exit code on the user's screenshot indicate a real signal-killed process or some Windows-side rendering quirk? Not investigated — the new `format_exit_status()` will make it readable regardless of root cause.
- Should the registry `:latest` image be rebuilt and pushed to ghcr.io to prevent migration mismatch for fresh users? Not handled this session.
- Are there other host-bleed env vars in `~/.axon/.env` that would cause container issues if `~/.axon/.env` has them set (e.g. `AXON_LOG_PATH`)? Spot-checked — current `~/.axon/.env` does not set these, but unbounded.

## Next Steps

**Started but not completed:**
- None — all 4 active beads (1v8a.1, 1v8a.4, 1v8a.5, 1v8a.6) are closed and pushed.

**Follow-on tasks not yet started:**
- Build and push v2.2.1 image to `ghcr.io/jmagar/axon:latest` so the default compose works without a local build.
- Consider whether `panel_stack.rs::StackRuntimeMode::detect()` should be simplified now that it just calls `running_in_container()`.
- Audit `~/.axon/.env` for any other host-path env vars that might bleed into container; add explicit overrides if found.
- The desktop palette has uncommitted changes to `apps/desktop/src/markdown.rs` (per session-state dirty file list) — review and either commit or stash.
