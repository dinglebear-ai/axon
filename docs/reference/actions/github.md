# axon github (removed — use `axon <source>`)

Last Modified: 2026-07-14

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | <code>axon &lt;source&gt;</code> |
| REST | <code>GET /v1/sources</code><br><code>GET /v1/sources/{source_id}</code><br><code>POST /v1/sources</code> |
| MCP atomic tools | <code>source</code> |
| Shared service ownership | [axon-services](../../../crates/axon-services/src/lib.rs) and the owning domain crate; see [crate ownership](../../architecture/crate-ownership.md) |

MCP names describe the atomic projection. The legacy `axon` tool uses the corresponding action/subaction selectors; `both` exposes both projections. Discover the running server before calling. [MCP contract](../mcp/tool-schema.md) owns exact schemas and selectors.

Family-level navigation does not imply identical suboperations or request shapes across transports. Source-specific guide, not a dedicated provider command. Use unified source acquisition.
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
