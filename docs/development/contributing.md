---
title: "Contributing to Axon"
created: 2026-07-07
updated: 2026-09-27
---

# Contributing to Axon
Last reviewed: 2026-09-27

This is the entry point for local development conventions: build setup, the
monolith policy, and security guardrails. For running tests see
[`testing.md`](testing.md); for cutting a release see
[`release-checklist.md`](release-checklist.md); for adding a new extension
point (source adapter, parser, provider, vector store, REST route, MCP
action) see the `adding-*.md` guides in this directory.

---

## Build setup

Read the canonical [AGENTS.md](../../AGENTS.md) and the relevant scoped guide
first. Repository manifests and checked-in build configuration are the build
authority; a developer’s global settings are optional host configuration.
See [documentation maintenance](documentation.md) for sources of truth and
verification scope.

### System prerequisites

- The exact Rust toolchain in `rust-toolchain.toml` (currently `1.97.1`),
  including rustfmt and clippy. `Cargo.toml` owns the minimum supported version.
  Check `rustup show active-toolchain` and any `RUSTUP_TOOLCHAIN` override;
  do not substitute whichever stable toolchain happens to be installed.
- `clang` and `mold` for fast Linux builds: `apt install clang mold`
- `mingw-w64` for Windows cross-compilation: `apt install mingw-w64`
- `just` command runner (optional): `cargo install just`

### Global Cargo config

Inspect `~/.cargo/config.toml` and environment overrides when diagnosing
build differences. This repository does not set a rustc-wrapper; hosts may
configure Kache, linker selection, or profile tuning globally. Use
`RUSTC_WRAPPER=""` to isolate a wrapper problem without rewriting another
project’s global configuration. Linux linker packages are not macOS
prerequisites, and macOS development should use appropriate external
provider endpoints rather than assuming local NVIDIA services.

### Local `.cargo/config.toml`

This repo's `.cargo/config.toml` contains the xtask alias, Windows
cross-compile linker, and the non-forced development opt-in
`AXON_ALLOW_FALLBACK_WEB_ASSETS=1`. Release builds still require the correct
web assets when that opt-in is absent:

```toml
[alias]
xtask = "run --package xtask --"

[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
```

The Windows linker entry is a per-repo setting because CI environments may
not have the standard global `~/.cargo/config.toml`. All other settings
(profile tuning, mold linker for Linux) are inherited from the global config.

### Windows cross-compilation

axon publishes Windows binaries via the release CI workflow. To
cross-compile locally:

```bash
rustup target add x86_64-pc-windows-gnu
cargo build --target x86_64-pc-windows-gnu --release
```

## Monolith policy

This repository enforces a ratcheting policy to prevent new monolithic files
and functions from being introduced.

**Scope:**
- Enforced on changed code, not the full codebase.
- Enforced locally via `lefthook` pre-commit.
- Enforced in CI for pull requests and pushes.

**Limits:**
- File size limit: `500` lines
- Rust function size limit: warn at `80` lines, hard fail at `120` lines

**Checked file types:** file-size enforcement applies only to changed Rust
source files (`.rs`); function-size enforcement applies to changed Rust
functions in `.rs` files.

**Test exemptions** (the following paths/patterns are exempt):

- `tests/**`
- `**/tests/**`
- `**/*_test.*`
- `**/*.test.*`
- `**/*.spec.*`
- `benches/**`
- `config/**`
- `**/config/**`
- `**/config.rs`

**Exceptions:** temporary file-level exceptions can be added to
`.monolith-allowlist` (one repo-relative path per line). Use exceptions only
when necessary, add a comment with ticket/date/owner above the entry, and
remove entries as soon as refactoring is complete.

**Local enforcement:**

```bash
./scripts/install-git-hooks.sh                                    # install hooks once
python3 scripts/enforce_monoliths.py --staged              # run manually against staged changes
```

**CI enforcement:**

```bash
python3 scripts/enforce_monoliths.py --base "$BASE_SHA" --head "$HEAD_SHA"
```

This keeps enforcement ratcheted to the change set under review.

**Config files:** checked-in policy logic lives in `scripts/enforce_monoliths.py`,
local hooks in `lefthook.yml`, the hook installer in
`scripts/install-git-hooks.sh`, the CI job in `.github/workflows/ci.yml`, and
the exception list in `.monolith-allowlist`.

## Security guardrails

Safety and security patterns enforced across the Axon stack.

### Credential management

- All credentials live in `~/.axon/.env` with `chmod 600` permissions.
- Never commit real `.env` files.
- Use `.env.example` as a tracked template with placeholder values only.
- Direct Docker Compose commands should pass `--env-file ~/.axon/.env` so the
  canonical env file is used for `${VAR}` interpolation; service containers
  also read `${AXON_HOME:-${HOME}/.axon}/.env`.

