# syntax=docker/dockerfile:1
#
# Build context is apps/web (context: apps/web in docker-compose.yaml).
# s6-overlay supervises one service: pnpm-dev (Next.js dev server).
# PID 1 must be root (/init) — cannot use USER node with s6-overlay.
# Services drop privileges via s6-setuidgid node in their run scripts.

FROM node:24-slim

ARG S6_OVERLAY_VERSION=3.2.1.0
ARG AXON_GIT_SHA=dev

LABEL org.opencontainers.image.revision="${AXON_GIT_SHA}"
LABEL io.axon.git_sha="${AXON_GIT_SHA}"
ENV AXON_IMAGE_GIT_SHA="${AXON_GIT_SHA}"

# Install system deps + AI CLIs as root into system paths
RUN apt-get update && apt-get install -y --no-install-recommends \
        curl \
        ca-certificates \
        g++ \
        git \
        inotify-tools \
        make \
        openssh-client \
        procps \
        python3 \
        xz-utils \
    && rm -rf /var/lib/apt/lists/*

# uv + uvx — required for stdio MCP servers (e.g. mcp-neo4j-memory)
RUN curl -LsSf https://astral.sh/uv/install.sh | UV_INSTALL_DIR=/usr/local/bin sh

# s6-overlay for process supervision (pnpm-dev)
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

# Claude: installer drops a symlink into $HOME/.local/bin pointing to the real
# binary under $HOME/.local/share/claude/versions/. Move the actual binary
# (dereference the symlink) so that non-root users can execute it without
# needing to traverse /root/.local/ (which is 700).
RUN curl -fsSL https://claude.ai/install.sh | bash && \
    cp "$(readlink -f /root/.local/bin/claude)" /usr/local/bin/claude && \
    chmod 755 /usr/local/bin/claude

# Codex + Gemini CLI
RUN npm install -g @openai/codex@0.105.0 @google/gemini-cli@0.32.1 --no-fund --no-audit

# corepack ships with Node 24 — use it to install pnpm without a separate npm call
# Pin to the version recorded in pnpm-lock.yaml (lockfileVersion: 9.0 → pnpm 10.x)
RUN corepack enable && corepack prepare pnpm@10.30.3 --activate

WORKDIR /app

# Pre-create dirs that anonymous volumes will mount over.
RUN mkdir -p /app/.next /app/node_modules

# Copy dep manifests first for install-layer reuse.
COPY --chown=node:node package.json pnpm-lock.yaml pnpm-workspace.yaml ./
# Run pnpm install as node so the anonymous volume inherits node-owned files.
# pnpm-dev and Next.js auto-install both run as node; ownership must match.
# Cache the pnpm content-addressable store so re-installs after lockfile changes
# don't re-download already-fetched tarballs.
# uid=1000,gid=1000 matches the node user so pnpm (running as node) can write to the mount.
RUN --mount=type=cache,target=/home/node/.local/share/pnpm/store,uid=1000,gid=1000 \
    chown -R node:node /app && su node -s /bin/sh -c "pnpm install --frozen-lockfile"

COPY --chown=node:node . ./

# s6 service definitions injected via additional_contexts (web-s6: docker/web/s6-rc.d)
COPY --from=web-s6 . /etc/s6-overlay/s6-rc.d/

# cont-init.d scripts injected via additional_contexts (web-cont-init: docker/web/cont-init.d)
COPY --from=web-cont-init . /etc/cont-init.d/

RUN find /etc/s6-overlay/s6-rc.d -type f -name run    -exec chmod +x {} + \
    && find /etc/s6-overlay/s6-rc.d -type f -name finish -exec chmod +x {} + \
    && chmod +x /etc/cont-init.d/* \
    && mkdir -p \
        /var/log/axon/pnpm-dev \
        /var/log/axon/pnpm-watcher \
        /var/log/axon/shell-server \
    && chown -R node:node /var/log/axon

# NOTE: s6-overlay v3 requires root for /init (PID 1) to manage process
# supervision. Services drop privileges via s6-setuidgid in their run scripts.
# Do NOT add USER node here.

ENTRYPOINT ["/init"]
