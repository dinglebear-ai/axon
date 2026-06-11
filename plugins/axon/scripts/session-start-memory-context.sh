#!/usr/bin/env bash
# Best-effort Claude Code SessionStart recall for Axon persistent memory.
# This hook must never block or fail session startup.
set -uo pipefail

PLUGIN_ROOT="${CLAUDE_PLUGIN_ROOT:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)}"
TIMEOUT_SECS="${AXON_SESSION_MEMORY_TIMEOUT_SECS:-4}"
TOKEN_BUDGET="${AXON_SESSION_MEMORY_TOKEN_BUDGET:-1200}"
LIMIT="${AXON_SESSION_MEMORY_LIMIT:-6}"

is_disabled() {
  case "${AXON_SESSION_MEMORY_CONTEXT:-1}" in
    0|false|FALSE|no|NO|off|OFF) return 0 ;;
    *) return 1 ;;
  esac
}

find_axon() {
  if [[ -n "${AXON_BIN:-}" && -x "${AXON_BIN}" ]]; then
    printf '%s\n' "${AXON_BIN}"
    return 0
  fi
  if command -v axon >/dev/null 2>&1; then
    command -v axon
    return 0
  fi
  if [[ -x "${PLUGIN_ROOT}/bin/axon" ]]; then
    printf '%s\n' "${PLUGIN_ROOT}/bin/axon"
    return 0
  fi
  return 1
}

read_hook_stdin() {
  timeout 1 cat 2>/dev/null || true
}

json_field() {
  local key="$1" payload="$2"
  printf '%s\n' "${payload}" \
    | tr '\n' ' ' \
    | sed -nE "s/.*\"${key}\"[[:space:]]*:[[:space:]]*\"([^\"]*)\".*/\\1/p" \
    | head -n 1
}

first_existing_dir() {
  local candidate
  for candidate in "$@"; do
    [[ -n "${candidate}" && -d "${candidate}" ]] || continue
    printf '%s\n' "${candidate}"
    return 0
  done
  return 1
}

infer_repo_root() {
  local cwd="$1" root
  root="$(git -C "${cwd}" rev-parse --show-toplevel 2>/dev/null || true)"
  if [[ -n "${root}" && -d "${root}" ]]; then
    printf '%s\n' "${root}"
    return 0
  fi
  return 1
}

infer_repo_slug() {
  local root="$1" remote slug
  remote="$(git -C "${root}" remote get-url origin 2>/dev/null || true)"
  [[ -n "${remote}" ]] || return 1
  slug="$(printf '%s\n' "${remote}" \
    | sed -E 's#^git@[^:]+:##; s#^https?://[^/]+/##; s#^ssh://git@[^/]+/##; s#\.git$##')"
  printf '%s\n' "${slug}" | awk -F/ 'NF >= 2 { print $(NF-1) "/" $NF; exit }'
}

xml_attr() {
  printf '%s' "$1" | tr -c 'A-Za-z0-9._/-' '_'
}

has_nonblank_output() {
  printf '%s' "$1" | grep -q '[^[:space:]]'
}

main() {
  is_disabled && exit 0
  command -v git >/dev/null 2>&1 || exit 0
  command -v timeout >/dev/null 2>&1 || exit 0

  local axon_bin payload stdin_cwd cwd repo_root project repo query output status
  axon_bin="$(find_axon)" || exit 0
  payload="$(read_hook_stdin)"
  stdin_cwd="$(json_field cwd "${payload}")"
  [[ -n "${stdin_cwd}" ]] || stdin_cwd="$(json_field working_directory "${payload}")"
  [[ -n "${stdin_cwd}" ]] || stdin_cwd="$(json_field project_dir "${payload}")"

  cwd="$(first_existing_dir \
    "${CLAUDE_PROJECT_DIR:-}" \
    "${CLAUDE_WORKING_DIRECTORY:-}" \
    "${CLAUDE_CWD:-}" \
    "${stdin_cwd}" \
    "${PWD:-}")" || exit 0
  repo_root="$(infer_repo_root "${cwd}")" || exit 0
  project="$(basename "${repo_root}")"
  repo="$(infer_repo_slug "${repo_root}" || true)"
  query="${AXON_SESSION_MEMORY_QUERY:-}"

  local cmd=("${axon_bin}" memory context --project "${project}" --token-budget "${TOKEN_BUDGET}" --limit "${LIMIT}")
  [[ -z "${repo}" ]] || cmd+=(--repo "${repo}")
  [[ -z "${query}" ]] || cmd+=(--query "${query}")

  output="$(cd "${repo_root}" && timeout "${TIMEOUT_SECS}" "${cmd[@]}" 2>/dev/null)"
  status=$?
  [[ ${status} -eq 0 ]] || exit 0
  has_nonblank_output "${output}" || exit 0

  printf '<axon_session_memory_context source="session_start" project="%s"' "$(xml_attr "${project}")"
  if [[ -n "${repo}" ]]; then
    printf ' repo="%s"' "$(xml_attr "${repo}")"
  fi
  printf ' trust="evidence_only">\n'
  printf '%s\n' "${output}"
  printf '</axon_session_memory_context>\n'
}

main "$@"
