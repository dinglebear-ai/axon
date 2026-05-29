---
date: 2026-05-29 18:13:11 EST
repo: git@github.com:jmagar/axon.git
branch: rename-stack-to-compose (work performed on fix/probe-rpc-hardening, since merged)
head: d8c609f4
working directory: /home/jmagar/workspace/axon_rust
worktree: /home/jmagar/workspace/axon_rust
pr: "#145 fix(endpoints): harden --probe-rpc concurrency, timeout, MCP fidelity, bounded reads (v4.13.1) — https://github.com/jmagar/axon/pull/145 (MERGED as 34d4b360)"
beads: axon_rust-cvnf
---

# Probe-rpc review, hardening, and PR #145 closeout

## User Request
Review the JSON-RPC probing code (`--probe-rpc`), explain how it works, identify issues, and suggest improvements — then "Address ALL issues", open a PR, run a multi-agent PR review (`/pr-review-toolkit:pr-review`), "address ALL issues" again, and run the `/gh-pr` comment-handler workflow.

## Session Overview
Reviewed the v4.13.0 `--probe-rpc` endpoint fingerprinter, found 7+ defects, and shipped fixes across two commits plus a CI lint fix on branch `fix/probe-rpc-hardening`. A 5-agent PR review surfaced one real bug (SSE parser) and several doc/test gaps; all were addressed. The PR's red CI was diagnosed (mostly my own test-only lint errors, plus a pre-existing `palette-tauri` infra failure) and fixed. PR #145 was subsequently merged to `main` as `34d4b360`.

## Sequence of Events
1. Read `src/services/endpoints/probe.rs`, `types/endpoints.rs`, the CLI handler, `build_client` (SSRF/redirect), and confirmed no existing probe tests; produced a written review identifying 7+ issues.
2. On user go-ahead, branched `fix/probe-rpc-hardening`, created bead `axon_rust-cvnf`, and implemented all fixes: per-endpoint semaphore, 3s timeout clamp, MCP session-id replay + `notifications/initialized`, SSE parsing + `Accept` header, 256 KiB body cap, removed dead `error` field, `protocol`/`transport` enums, `protocolVersion` bump, and a new `probe_tests.rs` sidecar.
3. Hit a hard build wall (see Errors); diagnosed it as sccache, not code. Got a green release build + 15 passing probe tests + clean clippy/fmt by disabling sccache.
4. Bumped to v4.13.1, committed (`8fd6144a`), pushed, opened PR #145.
5. Ran `/pr-review-toolkit:pr-review` — 5 parallel specialist agents. Addressed every finding (SSE drain bug, ACP→OpenRPC docs, CHANGELOG count, semaphore comment, tests); regenerated OpenAPI; committed `5d61db1a`.
6. Ran `/gh-pr`: 0 review threads (only a rate-limited CodeRabbit notice), but the status dashboard exposed red CI. Fixed two test-only lint errors caught by CI's `--all-targets` run; committed and pushed; verified `check`/`clippy`/`release`/`test` green.
7. PR #145 merged to `main` (`34d4b360`); branch deleted; follow-on work (`rename-stack-to-compose`) began outside this session.

## Key Findings
- **Semaphore was per-session, not per-endpoint** (`probe.rs`): `AXON_ENDPOINT_PROBE_CONCURRENCY` capped concurrent discovery sessions while a hardcoded `4` governed fan-out — re-broke a fix previously made for `BUNDLE_FETCH_SEMAPHORE`.
- **Timeout inflation**: the probe routed through `timeout_secs(cfg, ...)`, so `request_timeout_ms` (20s on `high-stable`) overrode the advertised 3s budget across up to 5 sequential requests/endpoint.
- **MCP fidelity gaps**: `tools/list` was a bare POST with no `Mcp-Session-Id`/`notifications/initialized`, so stateful Streamable-HTTP servers returned empty tool lists; SSE-replying servers degraded to a bare `transport: sse` label.
- **SSE parser false-negative** (review finding): `read_first_sse_json` never drained consumed blocks, so a non-data preamble on a kept-open stream was re-scanned until timeout — fixed by byte-buffer scanning + drain (`sse_event_boundary`).
- **Build environment, not code**: sccache could not `dlopen libssl.so.3` for the `sqlx` proc-macro after a linuxbrew OpenSSL upgrade; this masqueraded as shifting `spider`/`utoipa` compile errors.

