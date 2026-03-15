# syntax=docker/dockerfile:1
#
# Four-stage build using cargo-chef for dependency layer caching + BuildKit cache mounts.
#
# Stage 0 (chef):    installs cargo-chef once with registry/git cache mounts.
# Stage 1 (planner): generates a recipe.json from Cargo.toml + lock files only.
# Stage 2 (builder): cooks deps (cache-mounted target/), then builds binary.
# Stage 3 (runtime): debian-slim + s6-overlay; only the axon binary is copied in.
#
# Build context is the axon_rust directory.
# spider_agent and its transitive path deps are injected via the "spider-agent"
# BuildKit additional_context (declared in docker-compose.yaml) so the path dep
# ../spider/spider_agent resolves correctly without widening the build context
# to the whole workspace.

# ── Stage 0: chef — pre-built cargo-chef image (no compilation needed) ────────
# lukemathwalker/cargo-chef has cargo-chef pre-installed on top of the official
# rust image, eliminating the `cargo install cargo-chef` step entirely.
FROM lukemathwalker/cargo-chef:0.1.77-rust-1.94-bookworm AS chef

# ── Stage 1: planner ──────────────────────────────────────────────────────────
FROM chef AS planner

WORKDIR /src

COPY . ./axon_rust/
COPY --from=spider-agent . ./spider/spider_agent/
COPY --from=spider-agent-types . ./spider/spider_agent_types/
COPY --from=spider-agent-html . ./spider/spider_agent_html/

WORKDIR /src/axon_rust

RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 2: builder ──────────────────────────────────────────────────────────
FROM chef AS builder

WORKDIR /src

# sccache: compiler cache. Prebuilt binary is much faster than cargo install.
# RUSTC_WRAPPER activates it (mirrors host ~/.cargo/config.toml which is not
# copied into the build context).
ARG SCCACHE_VERSION=0.9.1
RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
      amd64) sccache_arch="x86_64-unknown-linux-musl" ;; \
      arm64) sccache_arch="aarch64-unknown-linux-musl" ;; \
      *) echo "Unsupported arch: $arch"; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/sccache.tar.gz \
      "https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VERSION}/sccache-v${SCCACHE_VERSION}-${sccache_arch}.tar.gz"; \
    tar -C /usr/local/bin -xzf /tmp/sccache.tar.gz \
      --strip-components=1 "sccache-v${SCCACHE_VERSION}-${sccache_arch}/sccache"; \
    rm /tmp/sccache.tar.gz

ENV RUSTC_WRAPPER=sccache

# mold: fast linker (5-10x faster than GNU ld for the link step).
# Mirrors the host ~/.cargo/config.toml linker setting which isn't in the build context.
RUN apt-get update \
    && apt-get install -y --no-install-recommends mold \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTFLAGS="-C link-arg=-fuse-ld=mold"

# Cook dependencies first — this layer is cached as long as recipe.json is
# unchanged (i.e., Cargo.toml / Cargo.lock have not changed).
COPY --from=planner /src/axon_rust/recipe.json recipe.json
COPY --from=spider-agent . ./spider/spider_agent/
COPY --from=spider-agent-types . ./spider/spider_agent_types/
COPY --from=spider-agent-html . ./spider/spider_agent_html/

WORKDIR /src/axon_rust

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/axon_rust/target \
    --mount=type=cache,target=/root/.cache/sccache \
    cargo chef cook --release --recipe-path /src/recipe.json

# Now copy the actual source. Only this layer re-runs on source changes.
WORKDIR /src

COPY . ./axon_rust/

WORKDIR /src/axon_rust

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/axon_rust/target \
    --mount=type=cache,target=/root/.cache/sccache \
    cargo build --release --locked --bin axon \
    && cp target/release/axon /usr/local/bin/axon-release

# ── Stage 3: runtime ──────────────────────────────────────────────────────────
FROM debian:12.9-slim

ARG S6_OVERLAY_VERSION=3.2.1.0
ARG AXON_GIT_SHA=dev

