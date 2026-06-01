# Monolith Policy
Last Modified: 2026-02-25

Version: 1.0.0
Last Updated: 01:26:53 | 02/25/2026 EST

This repository enforces a ratcheting policy to prevent new monolithic files and functions from being introduced.

## Scope

- Enforced on changed code, not the full codebase.
- Enforced locally via `lefthook` pre-commit.
- Enforced in CI for pull requests and pushes.

## Limits

- File size limit: `500` lines
- Rust function size limit: warn at `80` lines, hard fail at `120` lines

## Checked File Types

File-size enforcement applies only to changed Rust source files:

- `.rs`

Function-size enforcement applies to changed Rust functions in `.rs` files.

## Test Exemptions

The following paths/patterns are exempt:

- `tests/**`
- `**/tests/**`
- `**/*_test.*`
- `**/*.test.*`
- `**/*.spec.*`
- `benches/**`
- `config/**`
- `**/config/**`
- `**/config.rs`

## Exceptions

Temporary file-level exceptions can be added to `.monolith-allowlist` (one repo-relative path per line).

Rules:

- Use exceptions only when necessary.
- Add a comment with ticket/date/owner above the entry.
- Remove entries as soon as refactoring is complete.

## Local Enforcement

Install hooks:

```bash
./scripts/install-git-hooks.sh
```

Run policy manually against staged changes:

```bash
python3 ~/.claude/hooks/enforce_monoliths.py --staged
```

## CI Enforcement

CI runs:

```bash
python3 ~/.claude/hooks/enforce_monoliths.py --base "$BASE_SHA" --head "$HEAD_SHA"
```

This keeps enforcement ratcheted to the change set under review.

## Config Files

- Policy logic: `~/.claude/hooks/enforce_monoliths.py`
- Local hooks: `lefthook.yml`
- Hook installer: `scripts/install-git-hooks.sh`
- CI job: `.github/workflows/ci.yml`
- Exception list: `.monolith-allowlist`
