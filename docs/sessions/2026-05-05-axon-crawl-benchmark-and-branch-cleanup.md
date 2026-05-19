# 2026-05-05 Axon Crawl Benchmark and Branch Cleanup

## Context

- Repository: `/home/jmagar/workspace/axon_rust`
- Branch at save time: `main`
- HEAD at save time: `b356e8fd`
- Status at save time: clean, tracking `origin/main`
- Date: 2026-05-05

## Work Completed

### Crawl Benchmark

Built both Axon binaries:

```bash
cargo build --locked --bin axon
cargo build --release --locked --bin axon
```

The release build completed successfully but took about 9m13s because it performed an optimized compile with release profile settings.

Benchmarked debug vs release with the same crawl target and flags:

```bash
./target/{debug,release}/axon crawl https://code.claude.com/docs \
  --wait true --embed false --render-mode http --max-pages 500
```

Results:

| Binary | Run 1 wall | Run 2 wall | Average wall | Average user CPU |
| --- | ---: | ---: | ---: | ---: |
| debug | 24.43s | 23.61s | 24.02s | 173.02s |
| release | 10.44s | 9.71s | 10.08s | 39.26s |

Conclusion: release was about 2.4x faster wall-clock and about 4.4x lower user CPU for this HTTP crawl.

Output parity checks:

- Debug and release both wrote 2513 files.
- Both output directories were 67M.
- Both sync manifests had 1255 lines.

Output directories used:

- `/tmp/axon-bench-debug-code-claude`
- `/tmp/axon-bench-release-code-claude`
- `/tmp/axon-bench-debug2-code-claude`
- `/tmp/axon-bench-release2-code-claude`

### Branch and Worktree Cleanup

Initial state included:

- Main worktree: `/home/jmagar/workspace/axon_rust` on `main`
- Extra worktree: `/home/jmagar/workspace/axon_rust-1d2.3-fixes` on `bd-1d2.3/ssh-remote-deployment`
- Local branches:
  - `main`
  - `bd-1d2.3/ssh-remote-deployment`
  - `bd-1d2.1/config-system-cleanup`

Cleaned up merged work:

```bash
git worktree remove /home/jmagar/workspace/axon_rust-1d2.3-fixes
git branch -d bd-1d2.3/ssh-remote-deployment
git push origin --delete bd-1d2.3/ssh-remote-deployment
git push origin --delete bd-1d2.1.8.1/reject-home-parentdir bd-1d2.2/web-panel-axum-server
```

Investigated `bd-1d2.1/config-system-cleanup`:

- Ahead commit: `f2669ed3 fix: address PR 65 review comments`
- The commit was not merged by hash, but useful changes were already present on current `main` via later work.
- Raw cherry-pick/patch application did not apply cleanly because `main` had moved through refactors.
- Verified examples already present on `main`:
  - `AXON_MCP_TRANSPORT` resolver in `crates/core/config/parse/helpers.rs`
  - explicit missing `AXON_CONFIG_PATH` hard-fail in `crates/core/config/parse/toml_config.rs`
  - `CARGO_TARGET_DIR`-aware `just build` and `just install`
  - `test-infra-up` and `test-infra-down` aliases
  - updated `scripts/check_mcp_http_only.sh`
  - `scripts/dev-setup.sh` using `just services-up`

User requested closing the remaining branch, so it was removed despite being technically unmerged by commit hash:

```bash
git branch -D bd-1d2.1/config-system-cleanup
git push origin --delete bd-1d2.1/config-system-cleanup
```

Final branch/worktree state:

- Only worktree: `/home/jmagar/workspace/axon_rust`
- Only local branch: `main`
- No remaining `bd-1d2*` remote branches
- Worktree clean

## Commands With Notable Evidence

```bash
git worktree list --porcelain
git branch --format='%(refname:short)|%(HEAD)|%(upstream:short)'
git branch -r --format='%(refname:short)' | rg 'bd-1d2' || true
git status --short --branch
```

Final evidence:

```text
worktree /home/jmagar/workspace/axon_rust
HEAD b356e8fd18505ae3d470e4418e00743adadddd83
branch refs/heads/main

main|*|origin/main

## main...origin/main
```

## Open Questions

- None from this cleanup. If the deleted `bd-1d2.1/config-system-cleanup` branch had PR review-thread bookkeeping outside git, that would need separate GitHub-side verification.
