# Package Registries
Last Modified: 2026-07-15

Package registry targets are source adapter inputs.

## Inputs

Registry sources may use explicit prefixes such as npm, crates, PyPI, or other
adapter-supported package namespaces.

## Behavior

The adapter resolves package metadata, versions, README/content, repository
links, and dependency facts where available. Documents are associated with the
package source identity and generation.

## Review Points

- Preserve package name, registry, version, and canonical URL in metadata.
- Do not invent repository links when the registry does not provide them.
- Respect source limits for versions and related pages.