## Technical Decisions
- **Removed the dead `error` field** rather than wiring it up: the "return None on non-match" ladder has no clean place to surface it, and emitting results for every non-RPC URL would be noise.
- **`Option<RpcProtocol>`/`Option<RpcTransport>` enums** over a single richer enum: `None` = "probed, nothing matched" is a clean single meaning; a struct-level enum refactor would break the wire contract, so it was documented as a future option instead.
- **Hard 3s timeout ceiling** via `clamp(1, PROBE_TIMEOUT_SECS)`: a configured value may shorten the probe but never lengthen it.
- **Disable sccache locally** with `RUSTC_WRAPPER="" --config 'build.rustc-wrapper=""'` + pinned toolchain rather than fighting the brew OpenSSL state; a repo-level `.cargo/config.toml` `rustc-wrapper=""` (added concurrently) made this permanent.

## Files Changed
All changes merged via PR #145 (`34d4b360`). Paths relative to repo root.

| status | path | purpose | evidence |
|--------|------|---------|----------|
| modified | src/services/endpoints/probe.rs | per-endpoint semaphore, 3s clamp, MCP session/initialized, SSE incremental parse + `sse_event_boundary`, 256 KiB cap, enums | clippy/fmt clean; 15 tests pass |
| created | src/services/endpoints/probe_tests.rs | 15 httpmock tests (ladder, session replay, SSE drain/precedence, body cap, content-type fall-through, clamp table, notification-sent) | `test result: ok. 15 passed` |
| modified | src/services/types/endpoints.rs | `RpcProtocol`/`RpcTransport` enums, removed `error`, ACP→OpenRPC doc, Mcp SSE-inferred doc, struct invariant note | CI check/clippy green |
| modified | src/cli/commands/endpoints.rs | CLI printer uses `protocol.as_str()` enum | compiles |
| modified | src/web/server/openapi.rs | register `RpcProtocol`/`RpcTransport` schemas | openapi:export valid |
| modified | apps/web/openapi/axon.json | regenerated (enums, description fix, info.version 4.13.1) | valid JSON, 2-line diff |
| modified | apps/web/lib/generated/axon-api.ts | regenerated TS client (enums, `error` removed) | exit 0 |
| modified | CHANGELOG.md | v4.13.1 entry (+ "six"→"five" correction) | — |
| modified | Cargo.toml / Cargo.lock | version 4.13.0 → 4.13.1 | — |
| modified | README.md | version badge 4.13.1 | — |

Not authored by this session but present on the merged PR: `.cargo/config.toml`, `.github/workflows/ci.yml`, `docs/config/env-migration-matrix.toml`, `scripts/check-env-config-boundary.py`, `apps/web/package.json` (concurrent `fix(release)` work + a later v4.13.2 CI cone-mode fix for `palette-tauri`).

## Beads Activity
| id | title | actions | status | why |
|----|-------|---------|--------|-----|
| axon_rust-cvnf | Harden --probe-rpc: fix concurrency, timeout, MCP fidelity, bounded reads, tests | created (P2 bug), claimed, closed with reason | CLOSED | Tracked the full probe-rpc remediation shipped in PR #145 |

No other bead activity. No follow-up beads created: the only out-of-scope item surfaced (`palette-tauri` CI) was fixed by concurrent work and merged in #145.

