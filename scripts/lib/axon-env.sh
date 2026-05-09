#!/usr/bin/env bash

resolve_axon_env_file() {
  local repo_dir="$1"
  local axon_home="${AXON_HOME:-$HOME/.axon}"

  if [[ -n "${AXON_ENV_FILE:-}" ]]; then
    printf '%s\n' "$AXON_ENV_FILE"
    return 0
  fi

  if [[ -f "${axon_home}/.env" ]]; then
    printf '%s\n' "${axon_home}/.env"
    return 0
  fi

  printf '%s\n' "${repo_dir}/.env"
}

load_axon_env_file() {
  local repo_dir="$1"
  local env_file

  env_file="$(resolve_axon_env_file "$repo_dir")"
  if [[ -f "$env_file" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$env_file"
    set +a
  fi
}
