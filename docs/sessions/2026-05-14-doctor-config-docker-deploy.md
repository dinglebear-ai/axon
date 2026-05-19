---
date: 2026-05-14 00:04:51 EDT
repo: git@github.com:jmagar/axon.git
branch: main
head: 321fc6f2
agent: Codex
session id: unavailable
transcript: unavailable - no matching ~/.claude/projects/-home-jmagar-workspace-axon_rust/*.jsonl file was found
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust  321fc6f2 [main]
---

# Doctor, Config Docs, and Docker Deploy Session

## User Request

Dispatch parallel agents to fix `doctor` so it does not complain about OpenAI when OpenAI is not used, review stale documentation from the `.env` and `config.toml` migration, and deploy the latest code via Docker.

## Session Overview

- Dispatched three parallel agents for the doctor behavior fix, docs review, and Docker deployment.
- Integrated the doctor fix so OpenAI diagnostics are omitted unless OpenAI-compatible configuration is actually present.
- Updated stale documentation references around Gemini headless runtime, OpenAI compatibility, MCP auth, and the env/config boundary.
- Built and deployed the latest Docker image locally from `main` at `321fc6f2`.
- Verified the live container reports `all_ok: true` without an OpenAI service block.

## Sequence of Events

1. Started from `main` after the env/config boundary migration and unified web/MCP port merge had already landed.
2. Dispatched the requested agents:
   - one worker for doctor behavior,
   - one explorer for documentation staleness,
   - one worker for Docker deployment.
3. Applied and pushed the doctor/doc changes in focused commits.
4. Initial Docker deploy showed the app container healthy, but in-container `axon doctor --json` still failed Qdrant, TEI, and Chrome checks because doctor was still using SSRF-guarded probe helpers for internal service DNS.
5. Split doctor probing so internal health checks use `internal_service_http_client()` and local helper probes.
6. The pre-commit monolith hook rejected an intermediate version because `build()` in `src/core/health/doctor/lite.rs` reached 128 lines.
7. Refactored probe collection into helper functions, passed hooks, committed, pushed, rebuilt Docker, and recreated the `axon` service.
8. Verified local Docker health and GitHub Actions state.

## Key Findings

- OpenAI should be treated as optional in doctor output; if `OPENAI_BASE_URL` and a usable model are not configured, the OpenAI service block should not appear.
- Gemini headless is the active local LLM path for ask/evaluate/suggest/extract fallback/debug/research synthesis and should be surfaced separately in doctor output.
- In-container service DNS such as `axon-qdrant`, `axon-tei`, and `axon-chrome` is expected runtime behavior, so doctor internal service probes must not go through the public SSRF resolver path.
- `probe_http()` was not suitable for internal Docker service checks because it uses the external guard path; curl inside the container proved Qdrant, TEI, and Chrome were reachable.
- The Docker image workflow for `321fc6f2` completed successfully, while `.github/workflows/ci.yml` still fails immediately as a separate workflow.

## Technical Decisions

- Kept OpenAI diagnostics conditional rather than reporting OpenAI as a failed service when the runtime is Gemini-backed.
- Kept public URL fetch protections intact and added internal doctor-specific probe helpers for trusted service URLs.
- Refactored `src/core/health/doctor/lite.rs` after the monolith hook failure instead of allowlisting or bypassing the check.
- Recreated only the `axon` container after rebuilding the app image, leaving Qdrant, TEI, and Chrome running.

## Files Modified

- `src/core/health/doctor.rs`: removed the old Chrome probe helper that routed through the generic public HTTP probe path.
- `src/core/health/doctor/lite.rs`: added conditional OpenAI diagnostics, Gemini reporting support, internal service probe helpers, and a refactor into `ServiceProbes`.
- `src/cli/commands/doctor/render.rs`: made doctor render Gemini and optional OpenAI status correctly.
- `docs/CONFIG.md`: updated env/config boundary documentation.
- `docs/DEPLOYMENT.md`: updated runtime and deployment configuration notes.
- `docs/MCP-TOOL-SCHEMA.md`: updated MCP/schema config references.
- `docs/MCP.md`: updated MCP configuration/auth references.
- `docs/PERFORMANCE.md`: updated performance/config references.
- `docs/auth/API-TOKEN.md`: updated MCP auth/config references.
- `docs/commands/ask.md`: updated LLM runtime wording.
- `docs/mcp/TOOLS.md`: updated MCP tool/runtime documentation.

## Commands Executed

- `cargo fmt --check`: passed after formatting.
- `cargo test core::health::doctor::lite::tests --lib`: passed, 3 doctor tests.
- `cargo check --bin axon`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `python3 scripts/check-env-config-boundary.py`: passed earlier in the doc/config alignment pass with 193 classified keys.
- `python3 scripts/enforce_monoliths.py --staged`: passed after refactoring `build()`.
- `git commit -m "fix: probe internal doctor services without ssrf resolver"`: passed the pre-commit suite.
- `git push origin main`: pushed `321fc6f2` to `origin/main`.
- `docker build -f config/Dockerfile -t ghcr.io/jmagar/axon:latest -t axon:local-321fc6f2 .`: built the release image successfully.
- `docker compose --env-file /home/jmagar/.axon/.env -f docker-compose.yaml up -d --no-deps --force-recreate axon`: recreated the app container.
- `docker exec axon axon doctor --json`: returned `all_ok: true` with no OpenAI service block.
- `bd dolt push`: completed successfully.

## Errors Encountered

- The first post-change commit attempt failed because the monolith policy found `src/core/health/doctor/lite.rs:16 build()` at 128 lines with a hard limit of 120. The fix was to extract service probe collection into helper functions.
- A first in-container doctor summary command attempted to use `jq` inside the Docker container, where `jq` is not installed. The command also broke the stdout pipe. The verification was rerun by redirecting JSON to the host and using host-side `jq`.
- The first Docker redeploy after `8fc8a1b7` still reported Qdrant, TEI, and Chrome as failed in doctor because internal service probes still used the SSRF-guarded helper path. The final fix replaced those probes with internal-client helper functions.

## Behavior Changes (Before/After)

| Area | Before | After |
| --- | --- | --- |
| Doctor OpenAI output | OpenAI could appear as failed even when OpenAI was not part of the configured runtime. | OpenAI is omitted unless OpenAI-compatible base URL and model configuration are present. |
| Doctor Gemini output | Gemini headless runtime was not the clear LLM readiness signal. | `gemini_headless` is shown and used for extract LLM readiness. |
| Docker doctor checks | In-container Qdrant, TEI, and Chrome probes could fail despite working Docker networking. | Internal service checks use the internal HTTP client and report all services healthy. |
| Docs | Some docs still described old OpenAI-centric or stale env/config behavior. | Docs describe the current Gemini/headless and env/config split more accurately. |

## Verification Evidence

| Command | Expected | Actual | Status |
| --- | --- | --- | --- |
| `cargo fmt --check` | No formatting diff | Passed | Passed |
| `cargo test core::health::doctor::lite::tests --lib` | Doctor unit tests pass | 3 passed, 0 failed | Passed |
| `cargo check --bin axon` | Binary typechecks | Passed | Passed |
| `cargo clippy --all-targets -- -D warnings` | No clippy warnings | Passed | Passed |
| `python3 scripts/enforce_monoliths.py --staged` | Monolith policy passes | Passed after refactor | Passed |
| `docker build -f config/Dockerfile -t ghcr.io/jmagar/axon:latest -t axon:local-321fc6f2 .` | Image builds | Release build completed and image tagged | Passed |
| `curl -fsS -i http://127.0.0.1:8001/healthz` | HTTP 200 | `HTTP/1.1 200 OK` with body `ok` | Passed |
| `docker exec axon axon doctor --json` | Runtime services healthy, no OpenAI block | `all_ok: true`, `has_openai: false`, Gemini/Qdrant/TEI/Chrome/SQLite all true | Passed |
| `docker inspect axon --format ...` | Running and healthy | Container `b93516e37938...`, image `sha256:f42c22165122...`, `running`, `healthy` | Passed |
| `docker compose --env-file /home/jmagar/.axon/.env -f docker-compose.yaml ps` | Axon stack healthy | `axon`, `axon-qdrant`, `axon-tei`, and `axon-chrome` healthy | Passed |
| `gh run list --branch main --limit 5 ...` | Docker workflow status visible | Docker image workflow for `321fc6f2` completed with `success` | Passed |

## Risks and Rollback

- Risk: doctor now uses internal probe helpers for trusted runtime services; this is correct for internal health checks but should not be reused for arbitrary user-provided fetch paths.
- Risk: `.github/workflows/ci.yml` still fails immediately on `main`; this session did not investigate or fix that workflow.
- Rollback: revert `321fc6f2`, `8fc8a1b7`, and `932eb2b9` if the doctor/doc behavior needs to be restored, then rebuild and recreate the Docker app container.

## Decisions Not Taken

- Did not bypass the monolith pre-commit failure; the code was refactored instead.
- Did not install `jq` into the runtime container just for verification; host-side JSON parsing was enough.
- Did not modify `.github/workflows/ci.yml` because the request was scoped to doctor behavior, docs staleness, and Docker deployment, and the workflow failure pre-existed this final doctor fix.

## References

- GitHub Docker image workflow for `321fc6f2`: https://github.com/jmagar/axon/actions/runs/25834403469
- GitHub CI workflow failure for `321fc6f2`: https://github.com/jmagar/axon/actions/runs/25834403156
- Local Docker image tag: `axon:local-321fc6f2`
- Local Docker image digest: `sha256:f42c22165122aaefef5433f8a0646cd077987db0a9db066d4db58c6d91b57e47`

## Open Questions

- Why `.github/workflows/ci.yml` fails immediately with no useful job execution remains unresolved.
- The exact Codex transcript path was not available via the `save-to-md` skill's Claude transcript lookup path.

## Next Steps

Started but not completed:

- None.

Follow-on tasks not yet started:

- Investigate and fix the failing `.github/workflows/ci.yml` workflow on `main`.
- Decide whether the Docker Compose network warning should be cleaned up by marking the pre-existing `axon` network as external in compose configuration.