## Repository Maintenance
- **Plans**: checked `docs/plans/`; this session created/completed no plan files. The injected active plan (`2026-05-27-android-phase2-stubbed-modes.md`) already exists under `docs/plans/complete/`. No moves performed.
- **Beads**: `bd show axon_rust-cvnf` → CLOSED (verified). No stale/orphan cleanup attempted for this session's scope.
- **Worktrees/branches**: `git worktree list` shows the single main checkout. `git branch` shows only `main` and `rename-stack-to-compose`; `fix/probe-rpc-hardening` was already merged (PR #145) and deleted locally — no action needed. `rename-stack-to-compose` is active follow-on work and was left untouched.
- **Stale docs**: this session's doc changes (CHANGELOG/README/axon.json) are merged; nothing left stale by it.
- **Transparency**: working tree was clean at session-doc time (`git status --porcelain` empty). The session-file commit below is path-limited to this artifact only.

## Tools and Skills Used
- **Bash**: dominant tool — git, gh (mise binary, bypassing an rtk wrapper that mishandled `--job`), cargo/clippy/fmt (sccache-bypassed), `bd`, `python3` (gh-pr scripts), file inspection.
- **Read / Edit / Write**: source edits and review.
- **Agent (subagents)**: 5 `pr-review-toolkit` specialists in parallel — `code-reviewer`, `pr-test-analyzer`, `silent-failure-hunter`, `type-design-analyzer`, `comment-analyzer`. No failures.
- **Skill**: `pr-review-toolkit:review-pr`, `gh-pr`, `save-to-md`.
- **Issues observed**: (1) sccache build breakage required a non-obvious workaround; (2) `gh` intercepted by an rtk wrapper — resolved by calling the mise `gh` binary directly; (3) local `clippy --lib` did not lint test targets, so two test-only lints slipped to CI.

## Commands Executed
| command | result |
|---------|--------|
| `cargo build --release --bin axon` (sccache-bypassed) | Finished in 6m20s |
| `cargo test --release --lib endpoints::probe` (bypassed) | `15 passed; 0 failed` |
| `cargo clippy --workspace --all-targets --locked --features test-helpers -- -D warnings` (bypassed) | clean |
| `gh pr create --base main` | created PR #145 |
| `gh pr checks 145` | check/clippy/release/test pass; palette-tauri pre-existing fail |
| `gh pr view 145 --json state` | MERGED (34d4b360) |

## Errors Encountered
- **sccache cannot `dlopen libssl.so.3`**: every `cargo build` failed with shifting phantom errors in `spider`/`spider_transformations`/`utoipa-swagger-ui`; root cause was the sccache daemon's stale/incompatible OpenSSL environment after a linuxbrew upgrade. Resolved by building with `RUSTC_WRAPPER=""`, `RUSTUP_TOOLCHAIN=1.94.0-...`, the explicit toolchain `cargo`, and `--config 'build.rustc-wrapper=""'`. Saved to auto-memory (`env_sccache_libssl_build_break.md`).
- **probe test mock collision**: `body_includes("initialize")` also matched `"notifications/initialized"`, so the initialize mock swallowed the notification POST and `assert_calls` failed. Fixed by matching the quoted `"\"initialize\""`.
- **CI lint failures**: `unnecessary_qualifications` (`crate::core::config::Config`) and deprecated `assert_hits_async` in test code, caught by CI's `--all-targets`/warnings-as-errors but missed by local `--lib`. Fixed to `Config::test_default()` and `assert_calls_async`; verified with the exact CI command.

## Behavior Changes (Before/After)
| area | before | after |
|------|--------|-------|
| `AXON_ENDPOINT_PROBE_CONCURRENCY` | capped sessions; fan-out hardcoded to 4 | governs global per-endpoint in-flight probes |
| Probe timeout | up to 20s/request via profile | hard 3s ceiling |
| MCP `tools/list` | empty on stateful servers | session-id replayed + `initialized` sent → tools enumerated |
| MCP-over-SSE | bare `transport: sse` | `serverInfo`/tools parsed from first SSE frame |
| `RpcProbeResult` | free-form strings + dead `error` | typed enums, no `error` (wire strings unchanged) |

## Verification Evidence
| command | expected | actual | status |
|---------|----------|--------|--------|
| release build (bypassed) | compiles | Finished 6m20s | pass |
| `cargo test ... endpoints::probe` | all pass | 15 passed | pass |
| exact CI clippy command | no warnings | clean | pass |
| CI `test` job (PR #145) | full suite green | pass (9m57s) | pass |
| CI `check`/`clippy`/`release` | green | pass | pass |
| `gh pr view 145` | merged | MERGED 34d4b360 | pass |

## Risks and Rollback
- Wire contract preserved (enum strings unchanged; only the never-emitted `error` field dropped) — low consumer risk. Rollback = revert `34d4b360`.
- The SSE incremental parser is the highest-novelty code; covered by the keepalive-drain/precedence test, but cross-stream-chunk splitting is exercised only indirectly (httpmock delivers single chunks).

## Decisions Not Taken
- Did **not** refactor `RpcProbeResult` into a per-protocol enum (would break the wire contract) — documented as a follow-up instead.
- Did **not** re-trigger CodeRabbit or fix `palette-tauri` from this session — the former was credit-limited, the latter a pre-existing repo-wide CI break (later fixed by concurrent v4.13.2 cone-mode work).

## References
- PR: https://github.com/jmagar/axon/pull/145 (merged `34d4b360`)
- Auto-memory: `env_sccache_libssl_build_break.md`
- Beads memory: `axon-rust-ci-sparse-checkout-cone-mode-cargo` (palette-tauri cone-mode fix, v4.13.2)

## Open Questions
- Cross-chunk SSE frame assembly in `read_first_sse_json` is only indirectly tested; a fault-injecting chunked-body test would close the gap.

## Next Steps
- None required for this session — PR #145 is merged and CI is green. Current branch `rename-stack-to-compose` carries unrelated follow-on work (compose rename, ingest error surfacing) outside this session's scope.
- If desired later: add a chunked-SSE unit test for `read_first_sse_json`, and consider the `RpcProbe` enum refactor on a future wire-version bump.