LABEL org.opencontainers.image.revision="${AXON_GIT_SHA}"
LABEL io.axon.git_sha="${AXON_GIT_SHA}"
ENV AXON_IMAGE_GIT_SHA="${AXON_GIT_SHA}"

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        bash \
        ca-certificates \
        curl \
        git \
        libssl3 \
        xz-utils \
    && rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
      amd64) s6_arch="x86_64" ;; \
      arm64) s6_arch="aarch64" ;; \
      *) echo "Unsupported arch: $arch"; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/s6-overlay-noarch.tar.xz \
      "https://github.com/just-containers/s6-overlay/releases/download/v${S6_OVERLAY_VERSION}/s6-overlay-noarch.tar.xz"; \
    curl -fsSL -o /tmp/s6-overlay-noarch.tar.xz.sha256 \
      "https://github.com/just-containers/s6-overlay/releases/download/v${S6_OVERLAY_VERSION}/s6-overlay-noarch.tar.xz.sha256"; \
    curl -fsSL -o "/tmp/s6-overlay-${s6_arch}.tar.xz" \
      "https://github.com/just-containers/s6-overlay/releases/download/v${S6_OVERLAY_VERSION}/s6-overlay-${s6_arch}.tar.xz"; \
    curl -fsSL -o "/tmp/s6-overlay-${s6_arch}.tar.xz.sha256" \
      "https://github.com/just-containers/s6-overlay/releases/download/v${S6_OVERLAY_VERSION}/s6-overlay-${s6_arch}.tar.xz.sha256"; \
    cd /tmp && sha256sum -c s6-overlay-noarch.tar.xz.sha256; \
    cd /tmp && sha256sum -c "s6-overlay-${s6_arch}.tar.xz.sha256"; \
    tar -C / -Jxpf /tmp/s6-overlay-noarch.tar.xz; \
    tar -C / -Jxpf "/tmp/s6-overlay-${s6_arch}.tar.xz"; \
    rm -f "/tmp/s6-overlay-noarch.tar.xz" "/tmp/s6-overlay-noarch.tar.xz.sha256" \
          "/tmp/s6-overlay-${s6_arch}.tar.xz" "/tmp/s6-overlay-${s6_arch}.tar.xz.sha256"

WORKDIR /app

# Install yt-dlp standalone binary (no Python required) for YouTube transcript ingestion
# SHA512 checksums from: https://github.com/yt-dlp/yt-dlp/releases/download/VERSION/SHA2-512SUMS
ARG YT_DLP_VERSION=2024.12.23
ARG YT_DLP_SHA512_AMD64=252679da214cc56d4118d535786197adb1c6ce59aef6df1099b67416c34b3e68f53038b2e3cea695423de22d47379c3599377461ef5a2aa4b56a078571586531
ARG YT_DLP_SHA512_ARM64=343f0e84d0d1aaf60d9a0006e6f0c515d2af32e7b209fa725cb5d87c55ab08b51aa52b767ba08958807f1af27afb2ea2e57a3c1c6119e940e5aa8dca1de1452d
RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
      amd64) ytdlp_bin="yt-dlp_linux"; ytdlp_sha512="${YT_DLP_SHA512_AMD64}" ;; \
      arm64) ytdlp_bin="yt-dlp_linux_aarch64"; ytdlp_sha512="${YT_DLP_SHA512_ARM64}" ;; \
      *) echo "Unsupported arch: $arch"; exit 1 ;; \
    esac; \
    curl -fsSL -o /usr/local/bin/yt-dlp \
      "https://github.com/yt-dlp/yt-dlp/releases/download/${YT_DLP_VERSION}/${ytdlp_bin}"; \
    echo "${ytdlp_sha512}  /usr/local/bin/yt-dlp" | sha512sum --check; \
    chmod +x /usr/local/bin/yt-dlp

