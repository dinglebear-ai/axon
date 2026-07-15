# Source Graph
Last Modified: 2026-07-15

The source graph records relationships between sources, documents, entities,
and extracted facts.

## Relationship Examples

- source contains document
- document links to URL
- package points to repository
- repository defines symbol
- session references file or issue
- memory supersedes memory

## Rule

Graph writes happen through graph services and source-pipeline stages, not
through transport-specific side effects.
