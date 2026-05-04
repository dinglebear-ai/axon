#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${AXON_MCP_HTTP_PORT:-38001}"
HOST="${AXON_MCP_HTTP_HOST:-127.0.0.1}"
BASE_URL="http://${HOST}:${PORT}"

LOG_FILE="$(mktemp)"
RESP1_HEADERS="$(mktemp)"
RESP1_BODY="$(mktemp)"
RESP2_HEADERS="$(mktemp)"
RESP2_BODY="$(mktemp)"
RESP3_HEADERS="$(mktemp)"
RESP3_BODY="$(mktemp)"
RESP4_HEADERS="$(mktemp)"
RESP4_BODY="$(mktemp)"
TOKEN="${AXON_MCP_HTTP_TOKEN:-ci-mcp-token}"

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]] && kill -0 "${SERVER_PID}" 2>/dev/null; then
    kill "${SERVER_PID}" 2>/dev/null || true
    wait "${SERVER_PID}" 2>/dev/null || true
  fi
  rm -f "${LOG_FILE}" "${RESP1_HEADERS}" "${RESP1_BODY}" \
    "${RESP2_HEADERS}" "${RESP2_BODY}" \
    "${RESP3_HEADERS}" "${RESP3_BODY}" \
    "${RESP4_HEADERS}" "${RESP4_BODY}"
}
trap cleanup EXIT

wait_for_server() {
  local attempts=0
  while (( attempts < 480 )); do
    if ! kill -0 "${SERVER_PID}" 2>/dev/null; then
      echo "Server process exited before readiness check completed" >&2
      tail -n 200 "${LOG_FILE}" >&2 || true
      return 1
    fi
    if curl -fsS "${BASE_URL}/oauth/google/status" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
    attempts=$((attempts + 1))
  done
  echo "Server failed to become ready at ${BASE_URL}" >&2
  tail -n 200 "${LOG_FILE}" >&2 || true
  return 1
}

assert_status_code() {
  local expected="$1"
  local actual="$2"
  local label="$3"
  if [[ "${actual}" != "${expected}" ]]; then
    echo "${label}: expected HTTP ${expected}, got ${actual}" >&2
    tail -n 200 "${LOG_FILE}" >&2 || true
    exit 1
  fi
}

assert_not_status_code() {
  local forbidden="$1"
  local actual="$2"
  local label="$3"
  if [[ "${actual}" == "${forbidden}" ]]; then
    echo "${label}: expected a non-${forbidden} response, got ${actual}" >&2
    tail -n 200 "${LOG_FILE}" >&2 || true
    exit 1
  fi
}

(
  cd "${ROOT_DIR}"
  GOOGLE_OAUTH_CLIENT_ID="ci-smoke-client" \
  GOOGLE_OAUTH_CLIENT_SECRET="ci-smoke-secret" \
  GOOGLE_OAUTH_ALLOWED_EMAILS="ci@example.com" \
  GOOGLE_OAUTH_REDIRECT_HOST="localhost" \
  AXON_MCP_HTTP_HOST="${HOST}" \
  AXON_MCP_HTTP_PORT="${PORT}" \
  AXON_MCP_HTTP_TOKEN="${TOKEN}" \
  cargo run --quiet --bin axon -- mcp --transport http
) >"${LOG_FILE}" 2>&1 &
SERVER_PID=$!

wait_for_server

STATUS1="$(curl -sS -D "${RESP1_HEADERS}" -o "${RESP1_BODY}" -w '%{http_code}' "${BASE_URL}/mcp")"
assert_status_code "401" "${STATUS1}" "Unauthenticated /mcp request"

STATUS2="$(curl -sS -D "${RESP2_HEADERS}" -o "${RESP2_BODY}" -H 'Authorization: Bearer invalid-token' -w '%{http_code}' "${BASE_URL}/mcp")"
assert_status_code "401" "${STATUS2}" "Invalid bearer token /mcp request"

STATUS3="$(curl -sS -D "${RESP3_HEADERS}" -o "${RESP3_BODY}" -H "Authorization: Bearer ${TOKEN}" -w '%{http_code}' "${BASE_URL}/mcp")"
assert_not_status_code "401" "${STATUS3}" "Valid bearer token /mcp request reaches MCP handler"

STATUS4="$(curl -sS -D "${RESP4_HEADERS}" -o "${RESP4_BODY}" -H "x-api-key: ${TOKEN}" -w '%{http_code}' "${BASE_URL}/mcp")"
assert_not_status_code "401" "${STATUS4}" "Valid x-api-key token /mcp request reaches MCP handler"

echo "OK: /mcp enforces AXON_MCP_HTTP_TOKEN (missing/invalid rejected; bearer and x-api-key accepted)"
