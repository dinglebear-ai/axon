# config
Last Modified: 2026-06-01

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
| `provider <sub>` | Manage saved LLM provider/model profiles (see below). |

## Provider profiles

Saved backend+model profiles let you switch the LLM provider (codex ↔ gemini ↔
openai-compat) without editing env vars. Profiles live under `[providers.<name>]`
in `config.toml`.

| Subcommand | Description |
|------------|-------------|
| `provider list` | List saved profiles and the effective active backend. |
| `provider show <name>` | Show a profile's fields (`api-key` redacted unless `--reveal`). |
| `provider use <name>` | Activate a profile (sets `[llm] active-provider`). |
| `provider add <name> <backend> [field=value …]` | Create/replace a profile. |
| `provider set <name> <field> <value>` | Set one field on a profile. |
| `provider remove <name>` | Delete a profile (clears it as active if it was). |

`<backend>` is `gemini-headless`, `openai-compat`, or `codex-app-server`. Fields:
`model`, `base-url`, `api-key` (openai-compat), `cmd`, `home`.

```bash
axon config provider add codex codex-app-server model=gpt-5.5
axon config provider add llama openai-compat base-url=http://127.0.0.1:8080/v1 model=gemma-4
axon config provider use codex          # codex is now the active backend
axon ask "..."                          # answered via codex
axon ask "..." --provider llama         # one-off override for this run
```

**Precedence:** an active profile **overrides** `AXON_LLM_BACKEND` and the other
per-backend `AXON_*` env vars (so activation actually switches the backend).
Active selection order: `--provider <name>` > `AXON_PROVIDER` > `[llm] active-provider`.
Unset profile fields fall back to the env layer (e.g. omit `api-key` to use
`AXON_OPENAI_API_KEY`). The `api-key` is stored inline in `config.toml` (keep it
chmod 600) and redacted in output.

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
