# Config Examples
Last Modified: 2026-07-15

This page records common configuration shapes. Generated config and environment
references live beside this file.

## Local Development

Use `~/.axon/config.toml` for tuning knobs and `~/.axon/.env` for service URLs
and secrets.

## External Services

Configure Qdrant, TEI, Chrome/CDP, and LLM backends through the normal config
resolution path. CLI flags override environment values, and environment values
override TOML values.

## Review Rule

Do not add new config keys without updating generated schema/docs and migration
guidance. Removed keys must not be silently accepted as aliases.
