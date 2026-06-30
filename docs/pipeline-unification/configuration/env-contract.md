# Environment Contract
Last Modified: 2026-06-30

## Contract

`.env` is for bootstrap values that must be environment variables: service URLs,
secrets, auth/runtime paths, and Docker Compose interpolation. It is not the
place for normal tuning knobs.

The desired `.env` must be short enough to understand at first boot. Power-user
tuning belongs in `config.toml`.

## Design Rules

- `.env` contains URLs, secrets, runtime/bootstrap paths, and compose-only
  interpolation.
- `.env` does not contain normal pipeline tuning knobs.
- Empty values are allowed when defaults or disabled features are clear.
- Secrets are redacted in doctor/status/debug.
- `.env` can boot a minimal local Axon with Qdrant, TEI, and one LLM backend.
- Docker-only variables are isolated in a compose section.
- Any env key with a TOML equivalent must be documented as an override, not the
  preferred location.

## Required Shape

Target `.env.example` sections:

```text
# Core paths and service URLs
# Server/auth
# Provider secrets
# LLM runtime bootstrap
# Optional ingest credentials
# Logging/bootstrap overrides
# Docker Compose interpolation
```

## Core Paths and Service URLs

| Key | Required | Secret | Meaning |
|---|---:|---:|---|
| `AXON_DATA_DIR` | no | no | Data root; default `~/.axon`. |
| `AXON_HOME` | no | no | Optional home/config root override. |
| `QDRANT_URL` | yes | no | Runtime Qdrant endpoint. |
| `TEI_URL` | yes | no | Runtime embedding endpoint. |
| `AXON_CHROME_REMOTE_URL` | no | no | Chrome management/render endpoint. |
| `AXON_CONFIG_PATH` | no | no | Optional config.toml path override. |

`AXON_COLLECTION` should move to `config.toml` as the default collection. It may
remain an env override for ad hoc smokes and compose, but it is not part of the
minimal human `.env`.

## Server and Auth

| Key | Required | Secret | Meaning |
|---|---:|---:|---|
| `AXON_HTTP_HOST` | no | no | Unified server bind host. |
| `AXON_HTTP_PORT` | no | no | Unified server bind port. |
| `AXON_PUBLIC_URL` | oauth/public | no | Public URL for OAuth/callback metadata. |
| `AXON_HTTP_TOKEN` | bearer mode | yes | Static bearer token. |
| `AXON_AUTH_MODE` | no | no | `none`, `bearer`, `oauth`; safe defaults by bind. |
| `AXON_GOOGLE_CLIENT_ID` | oauth | no | OAuth client id. |
| `AXON_GOOGLE_CLIENT_SECRET` | oauth | yes | OAuth client secret. |
| `AXON_AUTH_ADMIN_EMAIL` | oauth | no | Admin/allowlist seed. |
| `AXON_ALLOWED_REDIRECT_URIS` | oauth | no | Comma-separated redirect allowlist. |
| `AXON_ALLOWED_ORIGINS` | no | no | CORS origins. |

Target naming uses unified `AXON_*` names when the setting applies to the
unified server. Transport-specific names are removed from the desired
end-state schema.

## Provider Secrets

| Key | Required | Secret | Meaning |
|---|---:|---:|---|
| `TAVILY_API_KEY` | when Tavily enabled | yes | External search fallback. |
| `AXON_OPENAI_API_KEY` | when OpenAI-compatible backend needs it | yes | OpenAI-compatible LLM secret. |
| `GEMINI_API_KEY` | optional | yes | Gemini API-key auth. |
| `GOOGLE_API_KEY` | optional | yes | Gemini-compatible alias. |
| `GITHUB_TOKEN` | private/high-rate GitHub | yes | GitHub API/token. |
| `GITLAB_TOKEN` | private/high-rate GitLab | yes | GitLab token. |
| `GITEA_TOKEN` | private/high-rate Gitea/Forgejo | yes | Gitea token. |
| `REDDIT_CLIENT_ID` | Reddit | yes-ish | Reddit OAuth id. |
| `REDDIT_CLIENT_SECRET` | Reddit | yes | Reddit OAuth secret. |
| `HF_TOKEN` | private HF model | yes | Hugging Face token. |

