---
title: "Rust Build Setup"
doc_type: "guide"
status: "active"
owner: "axon_rust"
audience:
  - "contributors"
  - "agents"
scope: "service"
source_of_truth: false
upstream_refs:
  - "https://github.com/jmagar/rmcp-template/blob/main/docs/RUST.md"
last_reviewed: "2026-05-15"
---

# Rust Build Setup

This repo follows the build conventions of the rmcp server family.
The canonical reference is [rmcp-template/docs/RUST.md](https://github.com/jmagar/rmcp-template/blob/main/docs/RUST.md).

## System prerequisites

- Rust stable ≥ 1.86 (`rustup update stable`)
- `clang` and `mold` for fast Linux builds: `apt install clang mold`
- `mingw-w64` for Windows cross-compilation: `apt install mingw-w64`
- `just` command runner (optional): `cargo install just`

## Global Cargo config

Build performance depends on `~/.cargo/config.toml` on the developer's machine.
See [rmcp-template/docs/RUST.md](https://github.com/jmagar/rmcp-template/blob/main/docs/RUST.md)
for the expected config (mold linker, profile settings, Cranelift backend).

## Local `.cargo/config.toml`

This repo's `.cargo/config.toml` contains the xtask alias and the Windows
cross-compile linker:

```toml
[alias]
xtask = "run --package xtask --"

[target.x86_64-pc-windows-gnu]
linker = "x86_64-w64-mingw32-gcc"
```

The Windows linker entry is a per-repo setting because CI environments may
not have the standard global `~/.cargo/config.toml`. All other settings
(profile tuning, mold linker for Linux) are inherited from the global config.

## Windows cross-compilation

axon_rust publishes Windows binaries via the release CI workflow. To
cross-compile locally:

```bash
rustup target add x86_64-pc-windows-gnu
cargo build --target x86_64-pc-windows-gnu --release
```
