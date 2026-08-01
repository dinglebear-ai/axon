#!/usr/bin/env bash
set -euo pipefail

repo_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
fixture_dir="$(mktemp -d)"
trap 'rm -rf -- "$fixture_dir"' EXIT

printf 'AXON_TEST_PRECEDENCE=file\nAXON_TEST_FROM_FILE=loaded\n' >"$fixture_dir/.env"

# shellcheck source=scripts/lib/axon-env.sh
source "$repo_dir/scripts/lib/axon-env.sh"
export AXON_ENV_FILE="$fixture_dir/.env"
export AXON_TEST_PRECEDENCE="process"
unset AXON_TEST_FROM_FILE || true
load_axon_env_file "$repo_dir"

[[ "$AXON_TEST_PRECEDENCE" == "process" ]]
[[ "$AXON_TEST_FROM_FILE" == "loaded" ]]
echo "axon env precedence: PASS"
