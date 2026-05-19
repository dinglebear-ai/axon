# Session Save: PR69 Merge And Ask Performance Regression

Date: 2026-05-07 10:45 EDT
Repo: `/home/jmagar/workspace/axon_rust`
Current branch: `main`
Current HEAD: `8403cbbb` (`Merge pull request #69 from jmagar/bd-teams/ask-perf-foundation`)
Working tree at save time: clean for tracked files; local `.env` is ignored and has machine-local edits.

## What Was Completed

- Merged PR #69: <https://github.com/jmagar/axon/pull/69>
- PR state: `MERGED`
- Merge commit: `8403cbbb7e3597fb841ea037b61e169bed656dbc`
- PR branch cleanup:
  - Deleted local merged branches `bd-teams/ask-perf-foundation`, `pr-69`, and `pr69-review`.
  - Remote PR branch was deleted by the GitHub merge.
- Root checkout returned to `main` and fast-forwarded to `origin/main`.

## PR69 Implemented Scope

- Ask sub-stage timing diagnostics.
- `axon ask --server-url` / `AXON_ASK_SERVER_URL`.
- Server `POST /v1/ask` endpoint.
- Token/auth hardening for server-backed ask.
- Warm ACP session plumbing and timing labels.
- Qdrant dual-search batch helper and fallback.
- Optional ask document-chunk cache.
- Adaptive `ask_full_docs` defaulting.
- Optional full-doc fetch skip gate.
- Ask perf benchmark harness and docs.
- HTTP idle pool cap.
- Version bumped through `1.6.2`.

## Verification Before Merge

- `cargo check`: passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo test --lib -- --test-threads=1`: `1596 passed; 0 failed; 5 ignored`.
- PR review threads: `0 unresolved / 25 total`.

## Config/Binary Work Performed After Merge

The first post-merge `axon ask` failed because `~/.axon/config.toml` contained new `[ask.cache]` / `[ask.adaptive]` sections while the installed `axon` binary was still old.

Actions taken:

- Replaced `/home/jmagar/.axon/config.toml` with current `config.example.toml`.
- Backup written to `/home/jmagar/.axon/config.toml.bak.20260507-101444`.
- Built current source with `cargo build --release --bin axon`.
- Verified `target/release/axon` is `axon 1.6.2`.

Important later finding:

- `/home/jmagar/.local/bin/axon` is a symlink into plugin cache:
  - `/home/jmagar/.local/bin/axon -> /home/jmagar/.claude/plugins/cache/jmagar-lab/axon/575c090bcaf5/bin/axon`
  - That binary currently reports `axon 1.5.5`.
- Current repo release binary reports `axon 1.6.2`:
  - `/home/jmagar/workspace/axon_rust/target/release/axon --version`
- This means PATH-level `axon` is not reliably current and can regress when plugin cache symlinks move.

## Runtime/Performance Investigation

Observed timings after PR69:

- Direct `axon ask "how do claude code hooks work?"`:
  - Retrieval/context were fast, roughly sub-second.
  - LLM dominated total time, with one observed run around `llm=244561ms`, `total=245079ms`.
- Short diagnostic direct ask:
  - Command: `AXON_ASK_DIAGNOSTICS=true timeout 90 axon ask --json "one sentence: what are claude code hooks?"`
  - Result: `retrieval=109ms`, `context_build=8ms`, `llm=28560ms`, `total=28678ms`.
- Server-backed ask against temporary current server:
  - Result: `retrieval=50ms`, `context_build=4ms`, `llm=18120ms`, `total=18175ms`.
  - Better than direct, still not acceptable.

## Stale Server Found

Port `8001` was initially owned by an old plugin-cache server:

- PID: `1236642`
- Command: `/home/jmagar/.claude/plugins/cache/jmagar-lab/axon/d3c85b352178/bin/axon serve mcp`
- Version: `axon 1.5.5`
- Env included `AXON_MCP_HTTP_TOKEN=<redacted>`, `AXON_COLLECTION=cortex`, and localhost service URLs.

That server was killed.

## Current Server State At Save Time

There is a current server still running:

- PID: `2091566`
- Command: `/home/jmagar/.local/bin/axon serve`
- Listening: `127.0.0.1:8001`
- Process tree shows Gemini ACP child without recursive Axon MCP/synapse/unraid children after MCP suppression.

However, because `/home/jmagar/.local/bin/axon` now points at plugin-cache `1.5.5`, this running server should be treated as suspect until its executable path/version is revalidated or restarted explicitly from `target/release/axon`.

## Local `.env` Changes

`.env` is ignored. Relevant machine-local changes at save time, secrets redacted:

```dotenv
AXON_ASK_SERVER_URL=http://127.0.0.1:8001
AXON_ASK_AGENT=gemini
AXON_ACP_GEMINI_ADAPTER_CMD=gemini
AXON_ACP_GEMINI_ADAPTER_ARGS=--acp|--allowed-mcp-server-names|__axon_ask_no_mcp__
AXON_ACP_PREWARM=true
AXON_ACP_PREWARM_AGENT=true
```

Rationale:

- Route plain `axon ask` to the canonical server path.
- Use Gemini ACP with MCP servers suppressed for ask synthesis.
- Keep prewarm enabled for the long-lived server path.

## Key Diagnosis

PR69 made retrieval/context fast and measurable, but did not make the LLM synthesis path fast enough.

Two concrete problems were found:

1. Plain `axon ask` still effectively used a cold/slow ACP completion path unless routed through `AXON_ASK_SERVER_URL`.
2. The Gemini ACP adapter initially loaded the full MCP/tool environment, spawning recursive `axon mcp`, synapse, and unraid MCP children during ask synthesis. That was fixed locally by changing Gemini args to:
   - `--acp|--allowed-mcp-server-names|__axon_ask_no_mcp__`

A third unresolved infrastructure problem remains:

3. `/home/jmagar/.local/bin/axon` is plugin-cache managed and currently resolves to `axon 1.5.5`, while the repo release binary is `1.6.2`.

## Open Questions / Next Steps

- Fix PATH/install source of truth:
  - Either replace `/home/jmagar/.local/bin/axon` with a stable wrapper that executes `/home/jmagar/workspace/axon_rust/target/release/axon`, or update the plugin-cache skill binary and symlink generation so it points to `1.6.2`.
- Kill/restart PID `2091566` explicitly from a known `1.6.2` binary after the PATH issue is fixed.
- Re-run:
  - `which -a axon`
  - `axon --version`
  - `/home/jmagar/workspace/axon_rust/target/release/axon --version`
  - `pstree -ap <axon-serve-pid>`
  - `timeout 90 axon ask --json "one sentence: what are claude code hooks?"`
- If LLM still takes tens of seconds with no MCP children, investigate ACP/Gemini model latency or switch ask synthesis to a direct LLM endpoint / lighter adapter.
- Consider making ask ACP setup enforce a no-tool/no-MCP synthesis mode in code instead of relying on `.env` args.

