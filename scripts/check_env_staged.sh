#!/usr/bin/env bash
# check_env_staged.sh
# Blocks commits that accidentally stage .env files containing secrets.
# .env.example is the only .env* file that should ever be committed.

set -euo pipefail

VIOLATIONS=""

while IFS= read -r f; do
    [[ -z "$f" ]] && continue
    base="$(basename "$f")"
    case "$base" in
        # Only exemption: template file that must be tracked
        .env.example) ;;
        # Block everything else: .env, .env.local, .env.production, .env.*, etc.
        .env | .env.*)
            VIOLATIONS="${VIOLATIONS}  $f\n"
            ;;
        # Block services.env (infra credentials — does not match .env* glob above)
        services.env | *.env)
            VIOLATIONS="${VIOLATIONS}  $f\n"
            ;;
    esac
done < <(git diff --cached --name-only 2>/dev/null)

if [[ -n "$VIOLATIONS" ]]; then
    echo "[env-guard] BLOCKED — staged file(s) may contain secrets:"
    printf "%b" "$VIOLATIONS"
    echo "[env-guard] Unstage with: git restore --staged <file>"
    echo "[env-guard] Only .env.example should ever be committed."
    exit 1
fi

exit 0
