# Domain Concepts

## What changed in this update

### Generated action surface contracts
`docs/reference/actions/*.md` pages include an auto-generated `Surfaces` block generated from parity data. In this update, many pages moved to generic markers like:

- `REST: Not inventoried`
- `MCP: Not exposed as a dedicated MCP action.`
- `Service: Not inventoried`

Those values come from the action surface parser when entries are missing in the current parity inventory.

### Wrapper delegation
`scripts/cargo-rustc-wrapper` now supports:

- `CARGO_BIN_ARTIFACT_WRAPPER_HELPER` / auto-discovery of `cargo-bin-artifact-wrapper`
- optional use of `sccache-wrapper`
- fallback to `sccache` and then bare rustc

The primary behavior remains: when not delegated, it still behaves as a compile wrapper and preserves the binary-install side effects expected by repo scripts.

### OpenWiki workflow control plane
The OpenWiki scheduler now calls `openwiki code --update --print` directly and updates additional paths in generated PRs.

## Why this matters

- Reduced drift: action surface documentation tracks parity assumptions in a predictable way.
- Reduced bootstrap risk: the compile wrapper can route through helper tooling when installed.
- Lower operational friction: workflow updates include workflow/docs control files in one PR payload.

## Related files

- `scripts/cargo-rustc-wrapper`
- `docs/reference/actions/*.md`
- `docs/reference/actions/README.md`
- `docs/reference/api-parity.md`
- `scripts/generate_action_docs.py`
- `.github/workflows/openwiki-update.yml`
