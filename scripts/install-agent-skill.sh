#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
SOURCE_SKILL_DIR="${REPO_ROOT}/plugins/axon/skills"

if [[ ! -d "${SOURCE_SKILL_DIR}" ]]; then
  echo "[error] source skill directory not found: ${SOURCE_SKILL_DIR}" >&2
  exit 1
fi

install_for_root() {
  local root_dir="$1"
  local target_skills_dir="${root_dir}/skills"
  local target_skill_dir="${target_skills_dir}"

  if [[ ! -d "${root_dir}" ]]; then
    echo "[skip] ${root_dir} not found"
    return
  fi

  mkdir -p "${target_skills_dir}" "${target_skill_dir}"
  cp -a "${SOURCE_SKILL_DIR}/." "${target_skill_dir}/"
  echo "[ok] installed to ${target_skill_dir}"
}

install_for_root "${HOME}/.claude"
install_for_root "${HOME}/.codex"
install_for_root "${HOME}/.gemini"

