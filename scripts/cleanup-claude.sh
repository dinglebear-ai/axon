#!/usr/bin/env bash
# cleanup-claude.sh — Kill zombie Claude Code sessions and their MCP child trees.
#
# Only kills Claude processes that are DEFINITELY dead:
#   - State 'T' (stopped/suspended via ctrl-z)
#   - Parent process is dead (orphaned to init/PID 1)
#   - Idle for longer than --max-age (default: 2 hours, 0 = disable age check)
#
# Active Zed sessions, terminal sessions, and recently-used sessions are KEPT.
#
# Usage:
#   ./scripts/cleanup-claude.sh                  # dry-run (default)
#   ./scripts/cleanup-claude.sh --kill           # actually kill
#   ./scripts/cleanup-claude.sh --cron           # kill + quiet (for cron)
#   ./scripts/cleanup-claude.sh --kill --max-age 0   # kill stopped/orphaned only, ignore age
#   ./scripts/cleanup-claude.sh --kill --max-age 60  # also kill sessions idle >60 min

set -euo pipefail

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------
readonly CRON_LOG="/tmp/cleanup-claude.log"
readonly CRON_LOG_MAX_LINES=500

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------
MODE="dry-run"
QUIET=false
MAX_AGE_MIN=120  # default: 2 hours

