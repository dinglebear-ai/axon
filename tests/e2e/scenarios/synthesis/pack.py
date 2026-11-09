"""Bounded admin/account rejection lifecycle against an OpenAI-compatible runtime.

This pack intentionally never starts an OAuth flow or touches a user's account.
The real HTTP server must reject account mutation for its non-Codex backend.
"""
from __future__ import annotations


def run(call):
    records = []
    cases = [
        ("GET", "/v1/synthesis/account", None, 503, "synthesis_account_unsupported"),
        ("POST", "/v1/synthesis/account/login", {"switch_account": False}, 400,
         "synthesis_account_confirmation_required"),
        ("POST", "/v1/synthesis/account/login", {"switch_account": True}, 503,
         "synthesis_account_unsupported"),
        ("POST", "/v1/synthesis/account/login/cancel", {"login_id": "e2e-nonexistent-login"},
         404, "synthesis_account_login_unknown"),
    ]
    for method, path, body, expected, code in cases:
        status, value = call(method, path, body, True)
        error = value.get("error", {})
        if status != expected or error.get("code") != code or value.get("ok") is not False:
            raise RuntimeError(f"synthesis account rejection mismatch: {method} {path}: HTTP {status}, code {error.get('code')!r}; expected {expected}/{code}")
        records.append({"path": path, "status": status, "code": code})
        status, _ = call(method, path, body, False)
        if status not in {401, 403}:
            raise RuntimeError(f"synthesis account admin boundary failed: {method} {path}: {status}")
    return {"operation_id": "surface:synthesis", "surface": "http", "result": "pass",
            "requests": 8, "cases": records,
            "proof_limit": "Non-Codex backend and unauthenticated admin rejection only; real OAuth and successful device login are not exercised."}
