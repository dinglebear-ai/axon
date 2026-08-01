#!/usr/bin/env bash

# Manage a host-loopback Incus proxy without exposing the nested service on a
# non-loopback interface. The caller supplies the `incus` command/function,
# which also makes this boundary directly testable without a live daemon.
ensure_loopback_proxy() {
  local container="$1" device="$2" listen="$3" connect="$4"
  case "$listen" in
    tcp:127.0.0.1:*) ;;
    *)
      echo "refusing non-loopback proxy listener: $listen" >&2
      return 1
      ;;
  esac

  if incus config device set "$container" "$device" \
    "listen=$listen" \
    "connect=$connect" >/dev/null 2>&1; then
    return 0
  fi
  incus config device add "$container" "$device" proxy \
    "listen=$listen" \
    "connect=$connect" >/dev/null
}
