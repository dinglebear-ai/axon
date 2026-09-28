# axon-route design-contract maintenance

This directory documents the `axon-route` boundary: Resolve source identity, canonical URIs, authority/aliases, scopes, and deterministic adapter selection before acquisition.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-route/src/AGENTS.md) · [Crate exports](../../../../crates/axon-route/src/lib.rs)

[url normalization](../../../../docs/reference/sources/url-normalization.md) · [adapter scopes](../../../../docs/reference/sources/adapter-scopes.md) · [adding source](../../../../docs/development/adding-source.md)

## Review the actual boundary

- Keep stable canonical identity across equivalent inputs; do not mix pagination/enrichment knobs into source identity. Aliases must not require fetching provider content.

- Keep local-path containment/security declarations, remote authority rules, and bare-source handling consistent with core CLI routing. Acquisition and persistence stay outside this crate.

For implementation evidence, inspect [resolver.rs](../../../../crates/axon-route/src/resolver.rs), [router.rs](../../../../crates/axon-route/src/router.rs), [canonical.rs](../../../../crates/axon-route/src/canonical.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

route_tests, route_normalization_tests, route_validation_tests, local_capability_tests and canonical/source ID sidecars; cover alias collisions, invalid scopes, and credential-free resolution.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
