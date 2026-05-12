# axon setup

First-run local Docker setup and remote Docker deployment helpers.
Local setup creates `~/.axon`, writes shared config/env files, installs compose assets, starts the Docker stack, checks health, prewarms TEI, and runs first-run smoke checks.

## Synopsis

```bash
axon setup [--json]
axon setup check [--json]
axon setup repair [--json]
axon setup targets [--json]
axon setup deploy <ssh-alias> [--remote-dir axon-deploy] [--accept-new-host-key] [--public-exposure] [--json]
```

## Subcommands

| Subcommand | Purpose |
|------------|---------|
| none | Create or repair local `~/.axon` config, install compose assets, start the Docker stack, and run first-run checks. |
| `check` | Inspect local setup without mutating files or starting services. |
| `repair` | Repair local config/assets and restart the Docker stack. |
| `targets` | List concrete SSH targets from `~/.ssh/config`. |
| `deploy <ssh-alias>` | Deploy Docker Compose assets and services to a remote SSH target. |

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--remote-dir <dir>` | `axon-deploy` | Remote directory under `$HOME` for compose assets. |
| `--accept-new-host-key` | `false` | Accept and add a new SSH host key on first connection. |
| `--public-exposure` | `false` | Bind remote service ports publicly instead of loopback-only. |
| `--json` | `false` | Print machine-readable JSON output. |

## Examples

```bash
axon setup targets
axon setup targets --json
axon setup deploy gpu-box --remote-dir axon-deploy --accept-new-host-key
```

## Output

Local setup prints phase status for config, Docker, Qdrant, TEI, Chrome, Axon
server health, TEI prewarm, and smoke checks. `targets` prints SSH aliases with
resolved host/user/port values. `deploy` prints the remote host, remote
directory, generated service URLs, config path, optional tunnel command, and
per-step status.
