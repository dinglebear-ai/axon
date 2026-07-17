# axon github (removed — use `axon <source>`)

Last Modified: 2026-07-14

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon <source>` |
| REST | `POST /v1/sources` |
| MCP | `{ "action": "source" }` |
| Service | `services::source::* via SourceRequest` |

Parity notes: Compatibility source page. Use the unified source action for CLI, REST, and MCP.
<!-- END GENERATED ACTION SURFACES -->


> **This command has been replaced.** Use the unified source command instead.
>
> `axon <source>` auto-detects the source type. GitHub slugs and URLs are
> recognized automatically.

## Migration

```bash
# Before
axon github rust-lang/rust
axon github rust-lang/rust --wait true
axon github tokio-rs/tokio --include-source true

# After (source code is now included by default)
axon rust-lang/rust
axon rust-lang/rust --wait true
axon tokio-rs/tokio                          # source included by default
axon tokio-rs/tokio --no-source              # to skip source code
```

See [`docs/pipeline-unification/foundation/source-pipeline.md`](../../pipeline-unification/foundation/source-pipeline.md)
for the shared SourceRequest contract and source-cutover shape.

> For implementation details and troubleshooting see [`docs/guides/ingest/github.md`](../../guides/ingest/github.md).
