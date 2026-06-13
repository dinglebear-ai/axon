# Palette Action Help

Date: 2026-06-12
Branch: `codex/palette-action-help`
PR: https://github.com/jmagar/axon/pull/206
Worktree: `/home/jmagar/workspace/axon/.worktrees/palette-action-help`

## Summary

Implemented the Axon Palette action-help plan. The palette now has a local `help` action, command parsing for `help`, `?`, `<action> help`, and `<action> --help`, a selected-action question-mark affordance, and structured help rendering backed by shared action metadata. Local help is handled before backend/config guards and remote request construction rejects local actions defensively.

The action metadata is centralized in `actionMeta.ts` and consumed by both palette command behavior and help rendering. History replay preserves local help payloads, and the command bar was extracted into its own component to keep `App.tsx` under the monolith guard.

## Review Follow-up

CodeRabbit comments were addressed after the first pushed implementation:

- Local help no longer hijacks non-help actions whose raw query is `help`.
- `HelpResultView` exports a named props interface and null-guards `route`.
- The command-bar help button stays enabled for catalog help when there is no active match.
- History replay preserves empty-string text values.
- Release workflow `actions/setup-node` usages were pinned to `a0853c24544627f65ddf259abe73b1d18a591444`.

## Build Assets

The Rust build script now fails closed if `apps/web/out` is missing, empty, unreadable, or contains only fallback marker assets unless `AXON_ALLOW_FALLBACK_WEB_ASSETS=1` is set. CI placeholder-asset jobs use that env explicitly, while release jobs build real `apps/web` assets before compiling the binary.

## Verification

- `pnpm test` in `apps/palette-tauri`: 79 tests passed.
- `pnpm typecheck` in `apps/palette-tauri`: passed.
- `pnpm vite:build` in `apps/palette-tauri`: passed with the existing large chunk warning.
- `cargo fmt --check`: passed.
- `git diff --check`: passed.
- `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo check --workspace --lib --locked`: passed.
- `AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo clippy --workspace --lib --locked -- -D warnings`: passed.
- `cargo check --workspace --lib --locked` without the fallback env failed as expected when only fallback assets were present.
- Pre-push with `AXON_ALLOW_FALLBACK_WEB_ASSETS=1`: clippy passed and nextest ran 2,809 tests passed / 6 skipped.
- Reproduced the CI `test` command locally after the first CI runner failed from GitHub runner disk exhaustion.

## Notes

The first CI `test` attempt failed before Cargo output because the GitHub runner hit `No space left on device` while writing its own worker diagnostic log. The failed jobs were rerun.
