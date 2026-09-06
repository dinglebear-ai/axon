#!/usr/bin/env python3
"""Fail-closed guard for destructive E2E operations.

This module never deletes anything itself. Callers supply a plan fetcher and a
single-target delete callback; the plan is re-fetched and verified immediately
before each callback.
"""
from __future__ import annotations

import hashlib
import hmac
import json
import time
from typing import Callable, NamedTuple


class GuardError(RuntimeError):
    pass


def _canonical(value: dict) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def plan_payload(run_id: str, attempt: int, targets: list[dict], expires_unix_ms: int) -> dict:
    if not run_id.startswith("axon_e2e_") or attempt < 1:
        raise GuardError("invalid run or attempt identity")
    if expires_unix_ms <= int(time.time() * 1000):
        raise GuardError("plan is already expired")
    normalized = []
    seen = set()
    for target in targets:
        item = {key: target.get(key) for key in ("type", "identity", "ownership_marker")}
        if not all(isinstance(value, str) and value for value in item.values()):
            raise GuardError("target identity and ownership marker are required")
        if not item["ownership_marker"].startswith(run_id) or not item["identity"].startswith("axon_e2e_"):
            raise GuardError("target is not owned by this run")
        key = (item["type"], item["identity"])
        if key in seen:
            raise GuardError("duplicate or ambiguous target")
        seen.add(key); normalized.append(item)
    normalized.sort(key=lambda item: (item["type"], item["identity"], item["ownership_marker"]))
    return {"version": 1, "run_id": run_id, "attempt": attempt,
            "expires_unix_ms": expires_unix_ms, "targets": normalized}


def digest(payload: dict, key: bytes) -> str:
    if len(key) < 32:
        raise GuardError("plan key must contain at least 256 bits")
    return hmac.new(key, _canonical(payload), hashlib.sha256).hexdigest()


class Confirmation(NamedTuple):
    digest: str
    run_id: str
    attempt: int


def execute(fetch_plan: Callable[[], dict], confirmation: Confirmation, key: bytes,
            delete_one: Callable[[dict], None], now_ms: Callable[[], int] | None = None) -> list[str]:
    clock = now_ms or (lambda: int(time.time() * 1000))
    initial = fetch_plan()
    canonical = plan_payload(initial.get("run_id", ""), initial.get("attempt", 0),
                             initial.get("targets", []), initial.get("expires_unix_ms", 0))
    if initial != canonical:
        raise GuardError("plan is not canonical")
    expected = digest(initial, key)
    if not hmac.compare_digest(expected, confirmation.digest):
        raise GuardError("confirmation digest mismatch")
    if (initial.get("run_id"), initial.get("attempt")) != (confirmation.run_id, confirmation.attempt):
        raise GuardError("confirmation scope mismatch")
    deleted = []
    for expected_target in initial.get("targets", []):
        current = fetch_plan()  # immediate revalidation before every deletion
        if digest(current, key) != expected or current != initial:
            raise GuardError("destructive plan changed after confirmation")
        if clock() >= int(current.get("expires_unix_ms", 0)):
            raise GuardError("destructive plan expired")
        matches = [item for item in current["targets"] if
                   (item["type"], item["identity"]) ==
                   (expected_target["type"], expected_target["identity"])]
        if len(matches) != 1 or matches[0]["ownership_marker"] != expected_target["ownership_marker"]:
            raise GuardError("target became missing, duplicate, or ambiguously owned")
        delete_one(expected_target); deleted.append(expected_target["identity"])
    return deleted