`.gitignore` and `.dockerignore` must include:

```
.env
*.secret
*.pem
*.key
```

Lefthook hooks verify security invariants:

| Hook | Purpose |
|------|---------|
| `cargo xtask check-env-staged` | Blocks commits that include `.env` files |
| `cargo xtask check-mcp-http` | Verifies MCP transport configuration parity |

### Web panel token model

The current web panel uses the file-backed panel password generated by
`axon serve`, plus the MCP HTTP auth boundary for MCP and first-party action
routes.

| Token | Scope | Browser-visible |
|-------|-------|-----------------|
| `~/.axon/panel-password` | Gates `/api/panel/config`, `/api/panel/ops`, and setup routes | Returned only after `/api/panel/login` succeeds |
| `AXON_HTTP_TOKEN` | Static bearer for `/mcp` and protected `/v1` routes | Client-configured secret |
| OAuth JWT (`AXON_AUTH_MODE=oauth`) | OAuth bearer for `/mcp` and protected `/v1` routes | Client obtains through OAuth flow |

Rules:
- `~/.axon/panel-password` must stay mode `0600`.
- Non-loopback HTTP binds require bearer or OAuth auth.
- Do not expose Chrome, Qdrant, or TEI directly to a network.
- Mobile clients must require an explicit panel-token unlock for
  `/api/panel/*`. Do not fall back to `AXON_HTTP_TOKEN` or OAuth tokens
  for panel config routes on the client.
- Mobile settings UIs must not write a masked placeholder back to disk as a
  secret value. Preserve the raw server text privately and patch only
  genuinely changed secret fields.

MCP OAuth is an HTTP auth mode for MCP and first-party action clients. It does
not replace the file-backed panel password used by setup/config panel routes.

### Docker security

Container images and Compose are development/reference surfaces, not the
supported production Axon-process lifecycle. Production uses native Axon
under systemd; see [deployment](../operations/deployment.md).

**Non-root execution.** The reference `axon` container runs the unified server as
UID/GID `1000:1000` via Compose and the runtime image `USER` directive:

```yaml
user: "1000:1000"
```

**No baked environment.** Docker images must not contain credentials at build
time: no `ENV AXON_PG_URL=...` in Dockerfiles, no `COPY .env` in Dockerfiles.
Credentials are injected at runtime via `env_file:` or container
`environment:`.

**Image verification:**

```bash
# Check for baked secrets
docker inspect axon:local | jq '.[0].Config.Env'

# Check container revision matches git SHA
docker compose --env-file ~/.axon/.env -f docker-compose.prod.yaml ps
```

### Network security

- All service URLs should use `https://` in production.
- HTTP is acceptable for local development and Docker-internal networking.
- The Chrome CDP endpoint is HTTP-only by design (internal network).
- Android allows cleartext only for the configured private Tailscale domains
  in `apps/android/app/src/main/res/xml/network_security_config.xml`; any
  other persisted Android server URL should be `https://`.

`validate_url()` (SSRF guard) enforces: no private/loopback IPs, no `file://`
or other non-HTTP schemes, and connect-time DNS-rebinding is closed by
`SsrfBlockingResolver` (re-checks every resolved IP).

> The Spider `firewall` feature (which would block known malware/phishing
> domains) is **not** enabled — `spider_firewall`'s build.rs fails under CI
> rate-limiting. `validate_url()` is the primary SSRF guard. See
> `docs/reference/spider-feature-flags.md`.

### Input handling

- Validate untrusted source URLs at the acquisition boundary and preserve
  connect-time SSRF checks when adding adapters.
- Keep source work in the canonical SourceRequest pipeline and unified job
  store; do not add source-specific queue or worker configuration.
- Read crawl limits, robots policy, chunking, provider concurrency, and retry
  defaults from the typed configuration and generated
  [configuration references](../reference/config/). Do not duplicate changing
  defaults in this contribution guide.
- A source adapter acquires content; document preparation, embedding, vector
  publication, and cleanup remain in their owning domains.

### Logging

- Never log credentials, tokens, or API keys.
- CLI outputs JSON data to stdout and progress/logs to stderr.
- Configure log rotation through the current configuration registry rather
  than relying on copied defaults in this guide.
- Keep terminal/progress logs on stderr and machine-readable command data on
  stdout.

## Rust module and test layout

### Module Layout — Modern Rust Convention (ENFORCED)

**Never use `mod.rs`.** Use the Rust 2018+ file-per-module layout:

