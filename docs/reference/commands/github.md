# axon github (removed — use `axon ingest`)

Last Modified: 2026-03-10

> **This command has been replaced.** Use [`axon ingest`](ingest.md) instead.
>
> `axon ingest` auto-detects the source type. GitHub slugs and URLs are recognized automatically.

## Migration

```bash
# Before
axon github rust-lang/rust
axon github rust-lang/rust --wait true
axon github tokio-rs/tokio --include-source true

# After (source code is now included by default)
axon ingest rust-lang/rust
axon ingest rust-lang/rust --wait true
axon ingest tokio-rs/tokio                          # source included by default
axon ingest tokio-rs/tokio --no-source              # to skip source code
```

See [`docs/reference/commands/ingest.md`](ingest.md) for full reference.

> For implementation details and troubleshooting see [`docs/guides/ingest/github.md`](../../guides/ingest/github.md).
