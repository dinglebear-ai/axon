# syntax=docker/dockerfile:1.7

# ── Web assets ────────────────────────────────────────────────────────────────
FROM node:24-bookworm-slim AS web-builder

WORKDIR /web
COPY apps/web/package.json apps/web/package-lock.json ./
RUN npm ci
COPY apps/web ./
RUN npm run build

# ── Builder ───────────────────────────────────────────────────────────────────
FROM rust:1.94.0-bookworm AS builder

# Build deps for sqlx-sqlite (libsqlite3), reqwest+rustls (pkg-config), and
# anything that links against system libraries during cargo build.
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config \
        libsqlite3-dev \
        ca-certificates \
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
# directory. Keeps incremental rebuilds fast while still producing a clean
# output binary.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo build --release --locked --bin axon \
    && cp /build/target/release/axon /usr/local/bin/axon

# ── Runtime ───────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# Runtime deps:
#   ca-certificates — TLS to qdrant/tei/openai/etc.
#   libsqlite3-0    — sqlx-sqlite runtime
#   curl            — healthcheck + NodeSource setup
#   tini            — PID 1 reaper for clean signal handling
#   nodejs (22 LTS) — required to run the Gemini CLI (ask/evaluate/research synthesis)
#                     Debian bookworm ships Node 18 which is too old for @google/gemini-cli;
#                     use NodeSource for Node 22 LTS.
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libsqlite3-0 \
        curl \
        git \
        tini \
    && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && npm install -g @google/gemini-cli@0.41.2 \
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
