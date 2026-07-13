# config
Last Modified: 2026-06-01

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon config ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


Read or write entries in `~/.axon/.env` and `~/.axon/config.toml` from the command line.

## Synopsis

```bash
axon config <SUBCOMMAND> [args]
```

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `list` | List every entry from `.env` and `config.toml` (secrets redacted). |
| `get <key>` | Print a single value. Auto-detects the file by key shape. |
| `set <key> <value>` | Write a value. Auto-detects the file (see routing below). |
| `unset <key>` | Remove a value from `.env` or `config.toml`. |
| `path` | Print the resolved paths to `.env` and `config.toml`. |

## Key routing

`config` auto-routes by key shape:

- **`UPPER_SNAKE_CASE`** keys → `~/.axon/.env` (service URLs, API keys, secrets).
- **`dotted.lowercase`** keys → `~/.axon/config.toml` (tuning knobs).

Override the auto-detection with `--env` or `--toml`.

## Flags

| Flag | Description |
|------|-------------|
| `--env` | Force the operation against `~/.axon/.env`. |
| `--toml` | Force the operation against `~/.axon/config.toml`. |
| `--reveal` | Show secret values instead of redacting them (`list`/`get`). |

## Usage

```bash
# Show resolved config file paths
axon config path

# List all settings (secrets redacted)
axon config list

# Read a value
axon config get QDRANT_URL
axon config get search.rrf_k

# Write a secret to .env (auto-detected by UPPER_SNAKE shape)
axon config set TAVILY_API_KEY tvly-xxxx

# Write a tuning knob to config.toml (auto-detected by dotted lowercase)
axon config set search.rrf_k 60

# Remove a value
axon config unset GITHUB_TOKEN

# Reveal a redacted secret
axon config get TAVILY_API_KEY --reveal
```

## Behavior

- Secrets are redacted by default in `list` and `get` output; pass `--reveal` to print real values.
- `set` creates the target file if it does not exist (`~/.axon/` with restrictive permissions).
- Priority at runtime is unchanged: CLI flags > env vars > `config.toml` > built-in defaults.

## See also

- [Configuration reference](../../guides/configuration.md) — full key inventory and the two-layer model.
- [`setup`](setup.md) — first-run infrastructure initialization.
