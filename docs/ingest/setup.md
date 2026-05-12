# Setup Implementation Notes

`axon setup` owns local Docker first-run and repair. It creates `~/.axon`, writes shared config/env files, installs compose assets, starts the Docker stack, checks health, prewarms TEI, and runs first-run smoke checks.

See [`docs/commands/setup.md`](../commands/setup.md) for CLI usage.