while [[ $# -gt 0 ]]; do
    case "$1" in
        --kill)
            MODE="--kill"
            shift
            ;;
        --cron)
            MODE="--kill"
            QUIET=true
            shift
            ;;
        --max-age)
            if [[ $# -lt 2 ]]; then
                printf 'error: --max-age requires a numeric argument (minutes)\n' >&2
                exit 1
            fi
            if ! [[ "$2" =~ ^[0-9]+$ ]]; then
                printf 'error: --max-age value must be a non-negative integer, got: %s\n' "$2" >&2
                exit 1
            fi
            MAX_AGE_MIN="$2"
            shift 2
            ;;
        *)
            printf 'Unknown arg: %s\n' "$1" >&2
            exit 1
            ;;
    esac
done

# ---------------------------------------------------------------------------
# Log rotation — truncate cron log at startup when running in cron/quiet mode
# so it never grows unboundedly.  Uses a tmp file + mv for atomic replacement.
# ---------------------------------------------------------------------------
if [[ "${QUIET}" == true ]] && [[ -f "${CRON_LOG}" ]]; then
    tmp_log=$(mktemp "${CRON_LOG}.XXXXXX")
    if tail -n "${CRON_LOG_MAX_LINES}" -- "${CRON_LOG}" > "${tmp_log}"; then
        mv -f -- "${tmp_log}" "${CRON_LOG}"
    else
        rm -f -- "${tmp_log}"
    fi
fi

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# log() — always returns 0 so set -e cannot fire on the "quiet" path.
log() {
    if [[ "${QUIET}" == false ]]; then
        echo "$@"
    fi
    return 0
}

# ps_field PID FORMAT — return a single ps field with whitespace stripped.
# Captures ps output into a variable BEFORE stripping so the || fallback is
# reachable even when ps fails (fixes the broken "ps | tr || fallback" pattern
# where tr sits between ps and ||, preventing the fallback from ever firing).
ps_field() {
    local pid="${1}" fmt="${2}" raw
    raw=$(ps -o "${fmt}=" -p "${pid}" 2>/dev/null) || raw=""
    printf '%s' "${raw//[[:space:]]/}"
}

# ---------------------------------------------------------------------------
# Collect all top-level Claude processes (the main "claude" binary, not children)
# ---------------------------------------------------------------------------
mapfile -t claude_pids < <(pgrep -x claude 2>/dev/null || true)

if [[ ${#claude_pids[@]} -eq 0 ]]; then
    log "No Claude processes found."
    exit 0
fi

keep_pids=()
kill_pids=()

# ---------------------------------------------------------------------------
# Classification pass
# ---------------------------------------------------------------------------
for pid in "${claude_pids[@]}"; do
    state=$(ps_field   "${pid}" "stat")
    tty=$(ps_field     "${pid}" "tty")
    ppid=$(ps_field    "${pid}" "ppid")
    rss_kb=$(ps_field  "${pid}" "rss")
    elapsed=$(ps_field "${pid}" "etimes")

    # Provide safe arithmetic defaults so we never operate on an empty string.
    rss_kb="${rss_kb:-0}"
    elapsed="${elapsed:-0}"

    rss_mb=$(( rss_kb / 1024 ))
    elapsed_min=$(( elapsed / 60 ))

    # Parent command — only query if ppid is non-empty and not "1".
    if [[ -z "${ppid}" ]]; then
        parent_cmd="dead"
    elif [[ "${ppid}" == "1" ]]; then
        parent_cmd="init"
    else
        parent_cmd=$(ps -o comm= -p "${ppid}" 2>/dev/null || echo "dead")
        parent_cmd="${parent_cmd//[[:space:]]/}"
    fi

    reason=""

    # KILL: stopped/suspended (ctrl-z'd) — always dead
    if [[ "${state}" == *"T"* ]]; then
        reason="stopped (ctrl-z)"
    # KILL: orphaned — parent is init (PID 1) or dead
    elif [[ "${ppid}" == "1" ]] || [[ "${parent_cmd}" == "dead" ]]; then
        reason="orphaned (parent dead)"
    # KILL: idle too long (if age check enabled)
    elif [[ "${MAX_AGE_MIN}" -gt 0 ]] && [[ "${elapsed_min}" -ge "${MAX_AGE_MIN}" ]]; then
        # Spare if it is foreground in a TTY — user is actively in it
        if [[ "${state}" == *"+"* ]]; then
            keep_pids+=("${pid}")
            log "  KEEP  PID=${pid}  state=${state}  tty=${tty}  parent=${parent_cmd}  ${rss_mb}MB  age=${elapsed_min}m  (foreground)"
            continue
        fi
        reason="idle ${elapsed_min}m (>${MAX_AGE_MIN}m)"
    fi

    if [[ -n "${reason}" ]]; then
        kill_pids+=("${pid}")
        log "  KILL  PID=${pid}  state=${state}  tty=${tty}  parent=${parent_cmd}  ${rss_mb}MB  age=${elapsed_min}m  (${reason})"
    else
        keep_pids+=("${pid}")
        log "  KEEP  PID=${pid}  state=${state}  tty=${tty}  parent=${parent_cmd}  ${rss_mb}MB  age=${elapsed_min}m"
    fi
done

if [[ ${#kill_pids[@]} -eq 0 ]]; then
    log "Nothing to clean up. ${#keep_pids[@]} active session(s)."
    exit 0
fi

log ""
log "Keeping ${#keep_pids[@]} active session(s), killing ${#kill_pids[@]} zombie(s)."

if [[ "${MODE}" != "--kill" ]]; then
    log ""
    log "Dry run — re-run with --kill to execute."
    exit 0
fi

# ---------------------------------------------------------------------------
# Kill pass
# ---------------------------------------------------------------------------
killed=0
freed_mb=0

for pid in "${kill_pids[@]}"; do
    # The process may have exited since the classification pass — that is fine.
    # Every ps/pgrep/kill call below is guarded with 2>/dev/null and || true.

    # Snapshot child and grandchild PIDs before we start killing anything.
    mapfile -t children < <(pgrep -P "${pid}" 2>/dev/null || true)

    # Build a simple associative array: child PID → space-separated grandchild PIDs.
    declare -A grandchildren=()
    for cpid in "${children[@]}"; do
        mapfile -t gc < <(pgrep -P "${cpid}" 2>/dev/null || true)
        grandchildren["${cpid}"]="${gc[*]:-}"
    done

    # ---- RSS accounting (best-effort; process may be gone by now) ----
    tree_rss=0

    raw_rss=$(ps --no-headers -o rss -p "${pid}" --ppid "${pid}" 2>/dev/null || true)
    while IFS= read -r line; do
        val="${line//[[:space:]]/}"
        if [[ "${val}" =~ ^[0-9]+$ ]]; then
            tree_rss=$(( tree_rss + val ))
        fi
    done <<< "${raw_rss}"

    for cpid in "${children[@]}"; do
        raw=$(ps --no-headers -o rss -p "${cpid}" 2>/dev/null || true)
        val="${raw//[[:space:]]/}"
        if [[ "${val}" =~ ^[0-9]+$ ]]; then
            tree_rss=$(( tree_rss + val ))
        fi

        for gpid in ${grandchildren["${cpid}"]:-}; do
            raw=$(ps --no-headers -o rss= -p "${gpid}" 2>/dev/null || true)
            val="${raw//[[:space:]]/}"
            if [[ "${val}" =~ ^[0-9]+$ ]]; then
                tree_rss=$(( tree_rss + val ))
            fi
        done
    done

    freed_mb=$(( freed_mb + tree_rss / 1024 ))

    # ---- Signal: children first (MCP servers), then the root Claude process ----

    # SIGTERM sweep
    pkill -TERM -P "${pid}" 2>/dev/null || true
    for cpid in "${children[@]}"; do
        pkill -TERM -P "${cpid}" 2>/dev/null || true
    done

    sleep 0.3

    # SIGKILL sweep: grandchildren → children → root
    pkill -KILL -P "${pid}" 2>/dev/null || true
    for cpid in "${children[@]}"; do
        for gpid in ${grandchildren["${cpid}"]:-}; do
            kill -KILL "${gpid}" 2>/dev/null || true
        done
        kill -KILL "${cpid}" 2>/dev/null || true
    done
    kill -KILL "${pid}" 2>/dev/null || true

    unset grandchildren
    declare -A grandchildren=()

    killed=$(( killed + 1 ))
done

log "Killed ${killed} zombie session(s), freed ~${freed_mb}MB."
