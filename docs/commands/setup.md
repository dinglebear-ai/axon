# axon setup

First-run and remote SSH deployment helpers.

## Synopsis

```bash
axon setup targets [--json]
axon setup deploy <ssh-alias> [--remote-dir axon-deploy] [--accept-new-host-key] [--public-exposure] [--json]
```

## Subcommands

| Subcommand | Purpose |
|------------|---------|
| `targets` | List concrete SSH targets from `~/.ssh/config`. |
| `deploy <ssh-alias>` | Deploy Qdrant, TEI, and Chrome services to a remote SSH target. |

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

`targets` prints SSH aliases with resolved host/user/port values. `deploy`
prints the remote host, remote directory, generated service URLs, config path,
optional tunnel command, and per-step status.
