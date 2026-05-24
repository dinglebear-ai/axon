#!/usr/bin/env bash
# Wall-clock wrapper. Kills the wrapped command after N seconds.
#
# Usage: with_timeout.sh <seconds> -- <command> [args...]
#
# Uses GNU `timeout` (Linux) or `gtimeout` (macOS via coreutils) when
# available. Falls back to a backgrounded watchdog so the wrapper still
# enforces the limit on systems with neither installed.
#
# Exit codes:
#   0       command succeeded within the budget
#   124     command exceeded the budget and was killed
#   other   command's own exit code

set -euo pipefail

if [ "$#" -lt 3 ] || [ "$2" != "--" ]; then
    echo "usage: $0 <seconds> -- <command> [args...]" >&2
    exit 2
fi

secs="$1"
shift 2

run_with_coreutils() {
    local rc
    set +e
    "$1" "${secs}" "${@:2}"
    rc=$?
    set -e
    if [ "${rc}" -eq 124 ]; then
        echo "with_timeout: '${*:2}' exceeded ${secs}s budget — killed" >&2
    fi
    exit "${rc}"
}

if command -v timeout >/dev/null 2>&1; then
    run_with_coreutils timeout "$@"
fi

if command -v gtimeout >/dev/null 2>&1; then
    run_with_coreutils gtimeout "$@"
fi

# Manual fallback: poll once per second. Avoids the subshell-sleep
# orphan footgun where killing a subshell watchdog leaves its
# `sleep $secs` child reparented to init for the full budget. With a
# poll loop we own the timing, so an early-finishing command exits
# cleanly without leaking long-lived sleeps.
"$@" &
cmd_pid=$!

deadline=$(( $(date +%s) + secs ))
while kill -0 "${cmd_pid}" 2>/dev/null; do
    if [ "$(date +%s)" -ge "${deadline}" ]; then
        echo "with_timeout: '$*' exceeded ${secs}s budget — killed" >&2
        kill -TERM "${cmd_pid}" 2>/dev/null || true
        sleep 1
        kill -KILL "${cmd_pid}" 2>/dev/null || true
        wait "${cmd_pid}" 2>/dev/null || true
        exit 124
    fi
    sleep 1
done

set +e
wait "${cmd_pid}"
rc=$?
set -e

# If the command was killed by SIGTERM (143) attribute it to the timeout.
if [ "${rc}" -eq 143 ]; then
    rc=124
fi
exit "${rc}"
