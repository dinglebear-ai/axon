#!/usr/bin/env bash
# Claude Code SessionStart hook. Delegates all Axon setup work to the shared
# Docker Compose setup path; this script intentionally owns no service manager.
set -euo pipefail

AXON_HOME="${AXON_HOME:-${HOME}/.axon}"

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
  command -v axon >/dev/null 2>&1 && return 0
  printf 'axon is not installed — install it with:\n  curl -fsSL https://raw.githubusercontent.com/jmagar/axon/main/install.sh | sh\nThen run: axon setup\n' >&2
  exit 1
}

warn_stale_systemd_unit() {
  local unit="${HOME}/.config/systemd/user/axon-mcp.service"
  if [[ -e "${unit}" ]]; then
    printf 'axon plugin setup: stale systemd unit detected at %s; Docker setup is canonical, remove the unit to avoid port conflicts\n' "${unit}" >&2
  fi
}

main() {
  reject_unsafe_value "CLAUDE_PLUGIN_OPTION_API_TOKEN" "${CLAUDE_PLUGIN_OPTION_API_TOKEN:-}"
  export_if_set AXON_MCP_HTTP_TOKEN CLAUDE_PLUGIN_OPTION_API_TOKEN
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
  axon setup plugin-hook "$@"
}

main "$@"
