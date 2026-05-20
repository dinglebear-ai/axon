#!/usr/bin/env bash

repo_dir="/home/jmagar/workspace/axon_rust"
axon_env_loader="${repo_dir}/scripts/lib/axon-env.sh"

if [[ -r "$axon_env_loader" ]]; then
  # shellcheck source=/home/jmagar/workspace/axon_rust/scripts/lib/axon-env.sh
  source "$axon_env_loader"
  load_axon_env_file "$repo_dir"
fi
