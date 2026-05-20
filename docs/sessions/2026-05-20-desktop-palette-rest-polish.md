---
date: 2026-05-20 18:11:06 EST
repo: git@github.com:jmagar/axon.git
branch: main
head: af13c72a
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust                                             af13c72a [main]
---

# Desktop Palette REST Polish

## User Request

Continue polishing and refining the Axon desktop app until Axon operations are fully operational and render cleanly. The session also carried forward earlier direction to use the REST API instead of `/v1/actions`, add a Zed-like menubar/settings surface, and keep the palette usable when resized or expanded.

## Session Overview

- Expanded the desktop palette command surface to cover the main REST-backed Axon operations.
- Converted REST JSON responses into readable palette output summaries instead of dumping raw payloads.
- Improved palette scrolling, optional argument labeling, and footer overflow behavior.
- Fixed the desktop REST client to read `AXON_SERVER_URL` and `AXON_MCP_HTTP_TOKEN` from `~/.axon/.env` directly when the app is not launched from a sourced shell.
- Verified the focused desktop crate with tests, release build, monolith checks, whitespace checks, and live REST probes.

## Sequence of Events

1. Reviewed the existing desktop changes and confirmed the worktree already had broad unrelated changes outside the current focus.
2. Added new desktop command actions for core REST operations and wired request construction for each route.
3. Added REST response formatting for ask, scrape, summarize, retrieve, research, query, search, map, suggest, evaluate, job-start responses, sources, domains, stats, doctor, and status.
4. Added action-list scrolling and UI polish for optional arguments and long footer text.
5. Ran focused tests and fixed compile/build issues as they surfaced.
6. Probed the live Axon server with bearer auth from the local config without printing the token.
7. Discovered that sourcing `~/.axon/.env` directly is unsafe because `NVIDIA_REQUIRE_CUDA=cuda>=12.2` is not shell-safe; updated the palette REST client to parse the file directly instead of requiring shell sourcing.

## Key Findings

- `src/web/server/routing.rs` exposes the REST route surface used by the palette: discovery routes, RAG routes, exploration routes, async job routes, admin routes, and watch routes.
- `/v1/evaluate` is mounted in the active server router even though the older `handlers/rest/sync_post.rs` comments describe it as intentionally absent from that separate sub-router.
- `~/.axon/.env` contains shell-unsafe values such as `NVIDIA_REQUIRE_CUDA=cuda>=12.2`; direct parsing is safer for the desktop app than sourcing.
- The app process cannot rely on inheriting `AXON_SERVER_URL` and `AXON_MCP_HTTP_TOKEN` from an interactive shell, so the desktop REST client now falls back to `~/.axon/.env`.

## Technical Decisions

- Kept the palette on REST routes instead of `/v1/actions`.
- Treated dangerous/admin operations conservatively: route/request wiring and validation probes were used, not destructive live operations.
- Rendered REST output as command-specific summaries to keep the UI readable and avoid raw JSON dumps.
- Parsed `.env` in the REST client with a small local parser to avoid shell execution and support shell-unsafe but valid dotenv-style values.
- Left the existing warning-level `ui_commands.rs::submit()` monolith warning untouched because it is under the hard limit and outside the immediate polish scope.

## Files Modified

- `apps/desktop/src/actions.rs`: added REST-backed palette actions and optional argument mode.
- `apps/desktop/src/rest_client.rs`: added REST request builders for new operations and `.env` fallback parsing.
- `apps/desktop/src/rest_client_tests.rs`: added request-builder tests and `.env` parsing coverage.
- `apps/desktop/src/output.rs`: routed REST output through command-specific formatting and scoped legacy process-output helpers to tests.
- `apps/desktop/src/output/formatting.rs`: added readable REST JSON formatters.
- `apps/desktop/src/output_tests.rs`: added REST output formatting tests.
- `apps/desktop/src/render/action_rows.rs`: made expanded action rows scrollable and improved required/optional metadata.
- `apps/desktop/src/render/footer.rs`: added overflow handling for long footer title/detail text.
- `apps/desktop/src/ui.rs`: integrated settings mode state and action-list scroll state already present in the current work.
- `apps/desktop/src/ui_body.rs`: passed action scroll state into action row rendering.
- `apps/desktop/src/ui_commands.rs`: opened settings as an internal action and allowed optional-argument actions to submit empty input.
- `apps/desktop/src/main.rs`: scoped the animation module to tests because it is currently test-only.

## Commands Executed

