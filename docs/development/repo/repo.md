---
title: "Repository Structure -- Axon"
created: 2026-04-04
updated: 2026-09-29
---

# Repository Structure -- Axon

Last reviewed: 2026-09-29

## Directory tree

The maintained [repository map](../../architecture/repo-structure.md) describes
the current workspace, apps, plugins, deployment material, and tooling. The
old monolithic `src/cli`, `src/crawl`, `src/ingest`, and `src/vector` tree is
not the current layout. The root `src/` is a thin bootstrap and utility-binary
surface; domain implementations live under `crates/`.

## Runtime modules

Use [crate structure](../../architecture/crate-structure.md) for current owners
and the [generated dependency graph](../../reference/crate-dependency-graph.md)
for exact edges. [Crate ownership](../../architecture/crate-ownership.md) is
the rule for deciding where a change belongs. CLI, MCP, and HTTP call shared
services; DTOs belong in `axon-api`.

### Module layout convention (enforced)

Rust uses a module root file and a sibling directory rather than `mod.rs`:

```text
foo.rs
foo/
  bar.rs
```

`cargo xtask check-no-mod-rs` enforces the filename rule. Existing `#[path]`
declarations, especially test sidecars and explicit module boundaries, are
part of the compiled module graph. Do not delete or rename them based on the
former blanket claim that production path attributes are forbidden. Preserve
cfg gates and run the affected tests after a split.

## Root files

[AGENTS.md](../../../AGENTS.md) is canonical. Root and scoped `CLAUDE.md` /
`GEMINI.md` files are direct relative aliases to their canonical `AGENTS.md`,
not the reverse. [Documentation maintenance](../documentation.md) describes
validation and agent instruction size budgets.

[Cargo.toml](../../../Cargo.toml), [Cargo.lock](../../../Cargo.lock),
[rust-toolchain.toml](../../../rust-toolchain.toml),
[Justfile](../../../Justfile), and [lefthook.yml](../../../lefthook.yml) own
the workspace/toolchain/developer workflow. Do not copy stale file sizes,
recipe counts, or version strings into this navigation page.

## Docker compose files

Compose is a development/reference surface. Production Axon is native under
systemd in Incus or bare-metal Linux with external providers. Follow
[deployment](../../operations/deployment.md) and inspect the actual target
before choosing lifecycle commands. A repository file named `prod` is not
evidence of an installation's service manager.
