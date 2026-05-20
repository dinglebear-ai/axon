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
  local line key value

  env_file="$(resolve_axon_env_file "$repo_dir")"
  if [[ -f "$env_file" ]]; then
    while IFS= read -r line || [[ -n "$line" ]]; do
      # Strip a UTF-8 BOM on the first line, ignore blanks/comments, and allow
      # optional `export KEY=value` syntax without evaluating shell metacharacters.
      line="${line#$'\ufeff'}"
      [[ "$line" =~ ^[[:space:]]*$ ]] && continue
      [[ "$line" =~ ^[[:space:]]*# ]] && continue
      line="${line#export }"
      [[ "$line" == *"="* ]] || continue

      key="${line%%=*}"
      value="${line#*=}"
      key="${key#"${key%%[![:space:]]*}"}"
      key="${key%"${key##*[![:space:]]}"}"
      [[ "$key" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || continue

      value="${value#"${value%%[![:space:]]*}"}"
      value="${value%"${value##*[![:space:]]}"}"
      if [[ "${value:0:1}" == '"' && "${value: -1}" == '"' ]]; then
        value="${value:1:${#value}-2}"
      elif [[ "${value:0:1}" == "'" && "${value: -1}" == "'" ]]; then
        value="${value:1:${#value}-2}"
      fi

      export "$key=$value"
    done < "$env_file"
  fi
}
