# Axon Feature Flags

Last reviewed: 2026-09-29

Cargo features, dependency features, and runtime configuration are distinct.
The root [Cargo.toml](../../Cargo.toml) owns root feature declarations;
individual crate manifests and [Cargo.lock](../../Cargo.lock) own the resolved
build graph. Do not infer enabled code or an optional dependency from a
feature name alone.

## Root feature matrix

| Root feature | Declaration | Interpretation |
|---|---|---|
| `default` | Empty list | No optional root features selected by default |
| `test-helpers` | Empty list | Marker for test-only cfg selection, not production permission to bypass checks |
| `live-qdrant` | Empty list | Marker for gated live tests; the selected suite still needs provider/isolation setup |
| `quickjs` | Empty list | Placeholder; does not select `rquickjs` or establish an active JS sandbox |
| `social-verticals` | Empty list | Placeholder; does not itself enable an optional dependency |

Empty dependency lists can still be referenced by Rust cfg gates. Inspect
the code and selected test targets before claiming a feature is inert in
every context. A root marker does not enable the same-named feature in
another workspace crate without an explicit dependency mapping.

## TLS and acquisition dependencies

TLS fingerprinting and Spider acquisition are wired through their owning
crate dependencies, not a root optional TLS toggle. Inspect the locked
graph and platform build scripts for native prerequisites. Historical cold
build timings and upstream feature counts are not guarantees for the
current compiler, cache, or release runner.

[Spider feature flags](spider-feature-flags.md) distinguishes compiled flags
from runtime behavior, including currently disabled conditional ETag reuse.
The owning manifest is `crates/axon-adapters/Cargo.toml`, not a removed
`axon-crawl` crate.

## Runtime gates

`AXON_ENABLE_VERTICALS`, `AXON_AUTO_DISPATCH_SKIP`, and
`AXON_CHALLENGE_WARMUP` are runtime extractor/acquisition controls, not
Cargo feature selectors. Use [configuration](../guides/configuration.md),
the runtime consumer, and provider prerequisites to establish effective
behavior. Enabling a feature does not grant tool execution or bypass
authorization, SSRF, output bounds, or generation visibility.

## Adding an optional capability

Choose the owning domain crate first. Declare an optional dependency there
and map the feature with `dep:<dependency>` only when needed. Gate actual
code and test targets deliberately, preserving existing cfg/sidecar names.
Test enabled and disabled paths, unsupported platform/provider errors, and
release packaging.

Use [testing](../development/testing.md),
[crate ownership](../architecture/crate-ownership.md), and current CI
workflows for required gates. Update generated references after changing
manifests and preserve the matching lockfile. Do not document a placeholder
as implemented functionality before the runtime and tests exist.
