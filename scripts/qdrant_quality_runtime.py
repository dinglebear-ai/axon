#!/usr/bin/env python3
"""Runtime URL resolution and Rust-default extraction helpers."""

from __future__ import annotations

import os
import re
import socket
import urllib.parse
import urllib.request
from pathlib import Path


def running_in_container() -> bool:
    return os.path.exists("/.dockerenv")


def hostname_resolves(hostname: str) -> bool:
    try:
        socket.getaddrinfo(hostname, None)
        return True
    except socket.gaierror:
        return False


def endpoint_reachable(base_url: str) -> bool:
    try:
        url = f"{base_url.rstrip('/')}/"
        parsed = urllib.parse.urlparse(url)
        if parsed.scheme not in {"http", "https"}:
            return False
        req = urllib.request.Request(url, method="GET")
        with urllib.request.urlopen(req, timeout=2):  # noqa: S310
            return True
    except Exception:
        return False


def resolve_runtime_qdrant_url(configured_url: str) -> str:
    if running_in_container():
        return configured_url

    parsed = urllib.parse.urlparse(configured_url)
    host = parsed.hostname
    if not host or host in {"localhost", "127.0.0.1"}:
        return configured_url

    if hostname_resolves(host):
        return configured_url

    candidates: list[str] = []
    if parsed.port == 6333:
        candidates.extend(["http://localhost:53333", "http://127.0.0.1:53333"])
    candidates.extend(["http://localhost:6333", "http://127.0.0.1:6333"])

    for candidate in candidates:
        if endpoint_reachable(candidate):
            return candidate

    return configured_url


def extract_rust_default_excludes() -> list[str]:
    """Extract default exclude prefixes from src/core/config.rs."""
    config_path = Path(__file__).resolve().parents[1] / "src/core/config.rs"
    if not config_path.exists():
        return []

    text = config_path.read_text(encoding="utf-8")
    match = re.search(
        r"fn\s+default_exclude_prefixes\(\)\s*->\s*Vec<String>\s*\{\s*vec!\[(?P<body>.*?)\]\s*\.into_iter\(\)",
        text,
        re.DOTALL,
    )
    if not match:
        return []

    body = match.group("body")
    values = re.findall(r'\"([^\"]+)\"', body)
    return sorted(set(values))
