# syntax=docker/dockerfile:1
#
# axon-chrome — headless_browser manager + chrome-headless-shell
#
# Stages:
#   builder  — compiles headless_browser from crates.io
#   runtime  — debian-slim + chrome-headless-shell + the built binary
#
# Ports:
#   6000  — headless_browser management API  (/json/version, /fork, /shutdown)
#   9222  — TCP proxy to Chrome CDP (primary connection point for spider.rs)
#   9223  — Chrome DevTools direct port
#
# ENV:
#   CHROME_PATH      — path to chrome-headless-shell binary (baked in below)
#   REMOTE_ADDRESS   — address Chrome binds to (0.0.0.0 for Docker networking)
#   HOSTNAME_OVERRIDE — rewrites the hostname in /json/version WebSocket URLs;
#                       set to the container name so other containers can connect

ARG CHROME_VERSION=134.0.6998.23

# ── Stage 1: build headless_browser ──────────────────────────────────────────
FROM rust:1.83-slim-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        make \
    && rm -rf /var/lib/apt/lists/*

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/usr/local/cargo/headless-target \
    CARGO_TARGET_DIR=/usr/local/cargo/headless-target \
    cargo install headless_browser --version 0.1.24 --locked \
    && strip /usr/local/cargo/bin/headless_browser

# ── Stage 2: runtime ─────────────────────────────────────────────────────────
FROM debian:12-slim

ARG CHROME_VERSION

# Chrome-headless-shell runtime dependencies (minimal set)
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        unzip \
        libglib2.0-0 \
        libnss3 \
        libnspr4 \
        libdbus-1-3 \
        libatk1.0-0 \
        libatk-bridge2.0-0 \
        libexpat1 \
        libatspi2.0-0 \
        libx11-6 \
        libxcomposite1 \
        libxdamage1 \
        libxext6 \
        libxfixes3 \
        libxrandr2 \
        libgbm1 \
        libdrm2 \
        libxkbcommon0 \
        libpango-1.0-0 \
        libcairo2 \
        libasound2 \
    && rm -rf /var/lib/apt/lists/*

# Keep the full archive directory — chrome-headless-shell needs icudtl.dat and
# other resource files in the same directory as the binary to start correctly.
#
# Chrome for Testing downloads are served over HTTPS from Google's CDN
# (storage.googleapis.com) with TLS certificate verification. The version is
# pinned via CHROME_VERSION build arg. Google does not publish separate hash
# files for Chrome for Testing — HTTPS + version pinning is the integrity model.
# To update: check https://googlechromelabs.github.io/chrome-for-testing/ for
# available versions.
RUN set -eux; \
    curl -fsSL -o /tmp/chrome.zip \
        "https://storage.googleapis.com/chrome-for-testing-public/${CHROME_VERSION}/linux64/chrome-headless-shell-linux64.zip"; \
    unzip -q /tmp/chrome.zip -d /opt/; \
    mv /opt/chrome-headless-shell-linux64 /opt/chrome; \
    chmod +x /opt/chrome/chrome-headless-shell; \
    rm -f /tmp/chrome.zip

COPY --from=builder /usr/local/cargo/bin/headless_browser /usr/local/bin/headless_browser

# CHROME_PATH is used as fallback when the binary is run without positional args.
# The CMD below passes the path explicitly as arg 1 (takes precedence), so this
# only matters if someone overrides the entrypoint without providing args.
ENV CHROME_PATH=/opt/chrome/chrome-headless-shell

EXPOSE 6000 9222 9223

HEALTHCHECK --interval=10s --timeout=5s --start-period=15s --retries=6 \
    CMD curl -fsSL http://127.0.0.1:6000/json/version > /dev/null || exit 1

# Args: <chrome-path> <bind-address> init
# "init" auto-starts Chrome on port 9222 at startup.
CMD ["headless_browser", "/opt/chrome/chrome-headless-shell", "0.0.0.0", "init"]
