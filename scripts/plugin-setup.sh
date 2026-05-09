#!/usr/bin/env bash
# SessionStart hook — deploys axon MCP HTTP server as a systemd user service
set -euo pipefail

# ── Config from userConfig ────────────────────────────────────────────────────
API_TOKEN="${CLAUDE_PLUGIN_OPTION_API_TOKEN:?API token is required}"

# ── Paths ─────────────────────────────────────────────────────────────────────
AXON_HOME="${AXON_HOME:-${HOME}/.axon}"
ENV_FILE="${AXON_HOME}/.env"
UNIT_FILE="${HOME}/.config/systemd/user/axon-mcp.service"

# ── Helpers ───────────────────────────────────────────────────────────────────

# Returns 0 if env file was written/changed, 1 if unchanged
write_env() {
  mkdir -p "${AXON_HOME}"
  chmod 700 "${AXON_HOME}" 2>/dev/null || true
  existing_env_var() {
    local key="$1"
    [[ -f "${ENV_FILE}" ]] || return 0
    awk -F= -v key="${key}" '$1 == key { sub(/^[^=]*=/, ""); print; exit }' "${ENV_FILE}"
  }
  value_from_option_or_env() {
    local option_value="$1" key="$2" default_value="${3:-}"
    if [[ -n "${option_value}" ]]; then
      printf '%s\n' "${option_value}"
      return
    fi
    local existing
    existing="$(existing_env_var "${key}")"
    if [[ -n "${existing}" ]]; then
      printf '%s\n' "${existing}"
      return
    fi
    printf '%s\n' "${default_value}"
  }
  local qdrant_url tei_url collection openai_base_url openai_api_key openai_model tavily_api_key chrome_remote_url mcp_host mcp_port
  qdrant_url="$(value_from_option_or_env "${CLAUDE_PLUGIN_OPTION_QDRANT_URL:-}" QDRANT_URL "http://localhost:53333")"
  tei_url="$(value_from_option_or_env "${CLAUDE_PLUGIN_OPTION_TEI_URL:-}" TEI_URL "http://localhost:52000")"
  collection="$(value_from_option_or_env "${CLAUDE_PLUGIN_OPTION_COLLECTION:-}" AXON_COLLECTION "cortex")"
  openai_base_url="$(value_from_option_or_env "${CLAUDE_PLUGIN_OPTION_OPENAI_BASE_URL:-}" OPENAI_BASE_URL)"
  openai_api_key="$(value_from_option_or_env "${CLAUDE_PLUGIN_OPTION_OPENAI_API_KEY:-}" OPENAI_API_KEY)"
  openai_model="$(value_from_option_or_env "${CLAUDE_PLUGIN_OPTION_OPENAI_MODEL:-}" OPENAI_MODEL)"
  tavily_api_key="$(value_from_option_or_env "${CLAUDE_PLUGIN_OPTION_TAVILY_API_KEY:-}" TAVILY_API_KEY)"
  chrome_remote_url="$(value_from_option_or_env "${CLAUDE_PLUGIN_OPTION_CHROME_REMOTE_URL:-}" AXON_CHROME_REMOTE_URL "http://localhost:6000")"
  mcp_host="$(value_from_option_or_env "${CLAUDE_PLUGIN_OPTION_MCP_HOST:-}" AXON_MCP_HTTP_HOST "0.0.0.0")"
  mcp_port="$(value_from_option_or_env "${CLAUDE_PLUGIN_OPTION_MCP_PORT:-}" AXON_MCP_HTTP_PORT "8001")"
  local allowed_origins auth_mode public_url google_client_id google_client_secret admin_email
  allowed_origins="${CLAUDE_PLUGIN_OPTION_MCP_ALLOWED_ORIGINS:-$(existing_env_var AXON_MCP_ALLOWED_ORIGINS)}"
  auth_mode="${CLAUDE_PLUGIN_OPTION_AUTH_MODE:-$(existing_env_var AXON_MCP_AUTH_MODE)}"
  public_url="${CLAUDE_PLUGIN_OPTION_PUBLIC_URL:-$(existing_env_var AXON_MCP_PUBLIC_URL)}"
  google_client_id="${CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_ID:-$(existing_env_var AXON_MCP_GOOGLE_CLIENT_ID)}"
  google_client_secret="${CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_SECRET:-$(existing_env_var AXON_MCP_GOOGLE_CLIENT_SECRET)}"
  admin_email="${CLAUDE_PLUGIN_OPTION_AUTH_ADMIN_EMAIL:-$(existing_env_var AXON_MCP_AUTH_ADMIN_EMAIL)}"
  local managed_env
  managed_env=$(cat << EOF
QDRANT_URL=${qdrant_url}
TEI_URL=${tei_url}
AXON_COLLECTION=${collection}
AXON_HOME=${AXON_HOME}
AXON_DATA_DIR=${AXON_HOME}
OPENAI_BASE_URL=${openai_base_url}
OPENAI_API_KEY=${openai_api_key}
OPENAI_MODEL=${openai_model}
TAVILY_API_KEY=${tavily_api_key}
AXON_CHROME_REMOTE_URL=${chrome_remote_url}
AXON_MCP_HTTP_HOST=${mcp_host}
AXON_MCP_HTTP_PORT=${mcp_port}
AXON_MCP_HTTP_TOKEN=${API_TOKEN}
EOF
)
  [[ -n "${allowed_origins}" ]] && managed_env="${managed_env}
AXON_MCP_ALLOWED_ORIGINS=${allowed_origins}"
  [[ -n "${auth_mode}" ]] && managed_env="${managed_env}
AXON_MCP_AUTH_MODE=${auth_mode}"
  [[ -n "${public_url}" ]] && managed_env="${managed_env}
AXON_MCP_PUBLIC_URL=${public_url}"
  [[ -n "${google_client_id}" ]] && managed_env="${managed_env}
AXON_MCP_GOOGLE_CLIENT_ID=${google_client_id}"
  [[ -n "${google_client_secret}" ]] && managed_env="${managed_env}
AXON_MCP_GOOGLE_CLIENT_SECRET=${google_client_secret}"
  [[ -n "${admin_email}" ]] && managed_env="${managed_env}
AXON_MCP_AUTH_ADMIN_EMAIL=${admin_email}"

  local managed_keys=(
    QDRANT_URL TEI_URL AXON_COLLECTION AXON_HOME AXON_DATA_DIR
    OPENAI_BASE_URL OPENAI_API_KEY OPENAI_MODEL TAVILY_API_KEY
    AXON_CHROME_REMOTE_URL AXON_MCP_HTTP_HOST AXON_MCP_HTTP_PORT
    AXON_MCP_HTTP_TOKEN AXON_MCP_ALLOWED_ORIGINS AXON_MCP_AUTH_MODE
    AXON_MCP_PUBLIC_URL AXON_MCP_GOOGLE_CLIENT_ID
    AXON_MCP_GOOGLE_CLIENT_SECRET AXON_MCP_AUTH_ADMIN_EMAIL
  )
  local preserved_env=""
  if [[ -f "${ENV_FILE}" ]]; then
    preserved_env="$(
      awk -F= -v keys="${managed_keys[*]}" '
        BEGIN {
          split(keys, key_list, " ")
          for (i in key_list) managed[key_list[i]] = 1
        }
        /^[[:space:]]*($|#)/ { print; next }
        {
          key = $1
          sub(/^[[:space:]]+/, "", key)
          sub(/[[:space:]]+$/, "", key)
          if (!(key in managed)) print
        }
      ' "${ENV_FILE}"
    )"
  fi

  local new_env="${managed_env}"
  if [[ -n "${preserved_env}" ]]; then
    new_env="${preserved_env}
${managed_env}"
  fi
  if [[ -f "${ENV_FILE}" ]] && diff -q <(echo "${new_env}") "${ENV_FILE}" >/dev/null 2>&1; then
    return 1  # unchanged
  fi
  umask 077
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

  local effective_host effective_port
  effective_host="$(awk -F= '$1 == "AXON_MCP_HTTP_HOST" { sub(/^[^=]*=/, ""); print; exit }' "${ENV_FILE}")"
  effective_port="$(awk -F= '$1 == "AXON_MCP_HTTP_PORT" { sub(/^[^=]*=/, ""); print; exit }' "${ENV_FILE}")"
  echo "axon: MCP HTTP server running on ${effective_host:-0.0.0.0}:${effective_port:-8001}"
}

link_binary() {
  mkdir -p "${HOME}/.local/bin"
  ln -sf "${CLAUDE_PLUGIN_ROOT}/bin/axon" "${HOME}/.local/bin/axon"
}

# ── Main ──────────────────────────────────────────────────────────────────────
link_binary
setup_systemd
