# axon-route

Resolve source identity, canonical URIs, authority/aliases, scopes, and deterministic adapter selection before acquisition.

## Read before changing

[url normalization](../../../docs/reference/sources/url-normalization.md) · [adapter scopes](../../../docs/reference/sources/adapter-scopes.md) · [adding source](../../../docs/development/adding-source.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[resolver.rs](resolver.rs) · [router.rs](router.rs) · [canonical.rs](canonical.rs) · [source_id.rs](source_id.rs) · [scope.rs](scope.rs) · [capability.rs](capability.rs) · [local_path.rs](local_path.rs) · [github.rs](github.rs)

## Change requirements

- Keep stable canonical identity across equivalent inputs; do not mix pagination/enrichment knobs into source identity. Aliases must not require fetching provider content.

- Validate declared schemes/scopes/capabilities before execution. Ambiguous inputs need actionable alternatives, not a silent provider guess.

- skills.sh aliases route to the bounded registry/API catalog; authentication is declared here, not read or logged. Optional audit limits do not change canonical mode identity.

- Keep local-path containment/security declarations, remote authority rules, and bare-source handling consistent with core CLI routing. Acquisition and persistence stay outside this crate.

## Verification for code changes

route_tests, route_normalization_tests, route_validation_tests, local_capability_tests and canonical/source ID sidecars; cover alias collisions, invalid scopes, and credential-free resolution.

Use focused `cargo test -p axon-route` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