Non-secret provider choices such as model names and concurrency belong in
`config.toml`.

## LLM Runtime Bootstrap

These stay in `.env` only when they are host/runtime/auth specific:

| Key | Meaning |
|---|---|
| `AXON_LLM_BACKEND` | Selected backend; may also be TOML if non-secret. |
| `AXON_OPENAI_BASE_URL` | OpenAI-compatible HTTP endpoint. |
| `AXON_CODEX_CMD` | Path/name for Codex binary. |
| `AXON_CODEX_HOME` | Optional Codex auth/config home. |
| `AXON_CODEX_LOAD_USER_CONFIG` | Explicit isolation escape hatch. |
| `AXON_HEADLESS_GEMINI_CMD` | Path/name for Gemini binary. |
| `AXON_HEADLESS_GEMINI_HOME` | Gemini OAuth home. |
| `GEMINI_HOME` | Docker bind source for Gemini OAuth. |

Model names, context assumptions, completion concurrency, and timeouts belong
in `config.toml`.

## Optional Ingest Endpoints

| Key | Meaning |
|---|---|
| `AXON_SEARXNG_URL` | Self-hosted SearXNG endpoint. URL belongs in env because it is deployment topology. |

`AXON_RESEARCH_FULL_CONTENT` belongs in `config.toml`; it is behavior tuning.

## Logging and Bootstrap Overrides

| Key | Meaning |
|---|---|
| `AXON_LOG_PATH` | Optional host-specific log file override. |
| `RUST_LOG` | Runtime tracing filter for debugging. |
| `NO_COLOR` | Terminal output convention. |

Normal log level/default formatting belongs in `config.toml` if Axon owns it.

## Docker Compose Interpolation

Compose-only keys may live in `.env` because Compose cannot read TOML.

Examples:

| Key | Meaning |
|---|---|
| `AXON_IMAGE` | Server image reference. |
| `AXON_HTTP_PUBLISH` | Published HTTP port. |
| `QDRANT_HTTP_PORT` | Host Qdrant HTTP port. |
| `QDRANT_GRPC_PORT` | Host Qdrant gRPC port. |
| `TEI_EMBEDDING_MODEL` | TEI container model. |
| `TEI_HTTP_PORT` | Host TEI port. |
| `NVIDIA_VISIBLE_DEVICES` | Container GPU selection. |
| `CUDA_VISIBLE_DEVICES` | CUDA GPU selection. |

Container internals such as TEI server batch limits should move to a compose
profile or compose env block, not the human runtime `.env`, unless the user is
expected to tune them directly.

## Target Minimal Example

```dotenv
AXON_DATA_DIR=
QDRANT_URL=http://127.0.0.1:53333
TEI_URL=http://127.0.0.1:52000
AXON_CHROME_REMOTE_URL=http://127.0.0.1:6000

AXON_AUTH_MODE=bearer
AXON_HTTP_TOKEN=

AXON_LLM_BACKEND=gemini-headless
AXON_HEADLESS_GEMINI_CMD=gemini
AXON_HEADLESS_GEMINI_HOME=
GEMINI_API_KEY=

TAVILY_API_KEY=
AXON_SEARXNG_URL=
GITHUB_TOKEN=
```

## Doctor Rules

`axon doctor` must report:

- missing required URLs
- unreachable Qdrant/TEI/Chrome
- selected LLM backend missing auth/runtime
- secrets present in `config.toml`
- tuning env vars that should move to TOML
- compose-only keys used outside compose when suspicious
- deprecated env keys with target replacements

## Completion Gate

The final `.env.example` is acceptable only if:

- it fits on one screen-ish for normal use
- it contains no large tuning catalog
- every secret is redacted by diagnostics
- every non-secret tuning knob has a TOML home
- a user can boot with minimal edits