- `git status --short`: confirmed broad dirty worktree with desktop, chrome extension, web, server, and docs changes.
- `cargo test --manifest-path apps/desktop/Cargo.toml`: passed after the final changes.
- `cargo build --release --manifest-path apps/desktop/Cargo.toml --bin axon-palette`: passed with a clean release build.
- `python3 scripts/enforce_monoliths.py --file ...`: passed with one warning-level function-size note.
- `git diff --check -- ...`: passed with no whitespace errors.
- Live REST probes using `curl` with bearer auth from `~/.axon/.env`: discovery/read routes returned success, and empty invalid write/job bodies returned expected validation errors.

## Errors Encountered

- `Div::single_line()` did not exist in this GPUI version.
  - Resolution: removed the unsupported call and kept `overflow_hidden()` plus `text_ellipsis()`.
- Directly sourcing `~/.axon/.env` failed on `NVIDIA_REQUIRE_CUDA=cuda>=12.2` and damaged shell command lookup during smoke testing.
  - Resolution: parsed only the needed keys with Python for the smoke test, then updated the Rust REST client to parse `.env` directly.
- Release build initially had warning noise from legacy process-output helpers and test-only animation functions.
  - Resolution: scoped those helpers/modules behind `#[cfg(test)]` where they are currently only used by tests.

## Behavior Changes (Before/After)

- Before: the palette covered only a smaller REST subset and could still leave users with raw JSON for many operations.
- After: the palette exposes the main REST-backed Axon operations and renders results as concise, readable output.
- Before: optional commands looked visually like required-argument commands.
- After: optional commands show an `optional` label.
- Before: expanded action rows could clip when the command list was long.
- After: the action list is scrollable with a stable max height.
- Before: the desktop app needed runtime environment variables to find the server/token.
- After: it can read `AXON_SERVER_URL` and `AXON_MCP_HTTP_TOKEN` from `~/.axon/.env` directly.

## Verification Evidence

| command | expected | actual | status |
| --- | --- | --- | --- |
| `cargo test --manifest-path apps/desktop/Cargo.toml` | desktop tests pass | 79 passed, 0 failed | pass |
| `cargo build --release --manifest-path apps/desktop/Cargo.toml --bin axon-palette` | release build succeeds | finished release profile cleanly | pass |
| `python3 scripts/enforce_monoliths.py --file ...` | monolith policy passes | passed; warning for `ui_commands.rs:67 submit()` at 107 lines | pass |
| `git diff --check -- ...` | no whitespace errors | no output | pass |
| `GET /v1/doctor`, `/v1/status`, `/v1/sources`, `/v1/domains`, `/v1/stats` | authenticated success | all returned 200 in live probe | pass |
| `POST /v1/query` | authenticated success | returned 200 with `results` | pass |
| `POST /v1/evaluate` with empty question | route reachable and validates input | returned 400 with `kind,message` | pass |
| `POST /v1/crawl`, `/v1/embed`, `/v1/extract`, `/v1/ingest` with empty bodies | routes reachable and validate input | returned 400 with `kind,message` | pass |
| `GET /v1/watch` | authenticated success | returned 200 with `limit,watches` | pass |

## Risks and Rollback

- The worktree contains many unrelated dirty files. Rollback should be scoped to the desktop files listed above unless the user explicitly wants a broader reset.
- The palette now exposes more write-capable operations. The UI routes requests through existing server auth and validation, but destructive/admin actions were not added as ordinary palette actions in this pass.
- REST output formatting is schema-tolerant, but future response shape changes may need formatter updates.
- Rollback path: revert the desktop files touched in this session and remove `apps/desktop/src/rest_client_tests.rs`.

## Decisions Not Taken

- Did not run a fresh Steamy visual session in this slice; verification focused on source, build, tests, and live REST route behavior.
- Did not wire destructive admin operations like migrate/dedupe as normal palette actions.
- Did not refactor `ui_commands.rs::submit()` despite the warning-level monolith notice because it was not a hard failure.

## References

- `src/web/server/routing.rs`
- `src/web/server/handlers/rag.rs`
- `src/services/types/service.rs`
- Prior Windows artifact testing memory noted an earlier `axon-palette.exe` packaged artifact crash with `0xc000001d` from a self-hosted runner CPU-codegen issue.

## Open Questions

- Whether the next packaged Windows/Linux artifact should be retested on Steamy after CI produces fresh binaries.
- Whether the palette should expose admin operations with additional confirmation UI.
- Whether `ui_commands.rs::submit()` should be split before it approaches the hard monolith limit.

## Next Steps

Started but not completed:
- Fresh visual QA of the newly built palette on Steamy/WSLg and Windows packaged artifacts.

Follow-on tasks:
- Push the desktop palette changes once the user confirms the broader dirty worktree staging strategy.
- Wait for CI and then download/test the latest packaged `axon.exe` and `axon-palette.exe`.
- Add confirmation UX before any destructive admin operation is exposed in the palette.
