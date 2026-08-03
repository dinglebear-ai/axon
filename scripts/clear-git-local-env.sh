#!/bin/sh
# Run a subprocess without repository-local variables exported by Git hooks.
#
# Hooks need variables such as GIT_DIR and GIT_INDEX_FILE while selecting staged
# files. Descendants that operate on a different repository must not inherit
# them: `git init /some/other/path`, for example, otherwise targets the hook's
# repository and can rewrite its shared config instead of creating that path.

set -eu

if [ "$#" -eq 0 ]; then
    echo "usage: $0 <command> [args...]" >&2
    exit 2
fi

git_local_env_names=$(git rev-parse --local-env-vars)
for git_local_env_name in $git_local_env_names; do
    unset "$git_local_env_name"
done
unset git_local_env_name git_local_env_names

exec "$@"
