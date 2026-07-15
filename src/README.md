# src/
Last Modified: 2026-07-15

Root binary entrypoint for Axon.

The old root-module layout has moved into workspace crates under `crates/`.
This directory now stays intentionally small:

- `main.rs` loads the process environment and calls the library entrypoint.
- `lib.rs` re-exports `axon_cli::run`.

## Related Docs
- [Repository README](../README.md)
- [Architecture](../docs/architecture/overview.md)
- [Docs Index](../docs/README.md)
