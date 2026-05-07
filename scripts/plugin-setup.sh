#!/usr/bin/env bash
# SessionStart hook — deploys axon MCP HTTP server as a systemd user service
set -euo pipefail

# ── Config from userConfig ────────────────────────────────────────────────────
API_TOKEN="${CLAUDE_PLUGIN_OPTION_API_TOKEN:?API token is required}"
QDRANT_URL="${CLAUDE_PLUGIN_OPTION_QDRANT_URL:-http://localhost:53333}"
TEI_URL="${CLAUDE_PLUGIN_OPTION_TEI_URL:-http://localhost:52000}"
COLLECTION="${CLAUDE_PLUGIN_OPTION_COLLECTION:-cortex}"
OPENAI_BASE_URL="${CLAUDE_PLUGIN_OPTION_OPENAI_BASE_URL:-}"
OPENAI_API_KEY="${CLAUDE_PLUGIN_OPTION_OPENAI_API_KEY:-}"
OPENAI_MODEL="${CLAUDE_PLUGIN_OPTION_OPENAI_MODEL:-}"
TAVILY_API_KEY="${CLAUDE_PLUGIN_OPTION_TAVILY_API_KEY:-}"
CHROME_REMOTE_URL="${CLAUDE_PLUGIN_OPTION_CHROME_REMOTE_URL:-http://localhost:6000}"
MCP_HOST="${CLAUDE_PLUGIN_OPTION_MCP_HOST:-127.0.0.1}"
MCP_PORT="${CLAUDE_PLUGIN_OPTION_MCP_PORT:-8001}"

# ── Paths ─────────────────────────────────────────────────────────────────────
ENV_FILE="${CLAUDE_PLUGIN_DATA}/axon.env"
UNIT_FILE="${HOME}/.config/systemd/user/axon-mcp.service"

# ── Helpers ───────────────────────────────────────────────────────────────────

# Returns 0 if env file was written/changed, 1 if unchanged
write_env() {
  mkdir -p "${CLAUDE_PLUGIN_DATA}"
  local new_env
  new_env=$(cat << EOF
QDRANT_URL=${QDRANT_URL}
TEI_URL=${TEI_URL}
AXON_COLLECTION=${COLLECTION}
OPENAI_BASE_URL=${OPENAI_BASE_URL}
OPENAI_API_KEY=${OPENAI_API_KEY}
OPENAI_MODEL=${OPENAI_MODEL}
TAVILY_API_KEY=${TAVILY_API_KEY}
AXON_CHROME_REMOTE_URL=${CHROME_REMOTE_URL}
AXON_MCP_HTTP_HOST=${MCP_HOST}
AXON_MCP_HTTP_PORT=${MCP_PORT}
AXON_MCP_HTTP_TOKEN=${API_TOKEN}
EOF
)
  if [[ -f "${ENV_FILE}" ]] && diff -q <(echo "${new_env}") "${ENV_FILE}" >/dev/null 2>&1; then
    return 1  # unchanged
  fi
  echo "${new_env}" > "${ENV_FILE}"
  return 0  # changed
}

setup_systemd() {
  mkdir -p "${HOME}/.config/systemd/user"

  local new_unit
  new_unit=$(cat << EOF
[Unit]
Description=axon MCP HTTP server
After=network.target

[Service]
ExecStart=${CLAUDE_PLUGIN_ROOT}/bin/axon serve mcp
EnvironmentFile=${ENV_FILE}
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF
)

  local unit_changed=false
  if ! diff -q <(echo "${new_unit}") "${UNIT_FILE}" >/dev/null 2>&1; then
    echo "${new_unit}" > "${UNIT_FILE}"
    unit_changed=true
  fi

  local env_changed=false
  write_env && env_changed=true || true

  if [[ "${unit_changed}" == "true" ]]; then
    systemctl --user daemon-reload
    systemctl --user enable --now axon-mcp
  elif [[ "${env_changed}" == "true" ]]; then
    systemctl --user restart axon-mcp
  elif ! systemctl --user is-active --quiet axon-mcp; then
    systemctl --user start axon-mcp
  fi

  echo "axon: MCP HTTP server running on ${MCP_HOST}:${MCP_PORT}"
}

link_binary() {
  mkdir -p "${HOME}/.local/bin"
  ln -sf "${CLAUDE_PLUGIN_ROOT}/bin/axon" "${HOME}/.local/bin/axon"
}

# ── Main ──────────────────────────────────────────────────────────────────────
link_binary
setup_systemd
