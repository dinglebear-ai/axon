#!/usr/bin/env bash
# cleanup-claude.sh — Kill zombie Claude Code sessions and their MCP child trees.
#
# Keeps: Claude processes that are foreground (+) in a real TTY.
# Kills: Stopped (T), backgrounded (no TTY), or stale sessions + all their children.
#
# Usage:
#   ./scripts/cleanup-claude.sh          # dry-run (default)
#   ./scripts/cleanup-claude.sh --kill   # actually kill
#   ./scripts/cleanup-claude.sh --cron   # kill + quiet (for cron)

set -euo pipefail

MODE="${1:-dry-run}"
QUIET=false
[[ "$MODE" == "--cron" ]] && MODE="--kill" && QUIET=true

killed=0
freed_mb=0

log() { [[ "$QUIET" == false ]] && echo "$@"; }

# Collect all top-level Claude processes (the main "claude" binary, not children)
mapfile -t claude_pids < <(pgrep -x claude 2>/dev/null || true)

if [[ ${#claude_pids[@]} -eq 0 ]]; then
    log "No Claude processes found."
    exit 0
fi

keep_pids=()
kill_pids=()

for pid in "${claude_pids[@]}"; do
    state=$(ps -o stat= -p "$pid" 2>/dev/null || echo "?")
    tty=$(ps -o tty= -p "$pid" 2>/dev/null || echo "?")
    rss_kb=$(ps -o rss= -p "$pid" 2>/dev/null || echo "0")
    rss_mb=$(( rss_kb / 1024 ))

    # Keep: foreground process in a real TTY (state contains '+')
    if [[ "$state" == *"+"* ]] && [[ "$tty" != "?" ]]; then
        keep_pids+=("$pid")
        log "  KEEP  PID=$pid  state=$state  tty=$tty  ${rss_mb}MB"
        continue
    fi

    # Kill: stopped (T), no TTY (?), or background without foreground flag
    reason=""
    if [[ "$state" == *"T"* ]]; then
        reason="stopped"
    elif [[ "$tty" == "?" ]]; then
        reason="no-tty (background/Zed)"
    else
        reason="background"
    fi

    kill_pids+=("$pid")
    log "  KILL  PID=$pid  state=$state  tty=$tty  ${rss_mb}MB  ($reason)"
done

if [[ ${#kill_pids[@]} -eq 0 ]]; then
    log "Nothing to clean up. ${#keep_pids[@]} active session(s)."
    exit 0
fi

log ""
log "Keeping ${#keep_pids[@]} active session(s), killing ${#kill_pids[@]} zombie(s)."

if [[ "$MODE" != "--kill" ]]; then
    log ""
    log "Dry run — re-run with --kill to execute."
    exit 0
fi

for pid in "${kill_pids[@]}"; do
    # Sum RSS of the entire process tree before killing
    tree_rss=$(ps --no-headers -o rss -p "$pid" --ppid "$pid" 2>/dev/null | awk '{s+=$1} END {print s+0}')
    # Also get deeper descendants
    descendants=$(pgrep -P "$pid" 2>/dev/null || true)
    for dpid in $descendants; do
        sub_rss=$(ps --no-headers -o rss -p "$dpid" 2>/dev/null | awk '{s+=$1} END {print s+0}')
        tree_rss=$(( tree_rss + sub_rss ))
        # Grandchildren (MCP servers spawn their own children)
        for gpid in $(pgrep -P "$dpid" 2>/dev/null || true); do
            g_rss=$(ps --no-headers -o rss= -p "$gpid" 2>/dev/null || echo 0)
            tree_rss=$(( tree_rss + g_rss ))
        done
    done

    freed_mb=$(( freed_mb + tree_rss / 1024 ))

    # Kill children first (MCP servers), then the Claude process
    pkill -TERM -P "$pid" 2>/dev/null || true
    for dpid in $descendants; do
        pkill -TERM -P "$dpid" 2>/dev/null || true
    done
    sleep 0.3

    # Force kill anything that survived
    pkill -KILL -P "$pid" 2>/dev/null || true
    for dpid in $descendants; do
        kill -KILL "$dpid" 2>/dev/null || true
    done
    kill -KILL "$pid" 2>/dev/null || true

    killed=$(( killed + 1 ))
done

log "Killed $killed zombie session(s), freed ~${freed_mb}MB."
