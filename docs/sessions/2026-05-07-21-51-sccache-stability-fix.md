# Session: sccache Stability Fix

Date: 2026-05-07 21:51 EDT
Repo: `/home/jmagar/workspace/axon_rust`
Branch: `bd-work/retrieval-remediation-ug6`

## Summary

Investigated `sccache` after warnings that the server shut down unexpectedly and poor Rust cache hit rates. The issue was machine-local, not repo code.

Two root causes were identified:

- The local cache was full: `~/.cache/sccache` was about 33 GiB while the configured maximum was 32 GiB, causing eviction pressure and poor hit rates.
- The `sccache` server was running as an orphaned ad hoc process under PID 1, so client-started server lifecycle churn could produce shutdown warnings.

## Changes Made

Updated machine-local sccache configuration:

- `~/.config/sccache/config`
  - Increased disk cache size from `32 GiB` to `128 GiB`.

Added a persistent user systemd service:

- `~/.config/systemd/user/sccache.service`
  - Runs `/usr/bin/sccache` in foreground server mode.
  - Uses `SCCACHE_NO_DAEMON=1`.
  - Starts with `SCCACHE_START_SERVER=1`.
  - Sets `SCCACHE_CACHE_SIZE=128G`.
  - Writes sccache error output to `~/.local/state/sccache/error.log`.
  - Enabled under `default.target`.

## Verification

Fresh checks after the fix:

```text
systemctl --user is-enabled sccache.service
enabled

systemctl --user is-active sccache.service
active
```

`sccache --show-stats` reported:

```text
Cache location                  Local disk: "/home/jmagar/.cache/sccache"
Use direct/preprocessor mode?   yes
Version (client)                0.10.0
Cache size                           34 GiB
Max cache size                      128 GiB
```

A temporary Rust library smoke test was run:

1. `cargo check`
2. `cargo clean`
3. `cargo check`
4. `sccache --show-stats`

That produced one Rust cache miss followed by one Rust cache hit, proving the managed server can read/write cache entries correctly after a clean rebuild.

## Current State

The service is live and currently managed by user systemd:

```text
/usr/bin/sccache
```

The repo worktree was clean before this note was created.

## Follow-Ups

No further sccache repair is needed right now.

Optional future improvement:

- Add monitoring or alerting if `~/.cache/sccache` approaches the 128 GiB cap. This is not required for cleanup, because sccache already performs built-in LRU eviction at the configured max size.
