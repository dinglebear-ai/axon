# syntax=docker/dockerfile:1.7

# ── Builder ───────────────────────────────────────────────────────────────────
FROM rust:1.83-bookworm AS builder

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
#   curl            — healthcheck
#   tini            — PID 1 reaper for clean signal handling
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libsqlite3-0 \
        curl \
        tini \
    && rm -rf /var/lib/apt/lists/*

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
    AXON_DATA_DIR=/home/axon/.axon \
    RUST_LOG=info

EXPOSE 8001

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/axon"]
CMD ["serve"]
