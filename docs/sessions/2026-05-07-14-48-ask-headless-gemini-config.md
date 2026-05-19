# Session: Ask Headless Gemini Config and Runtime Fixes

Date: 2026-05-07
Repo: `/home/jmagar/workspace/axon_rust`
Branch: `main`

## Summary

This session finished the `axon ask` headless Gemini work and made the local runtime match the intended default:

- `ask` now uses headless Gemini CLI as the canonical default path.
- `AXON_ASK_BACKEND=auto` now resolves to headless, not ACP.
- ACP is used only when `AXON_ASK_BACKEND=acp` is explicitly selected.
- The installed `axon` binary was rebuilt and installed at the Claude plugin-cache path used by `~/.local/bin/axon`.
- Local `.env` was updated and verified source-safe.

## Commits Pushed

- `46e9a37b fix: avoid duplicate ask output`
- `10834d67 fix: renumber ask source citations`
- `a1a9f21b fix: align headless llm paths`
- `728e5ae9 fix: make headless the canonical ask backend`

## Runtime Fixes

### Duplicate ask output

Problem: non-JSON `axon ask` streamed token deltas to stdout and then printed the final formatted `Conversation` block, duplicating the answer.

Fix: stdout rendering is centralized in the CLI. The streaming path still handles TTFT/fallback internally, but no longer prints token deltas before the final formatted output.

### Citation numbering

Problem: sources were labeled while assembling context buckets, then the context was sorted by relevance afterward. A high-relevance source could appear as `[S11]`, and grouped citations such as `[S11, S13]` were not rewritten.

Fixes:

- Context headers are renumbered after final relevance sorting.
- Final answer normalization rewrites sparse and grouped citations to display IDs.
- Inline citations now match the final `## Sources` list.

### Headless as canonical backend

Problem: headless was the default, but `auto` and some surrounding paths still behaved like ACP.

Fixes:

- Added effective backend helpers:
  - `AskBackend::uses_headless()`
  - `AskBackend::uses_acp()`
- `Headless` and `Auto` use the headless CLI completion path.
- `Acp` is the only mode that warms or calls ACP adapters.
- Updated affected paths:
  - `ask`
  - `evaluate`
  - `research`
  - `suggest`
  - `debug`
  - shared completion gateway

## Local `.env`

Updated local ignored `.env` to make the ask path explicit:

```dotenv
AXON_ASK_BACKEND=headless
AXON_ASK_AGENT=gemini
AXON_HEADLESS_GEMINI_HOME=
AXON_ASK_SERVER_URL=
OPENAI_MODEL=
```

Also fixed source safety for ACP args:

```dotenv
AXON_ACP_GEMINI_ADAPTER_ARGS='--acp|--allowed-mcp-server-names|__axon_ask_no_mcp__'
```

Important: ACP settings remain in `.env`, but are now used only when `AXON_ASK_BACKEND=acp`.

## Verification

Commands run successfully during the session:

```bash
cargo fmt --check
cargo test research --lib
cargo test suggest --lib
cargo test debug --lib
cargo test ask:: --lib
cargo check --bin axon
cargo clippy --lib --tests
cargo build --release --bin axon
AXON_ASK_BACKEND=auto axon ask "how do claude code hooks work?" --json
axon ask "how do claude code hooks work?" --json
zsh -c 'set -a; source .env; set +a; printf "AXON_ASK_BACKEND=%s\nAXON_ASK_AGENT=%s\nAXON_HEADLESS_GEMINI_HOME=%s\nAXON_ASK_SERVER_URL=%s\nOPENAI_MODEL=%s\n" "$AXON_ASK_BACKEND" "$AXON_ASK_AGENT" "$AXON_HEADLESS_GEMINI_HOME" "$AXON_ASK_SERVER_URL" "$OPENAI_MODEL"'
```

Observed live ask timings:

- `AXON_ASK_BACKEND=auto axon ask ... --json`: about `8.2s`
- installed `axon ask ... --json`: about `7.7s`

The installed binary reports:

```bash
axon 1.8.0
```

and resolves to:

```text
/home/jmagar/.claude/plugins/cache/jmagar-lab/axon/575c090bcaf5/bin/axon
```

## Notes and Caveats

- `./scripts/axon` sources `.env`, then runs `cargo run`, so it uses `target/debug/axon`, not the installed release binary. A wrapper smoke was slower because it launched the debug build and a Gemini child.
- The direct installed `axon` command is the faster runtime path verified above.
- `sccache` still prints `server looks like it shut down unexpectedly` at release-build startup, but builds complete via local compile. `cargo check` and clippy passed after cleanup.
- No Axon ask/Gemini processes were left running at the end of the session.
- `docs/sessions/` and `.env` are ignored by git in this repo.

## Final State

- Git branch `main` is up to date with `origin/main`.
- Tracked worktree is clean.
- `.env` is ignored and locally updated.
- Session note saved here:
  `docs/sessions/2026-05-07-14-48-ask-headless-gemini-config.md`
