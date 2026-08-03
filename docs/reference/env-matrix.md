# Axon Environment Ownership Matrix

Last reviewed: 2026-08-02.

Axon deliberately separates deployment inputs from behavioral tuning:

- `.env` owns endpoint URLs, credentials, authentication, trusted bootstrap
  paths, and Docker Compose interpolation.
- `~/.axon/config.toml` owns typed, non-secret behavior and tuning.
- CLI flags override both for the current invocation.

The machine-readable ownership contract is
[`env-matrix.toml`](env-matrix.toml). The generated operator references are
[`config/env.md`](config/env.md), [`config/config-toml.md`](config/config-toml.md),
and [`config/config.schema.json`](config/config.schema.json). Do not copy the
full key inventory into another hand-maintained table.

## Ownership classes

| Class | Owner | Examples |
|---|---|---|
| `keep-env` | Runtime environment | service URLs, provider credentials, OAuth and bearer-token values |
| `compose-env` | Docker Compose interpolation | images, published ports, container names, provider service settings |
| `trusted-bootstrap` | Explicit operator-controlled environment | config/data paths, binary paths, Codex command/home overrides |
| `move-toml` | Typed `config.toml` setting | worker, search, chunking, provider, and logging tuning |
| `compat-shim` | Migration reader only | deprecated names accepted long enough to produce migration guidance |
| `delete` | No live runtime owner | removed queue, database, and source-family configuration |
| `external/test-only` | Repository tooling or a child process | build/test controls that are not Axon runtime configuration |

Secrets remain environment-owned even when a related provider has typed TOML
tuning. A value classified as `compose-env` is not automatically read by the
Rust process; Compose may consume it only while constructing a service.

## Supported LLM backend wiring

`AXON_LLM_BACKEND` selects `gemini-headless`, `openai-compat`, or
`codex-app-server`. Backend credentials and endpoint/bootstrap values stay in
the environment. Model and concurrency values documented by the generated
environment registry also remain environment overrides where registered.

`AXON_CODEX_CMD` and `AXON_CODEX_HOME` are trusted host-side bootstrap
overrides. Axon ignores generic `OPENAI_BASE_URL` and `OPENAI_MODEL`; use the
registered `AXON_OPENAI_*` names for the OpenAI-compatible backend.
`OPENAI_API_KEY` is not an Axon configuration key, though the isolated Codex
child may receive it as an authentication fallback.

## Migration and validation

`axon setup config rewrite` is the supported clean-break migration helper for
removed or relocated keys. Unknown or removed clean-break keys must produce
clear guidance instead of silently becoming runtime behavior.

Run these checks after changing configuration ownership:

```bash
python3 scripts/check-env-config-boundary.py
cargo xtask schemas generate --check
cargo xtask docs generate --check
```

Any new key must update the owning Rust registry, the machine-readable matrix,
the appropriate example file, and the generated references together. Never
place secret values in this repository or in generated documentation.