```plaintext
# WRONG — do not do this
foo/
└── mod.rs      ← forbidden

# CORRECT
foo.rs          ← module root lives here
foo/
├── bar.rs      ← submodule
└── baz.rs      ← submodule
```

- Module root always lives in `foo.rs`, never `foo/mod.rs`
- Submodules live in `foo/bar.rs`, declared with `mod bar;` inside `foo.rs`
- When splitting an existing `foo/mod.rs`: copy it to `foo.rs`, delete `foo/mod.rs` — the submodule files stay in `foo/` unchanged
- This applies everywhere: `src/`, `src/*/`, nested modules — no exceptions

### Test files — sidecar `_tests.rs` convention (ENFORCED)

**Tests live in sibling files**, not inline `#[cfg(test)] mod tests { ... }` blocks. For each source file with tests, create a sibling `_tests.rs` file and declare it inside the source with the `#[path]` attribute:

```plaintext
foo.rs          ← source code
foo_tests.rs    ← sidecar test file (one per original `#[cfg(test)] mod X` block)
```

In `foo.rs`:

```rust
#[cfg(test)]
#[path = "foo_tests.rs"]
mod tests;
```

In `foo_tests.rs`:

```rust
use super::*;  // tests still see foo.rs's private items

#[test]
fn it_works() { ... }
```

**Rules:**

- **One sidecar per original `#[cfg(test)] mod X { ... }` block.** Never wrap multiple blocks under a single `mod tests` — this breaks `cargo test foo::<orig_mod_name>::test_x` selectors and risks visibility escalation. If `foo.rs` had `mod tests`, `mod legacy`, and `mod proptest_tests`, emit three sidecar files: `foo_tests.rs`, `foo_legacy_tests.rs`, `foo_proptest_tests.rs`, with three matching `#[path]` declarations in `foo.rs`.
- **Source-side `mod` name must match the original block's mod name** (`mod legacy`, `mod proptest_tests`, not always `mod tests`). Test selectors stay identical to pre-migration.
- **Why `#[path]`?** It decouples disk location from module hierarchy. The file is a sibling of `foo.rs` on disk, but the module is a **child** of `foo`, so `use super::*;` keeps private-item access. A sibling-declared `mod foo_tests;` (without `#[path]`) would make `foo_tests` a sibling of `foo` in the module tree and lose private access.
- **Compound cfg gates carry over.** A source with `#[cfg(all(test, unix))]` becomes:

  ```rust
  #[cfg(all(test, unix))]
  #[path = "foo_tests.rs"]
  mod tests;
  ```

  The sidecar inherits the parent's gate; do not re-gate items inside it.

- **`mod test_support;` and other non-`#[cfg(test)]` helper modules are NOT sidecars** — they stay declared as regular submodules with their files in `foo/`.
- **Footgun.** If a sidecar `foo_tests.rs` itself declares `mod bar;` (without `#[path]`), rustc resolves `bar` relative to the sidecar's on-disk location and looks for `foo_tests/bar.rs`, *not* `foo/bar.rs`. Inline the submodule or pass an explicit `#[path]` from the sidecar.
- **Monolith policy.** `**/*_tests.*` is exempt from the 500-line cap — sidecars can hold large test suites without splitting.
- **No `xtask` CI guardrail** for inline-test regressions; the convention is enforced by docs + reviewer attention. The pre-commit `test` hook runs `cargo test --no-run --workspace --lib --locked` which compiles every sidecar — broken `#[path]` strings fail there. Do not rely on `cargo check` alone; it skips `cfg(test)` modules and will pass a misnamed `#[path]`.
- **Prefer keeping `#[cfg(test)] impl` helpers with their source type.** This is a repository layout convention, not a Rust orphan-rule restriction: inherent impls may be declared elsewhere within the defining crate when visibility permits. Preserve test gates and private-item access when moving them.
- **Block-scoped `use` semantics shift.** Inside an inline `mod tests { ... }`, `use super::X;` and similar imports are scoped to the block. After moving to a sidecar, those imports become file-scoped (still inside the same module, but visible to every test in the file). Always use `use super::*;` in sidecars — it keeps private-item access and matches the sidecar convention.
- **Directory-split footgun.** If `foo.rs` later splits into `foo/sub.rs`-style submodules (and the source moves into `foo/`), the `#[path = "foo_tests.rs"]` string is now relative to the new source's directory, not the old one. Move the `_tests.rs` files to match, or update the `#[path]` to the correct relative location. Mitigated by the test-compile gate above, but watch for it during structural refactors.

Worked examples are common under `crates/*/src/`: pair `foo.rs` with
`foo_tests.rs`, and use additional named sidecars when one source module has
multiple test modules.
