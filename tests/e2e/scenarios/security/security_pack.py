#!/usr/bin/env python3
"""Executable security-negative contracts for Axon's E2E suite."""
from __future__ import annotations

import base64
import hashlib
import ipaddress
import json
import urllib.parse
from pathlib import Path


class SecurityError(RuntimeError):
    pass


SSRF_CASES = (
    "http://127.0.0.1/x", "http://2130706433/x", "http://0177.0.0.1/x",
    "http://0x7f000001/x", "http://[::1]/x", "http://[::ffff:127.0.0.1]/x",
    "http://169.254.169.254/x", "http://10.0.0.1/x", "http://172.16.0.1/x",
    "http://192.168.0.1/x", "http://[fc00::1]/x", "http://user@127.0.0.1/x",
    "file:///etc/passwd", "gopher://127.0.0.1/x", "ftp://127.0.0.1/x",
    "http://rebind.axon-e2e.invalid/x", "http://redirect.axon-e2e.invalid/x",
)


def forbidden_destination(url: str, dns: dict[str, list[str]] | None = None) -> str:
    try:
        parsed = urllib.parse.urlsplit(url)
        if parsed.scheme not in {"http", "https"} or not parsed.hostname:
            return "url.scheme_forbidden"
        host = parsed.hostname.rstrip(".").lower()
        addresses = (dns or {}).get(host, [host])
        for address in addresses:
            try:
                ip = ipaddress.ip_address(address)
            except ValueError:
                try:
                    if address.lower().startswith("0x"):
                        ip = ipaddress.ip_address(int(address, 16))
                    elif address.isdigit() and "." not in address:
                        ip = ipaddress.ip_address(int(address, 10))
                    elif "." in address:
                        parts = address.split(".")
                        if len(parts) != 4 or not all(part.isdigit() for part in parts):
                            raise ValueError
                        values = [int(part, 8) if len(part) > 1 and part.startswith("0") else int(part, 10)
                                  for part in parts]
                        if any(value > 255 for value in values):
                            raise ValueError
                        ip = ipaddress.ip_address(".".join(map(str, values)))
                    else:
                        continue
                except (ValueError, OverflowError):
                    return "url.malformed"
            if not ip.is_global:
                return "url.private_address"
        if host.endswith(".axon-e2e.invalid"):
            return "url.fixture_rebinding_forbidden"
        return "allowed"
    except (ValueError, UnicodeError):
        return "url.malformed"


def assert_zero_connections(before: int, after: int, classification: str) -> None:
    if classification == "allowed" or before != after:
        raise SecurityError("forbidden destination received a connection")


def provider_boundary(resource: str, identity: str, operation: str, run_id: str,
                      marker: str | None, active: bool = False) -> str:
    allowed = {
        "qdrant": {"collection.delete", "alias.delete", "snapshot.delete"},
        "chrome": {"profile.delete", "session.close"},
    }
    if resource not in allowed or operation not in allowed[resource]:
        return "provider.operation_forbidden"
    if operation.endswith(("list", "enumerate")) or operation.startswith("admin."):
        return "provider.enumeration_forbidden"
    if not identity.startswith("axon_e2e_") or marker is None or not marker.startswith(run_id):
        return "provider.not_owned"
    if active:
        return "provider.resource_active"
    return "allowed"


AUTH_MATRIX = (
    # surface, route/action, local policy, remote policy, scope, origin
    ("cli_local", "status", "local", "n/a", "none", "n/a"),
    ("mcp_stdio", "initialize", "process", "n/a", "none", "n/a"),
    ("mcp_http", "initialize", "token", "token", "read", "required"),
    ("mcp_tasks", "tasks/get", "token", "token", "read", "required"),
    ("mcp_tasks", "tasks/cancel", "token", "token", "write", "required"),
    ("mcp_sse", "tasks/get", "token", "token", "read", "required"),
    ("rest", "/v1/status", "loopback_optional", "token", "read", "cors"),
    ("uploads", "/v1/uploads", "loopback_optional", "token", "write", "cors"),
    ("artifacts", "/v1/artifacts/{id}", "loopback_optional", "token", "read", "cors"),
    ("destructive", "/v1/prune/execute", "token", "token", "write", "cors"),
    ("oauth", "/authorize", "oauth", "oauth", "claims", "redirect_pkce_state"),
)


def validate_auth_observation(case: dict) -> None:
    required = {"surface", "route", "credential", "status", "error_code"}
    if not required <= case.keys():
        raise SecurityError("auth observation is incomplete")
    credential, status = case["credential"], case["status"]
    if credential in {"missing", "invalid", "conflicting", "insufficient_scope"} and status not in {401, 403}:
        raise SecurityError("unauthorized request did not fail closed")
    if case.get("nonloopback") and credential == "missing" and status != 401:
        raise SecurityError("tokenless non-loopback request was accepted")
    if case.get("oauth"):
        if not all(case.get(key) for key in ("state_verified", "pkce_verified", "redirect_verified", "claims_verified")):
            raise SecurityError("OAuth state, PKCE, redirect, or claims were not verified")


def transformations(secret: str) -> dict[str, str]:
    raw = secret.encode()
    return {"plain": secret, "base64": base64.b64encode(raw).decode(),
            "hex": raw.hex(), "url": urllib.parse.quote(secret, safe=""), "reversed": secret[::-1]}


def scan_artifact(data: bytes, secrets: list[str]) -> list[dict]:
    text = data.decode("utf-8", errors="replace")
    findings = []
    for secret in secrets:
        for encoding, value in transformations(secret).items():
            if value in text:
                findings.append({"secret_sha256": hashlib.sha256(secret.encode()).hexdigest(),
                                 "encoding": encoding})
    return findings


def scan_tree(root: Path, secrets: list[str]) -> None:
    for path in root.rglob("*"):
        if path.is_file():
            findings = scan_artifact(path.read_bytes(), secrets)
            if findings:
                raise SecurityError(f"canary detected in evidence artifact {path.name}: {json.dumps(findings)}")