# Install ACP adapters as standalone binaries (no Node.js runtime required).
ARG CLAUDE_ACP_VERSION=0.19.2
ARG CODEX_ACP_VERSION=0.9.5
RUN set -eux; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in \
      amd64) \
        claude_asset="claude-agent-acp-linux-x64.tar.gz"; \
        codex_asset="codex-acp-${CODEX_ACP_VERSION}-x86_64-unknown-linux-gnu.tar.gz" ;; \
      arm64) \
        claude_asset="claude-agent-acp-linux-arm64.tar.gz"; \
        codex_asset="codex-acp-${CODEX_ACP_VERSION}-aarch64-unknown-linux-gnu.tar.gz" ;; \
      *) echo "Unsupported arch: $arch"; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/claude-agent-acp.tar.gz \
      "https://github.com/zed-industries/claude-agent-acp/releases/download/v${CLAUDE_ACP_VERSION}/${claude_asset}"; \
    tar -xzf /tmp/claude-agent-acp.tar.gz -C /tmp; \
    install -m 0755 /tmp/claude-agent-acp /usr/local/bin/claude-agent-acp; \
    curl -fsSL -o /tmp/codex-acp.tar.gz \
      "https://github.com/zed-industries/codex-acp/releases/download/v${CODEX_ACP_VERSION}/${codex_asset}"; \
    tar -xzf /tmp/codex-acp.tar.gz -C /tmp; \
    install -m 0755 /tmp/codex-acp /usr/local/bin/codex-acp; \
    rm -rf /tmp/claude-agent-acp.tar.gz /tmp/codex-acp.tar.gz /tmp/claude-agent-acp /tmp/codex-acp

COPY --from=builder /usr/local/bin/axon-release /usr/local/bin/axon

COPY docker/s6/cont-init.d/ /etc/cont-init.d/
COPY docker/s6/s6-rc.d/ /etc/s6-overlay/s6-rc.d/
COPY docker/scripts/healthcheck-workers.sh /usr/local/bin/healthcheck-workers.sh

# NOTE: UID 1001 must not be changed to 1000. Existing data volumes are owned
# by UID 1001; changing it would cause permission regressions requiring a manual
# chown on all volume data. If migrating from a prior image that used UID 1000,
# run: docker exec axon-workers chown -R axon:axon /var/log/axon /app /home/axon
RUN groupadd -r axon && useradd -r -g axon -u 1001 -d /home/axon -m axon \
    && chmod +x /etc/cont-init.d/* \
    && find /etc/s6-overlay/s6-rc.d -type f -name run -exec chmod +x {} + \
    && find /etc/s6-overlay/s6-rc.d -type f -name finish -exec chmod +x {} + \
    && chmod +x /usr/local/bin/healthcheck-workers.sh \
    && mkdir -p /home/axon/.claude /home/axon/.codex /home/axon/.gemini \
    && mkdir -p \
        /var/log/axon/crawl-worker \
        /var/log/axon/batch-worker \
        /var/log/axon/embed-worker \
        /var/log/axon/extract-worker \
        /var/log/axon/ingest-worker \
        /var/log/axon/claude-session \
        /var/log/axon/claude-watcher \
        /app/logs \
    && chown -R axon:axon /var/log/axon /app /home/axon

# NOTE: s6-overlay v3 requires root for /init (PID 1) to manage process
# supervision. Workers drop privileges via s6-setuidgid in their run scripts.
# Do NOT add `USER axon` here — it would break s6-overlay's /init entrypoint.

ENV AXON_ENV_FILE=/app/.env

# Healthcheck: verify workers are running AND can reach Postgres (liveness + connectivity)
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD /usr/local/bin/healthcheck-workers.sh \
        && bash -c "echo >/dev/tcp/${AXON_PG_HOST:-axon-postgres}/${AXON_PG_PORT:-5432}" 2>/dev/null

# ===========================================================================
# OPERATIONAL RUNBOOKS
# ===========================================================================
#
# PRE-MAINTENANCE QUEUE DRAIN:
# 1. Stop accepting new jobs: remove port exposure or set upstream to 503
# 2. Wait for active jobs to complete: watch `axon status` until all 'running' = 0
# 3. Or force drain: axon crawl clear && axon extract clear && axon embed clear
# 4. Shutdown workers: docker compose stop axon-workers
#
# ROLLBACK PROCEDURE (failed deploy):
# 1. docker compose stop axon-workers
# 2. docker tag axon-workers:latest axon-workers:failed
# 3. docker tag axon-workers:previous axon-workers:latest
# 4. docker compose up -d axon-workers
# 5. Verify: docker compose logs axon-workers --tail=50
# 6. Reclaim stale jobs: axon crawl recover && axon extract recover
# ===========================================================================

ENTRYPOINT ["/init"]
