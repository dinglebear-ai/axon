# syntax=docker/dockerfile:1.7

# ── Web assets ────────────────────────────────────────────────────────────────
FROM node:24-bookworm-slim@sha256:ba849c60be29959425b8734d57b8b4b7d56f98edd9504c9af091d5281095a71e AS web-builder

WORKDIR /web
COPY apps/web/package.json apps/web/package-lock.json ./
RUN npm ci
COPY apps/web ./
COPY assets ./public/assets
RUN npm run build

# ── Builder ───────────────────────────────────────────────────────────────────
FROM rust:1.97.1-bookworm@sha256:0e2bcaef56d041a486784e54104a81aebe0da44bd03019bd70bc0401e42e4a97 AS builder

# Build deps for sqlx-sqlite (libsqlite3), reqwest+rustls (pkg-config), and
# anything that links against system libraries during cargo build.
#
# cmake, clang, and libclang-dev are for boring-sys2, which wreq pulls in: it
# builds BoringSSL from source with CMake and then generates its bindings with
# bindgen, which dlopens libclang. Nothing caught their absence for a long
# while because no build ever selected this stage -- see the dev-runtime stage
# comment below.
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config \
        libsqlite3-dev \
        ca-certificates \
        cmake \
        clang \
        libclang-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy the full workspace. Single-crate workspace at repo root, so this is
# straightforward — no Cargo workspace member juggling needed.
COPY . .
COPY --from=web-builder /web/out apps/web/out
RUN test -d apps/web/out

# The embedded web bundle is generated output. Fail closed if a stale local
# export contains secrets before RustEmbed bakes it into the binary.
RUN if [ -d apps/web/out ]; then \
        grep -RIEn \
          'AXON_.*TOKEN|NEXT_PUBLIC_.*TOKEN|OPENAI_API_KEY|TAVILY_API_KEY|REDDIT_CLIENT_SECRET|Bearer[[:space:]]+[A-Za-z0-9._~+/-]+' \
          apps/web/out; \
        status="$?"; \
        if [ "$status" -eq 0 ]; then \
          echo "secret-like content found in apps/web/out" >&2; \
          exit 1; \
        elif [ "$status" -ne 1 ]; then \
          echo "secret scan failed for apps/web/out" >&2; \
          exit "$status"; \
        fi; \
    fi

# Use BuildKit cache mounts for the Cargo registry, git index, and target
# directory. The container build is an artifact/deployment build, so use the
# workspace's container-fast profile: the production systemd service does not
# need thin/linker-plugin LTO, and shallow optimization with high codegen
# parallelism avoids multi-minute links on the four-vCPU Incus builder while
# retaining a stripped deployment binary.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo build --profile container-fast --locked --bin axon \
    && cp /build/target/container-fast/axon /usr/local/bin/axon

# ── Runtime ───────────────────────────────────────────────────────────────────
FROM node:22-bookworm-slim@sha256:83f487e0a63425e5b4d146fb5e5be574bcbe1b7b843d3ebafdd95eaf7767a7e5 AS runtime

# Runtime deps:
#   ca-certificates — TLS to qdrant/tei/openai/etc.
#   libsqlite3-0    — sqlx-sqlite runtime
#   curl            — healthcheck
#   tini            — PID 1 reaper for clean signal handling
#   nodejs (22 LTS) — inherited from the digest-pinned official Node image
#   @openai/codex   — the codex-app-server LLM backend (AXON_LLM_BACKEND=codex-app-server).
#                     Installed in-image so the container has a FRESH codex home
#                     (no host MCP servers/skills/hooks) — fast by default, and
#                     MCP-capable only when the container's own codex home is
#                     configured. docker-compose relocates that home to
#                     ~/.axon/codex via CODEX_HOME so it persists across recreates.
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libsqlite3-0 \
        curl \
        git \
        tini \
    && npm install -g @google/gemini-cli@0.41.2 @openai/codex@0.136.0 \
    && rm -rf /var/lib/apt/lists/* /root/.npm

# Non-root user with stable UID/GID 1000 — matches the host user that owns
# bind-mounted ~/.axon, avoiding permission churn on writes.
RUN groupadd --system --gid 1000 axon \
    && useradd  --system --uid 1000 --gid 1000 \
                --home-dir /home/axon --shell /usr/sbin/nologin \
                --create-home axon

COPY --from=builder /usr/local/bin/axon /usr/local/bin/axon

USER 1000:1000
WORKDIR /home/axon

# ~/.axon is the canonical config + data directory; the compose file
# bind-mounts the host's ~/.axon onto this path so config.toml, .env,
# jobs.db, output/, logs/, artifacts/ persist across container restarts.
ENV HOME=/home/axon \
    AXON_HOME=/home/axon/.axon \
    AXON_DATA_DIR=/home/axon/.axon \
    AXON_IN_CONTAINER=1 \
    AXON_MCP_HTTP_HOST=0.0.0.0 \
    CLICOLOR_FORCE=1 \
    RUST_LOG=info

EXPOSE 8001

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/axon"]
CMD ["serve"]

# ── Dev runtime ───────────────────────────────────────────────────────────────
# Runs the host-built debug binary bind-mounted by docker-compose.yaml.
#
# THIS IS THE LAST STAGE, so an untargeted `docker build` selects it. Any build
# that ships an image must pin `--target runtime`; publishing this stage gives
# a container with no binary of its own, which crash-loops on the ENTRYPOINT
# exec below for everyone who does not bind-mount a host build over it. That
# shipped once already — scripts/ci/check_dockerfile_build_targets.py now
# fails any workflow that builds this file without a target.
# Use a newer libc than the production image so the host binary can run without
# rebuilding the full container for ordinary Rust code changes.
FROM node:24-trixie-slim@sha256:50c3b2f6988dfc307b86e5301d69611af31f4789bdf232863b07d3b02fe55ae0 AS dev-runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libsqlite3-0 \
        curl \
        git \
        tini \
    && npm install -g @google/gemini-cli@0.41.2 @openai/codex@0.136.0 \
    && mkdir -p /home/axon \
    && chown 1000:1000 /home/axon \
    && rm -rf /var/lib/apt/lists/* /root/.npm

USER 1000:1000
WORKDIR /home/axon

ENV HOME=/home/axon \
    AXON_HOME=/home/axon/.axon \
    AXON_DATA_DIR=/home/axon/.axon \
    AXON_IN_CONTAINER=1 \
    AXON_MCP_HTTP_HOST=0.0.0.0 \
    CLICOLOR_FORCE=1 \
    RUST_LOG=info

EXPOSE 8001

ENTRYPOINT ["/usr/bin/tini", "--", "/home/axon/.axon/dev/axon"]
CMD ["serve"]
