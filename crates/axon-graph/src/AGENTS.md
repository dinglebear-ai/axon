# axon-graph

Persist and query evidence-backed SourceGraph nodes, edges, authority, merges, and conflicts.

## Read before changing

[source graph](../../../docs/reference/sources/source-graph.md) · [graph](../../../docs/reference/sources/graph.md) · [database schema](../../../docs/reference/runtime/database-schema.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[store.rs](store.rs) · [sqlite.rs](sqlite.rs) · [candidate.rs](candidate.rs) · [node.rs](node.rs) · [edge.rs](edge.rs) · [evidence.rs](evidence.rs) · [authority.rs](authority.rs) · [merge.rs](merge.rs) · [migration.rs](migration.rs)

## Change requirements

- Consume GraphCandidate facts; source parsing and acquisition stay outside graph persistence. Do not invent relationships without evidence or explicit authority.

- Preserve provenance, confidence, source/generation filtering, and explicit conflict resolution. Candidate replay and upserts must be idempotent.

- Keep closed node/edge/evidence registries and schema fixtures aligned. Verify changes through SQLite as well as fake-store behavior.

- Append migrations and test existing-state upgrades; graph cleanup must not destroy current-generation evidence when older generations expire.

## Verification for code changes

store_tests, fixture_tests, schema_fixture_tests and SQLite/merge sidecars; exercise conflicting evidence, duplicate candidates, source filtering, and generation cleanup.

Use focused `cargo test -p axon-graph` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
