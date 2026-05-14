#!/usr/bin/env bash
# Claude Code SessionStart hook. Delegates all Axon setup work to the shared
# Docker Compose setup path; this script intentionally owns no service manager.
set -euo pipefail

AXON_HOME="${AXON_HOME:-${HOME}/.axon}"
INSTALL_URL="${AXON_INSTALL_URL:-https://raw.githubusercontent.com/jmagar/axon/v1.10.1/install.sh}"

reject_unsafe_value() {
  local name="$1" value="${2:-}"
  if [[ "${value}" == *$'\n'* || "${value}" == *$'\r'* ]]; then
    printf 'axon plugin setup: %s must not contain newlines\n' "${name}" >&2
    exit 2
  fi
}

export_if_set() {
  local env_name="$1" option_name="$2" value
  value="$(printenv "${option_name}" || true)"
  reject_unsafe_value "${option_name}" "${value}"
  [[ -n "${value}" ]] || return 0
  export "${env_name}=${value}"
}

ensure_axon_binary() {
  if command -v axon >/dev/null 2>&1; then
    return 0
  fi
  printf 'axon plugin setup: axon not found; running installer %s\n' "${INSTALL_URL}" >&2
  curl -fsSL "${INSTALL_URL}" | sh
  export PATH="${HOME}/.local/bin:${PATH}"
  command -v axon >/dev/null 2>&1 || {
    printf 'axon plugin setup: installer completed but axon is still not on PATH\n' >&2
    exit 1
  }
}

warn_stale_systemd_unit() {
  local unit="${HOME}/.config/systemd/user/axon-mcp.service"
  if [[ -e "${unit}" ]]; then
    printf 'axon plugin setup: stale systemd unit detected at %s; Docker setup is canonical, remove the unit to avoid port conflicts\n' "${unit}" >&2
  fi
}

run_setup() {
  if axon setup check; then
    return 0
  fi

  if axon setup repair; then
    return 0
  fi

  printf 'axon plugin setup: setup repair reported failed phases; continuing so SessionStart is non-blocking\n' >&2
  axon setup check || true
}

main() {
  reject_unsafe_value "CLAUDE_PLUGIN_OPTION_API_TOKEN" "${CLAUDE_PLUGIN_OPTION_API_TOKEN:-}"
  export_if_set AXON_MCP_HTTP_TOKEN CLAUDE_PLUGIN_OPTION_API_TOKEN
  export_if_set AXON_SERVER_URL CLAUDE_PLUGIN_OPTION_SERVER_URL
  export_if_set TAVILY_API_KEY CLAUDE_PLUGIN_OPTION_TAVILY_API_KEY
  export_if_set GITHUB_TOKEN CLAUDE_PLUGIN_OPTION_GITHUB_TOKEN
  export_if_set REDDIT_CLIENT_ID CLAUDE_PLUGIN_OPTION_REDDIT_CLIENT_ID
  export_if_set REDDIT_CLIENT_SECRET CLAUDE_PLUGIN_OPTION_REDDIT_CLIENT_SECRET
  export_if_set AXON_MCP_AUTH_MODE CLAUDE_PLUGIN_OPTION_AUTH_MODE
  export_if_set AXON_MCP_PUBLIC_URL CLAUDE_PLUGIN_OPTION_PUBLIC_URL
  export_if_set AXON_MCP_GOOGLE_CLIENT_ID CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_ID
  export_if_set AXON_MCP_GOOGLE_CLIENT_SECRET CLAUDE_PLUGIN_OPTION_GOOGLE_CLIENT_SECRET
  export_if_set AXON_MCP_AUTH_ADMIN_EMAIL CLAUDE_PLUGIN_OPTION_AUTH_ADMIN_EMAIL

  mkdir -p "${AXON_HOME}"
  chmod 700 "${AXON_HOME}" 2>/dev/null || true
  warn_stale_systemd_unit
  ensure_axon_binary
  run_setup
}

main "$@"
