#!/usr/bin/env python3
"""Fail-closed redaction and evidence packaging for Axon E2E runs."""
from __future__ import annotations

import base64
import hashlib
import json
import os
import re
import stat
from pathlib import Path
from typing import Iterable
from urllib.parse import quote, quote_plus

MAX_FILE_BYTES = 512 * 1024
MAX_SCENARIO_BYTES = 2 * 1024 * 1024
MAX_SUITE_BYTES = 16 * 1024 * 1024
FORBIDDEN_NAMES = {".env", "jobs.db", "config.toml"}
FORBIDDEN_PARTS = {"chrome-profile", "chrome_profile", "user-data-dir", "private-content"}
SAFE_SUFFIXES = {".json", ".jsonl", ".log", ".txt", ".xml", ".md"}
TOKEN_PATTERNS = (
    re.compile(r"(?i)(authorization\s*:\s*(?:bearer\s+)?)[^\s\"']+"),
    re.compile(r"(?i)((?:api[_-]?key|access[_-]?token|refresh[_-]?token|client[_-]?secret|auth(?:entication)?[_-]?(?:token|secret)|session[_-]?(?:token|secret|cookie)|cookie|password|token)\s*[=:]\s*)[^\s,;\"']+"),
    re.compile(r"(?i)(\"(?:api[_-]?key|access[_-]?token|refresh[_-]?token|client[_-]?secret|auth(?:entication)?[_-]?(?:token|secret)|session[_-]?(?:token|secret|cookie)|cookie|password|token)\"\s*:\s*\")[^\"]+(\")"),
    re.compile(r"(?i)(https?://[^\s/@:]+:)[^\s/@]+(@)"),
)


class RedactionError(RuntimeError): pass


def _variants(secret: str) -> set[str]:
    raw = secret.encode()
    values = {secret, secret.lower(), secret.upper(), quote(secret, safe=""), quote_plus(secret),
              base64.b64encode(raw).decode(), base64.urlsafe_b64encode(raw).decode(), raw.hex()}
    if len(secret) > 3:
        values.add("\n".join(secret[index:index + 3] for index in range(0, len(secret), 3)))
    return {value for value in values if value}


def redact_text(text: str, secrets: Iterable[str]) -> tuple[str, int]:
    count = 0
    for secret in secrets:
        if not secret: continue
        for value in sorted(_variants(secret), key=len, reverse=True):
            occurrences = text.count(value)
            if occurrences: text = text.replace(value, "[REDACTED]"); count += occurrences
    for pattern in TOKEN_PATTERNS:
        text, found = pattern.subn(lambda match: match.group(1) + "[REDACTED]" + (match.group(2) if match.lastindex == 2 else ""), text)
        count += found
    return text, count


def scan_bytes(data: bytes, secrets: Iterable[str]) -> None:
    if b"\x00" in data: raise RedactionError("binary evidence is forbidden")
    try: text = data.decode("utf-8")
    except UnicodeDecodeError as error: raise RedactionError("non-UTF-8 evidence is forbidden") from error
    lowered = text.casefold()
    for secret in secrets:
        for value in _variants(secret):
            if value.casefold() in lowered: raise RedactionError("transformed secret canary detected")
    for pattern in TOKEN_PATTERNS:
        if any("[REDACTED]" not in match.group(0) for match in pattern.finditer(text)):
            raise RedactionError("credential-shaped content detected")


def sanitize_text(text: str, secrets: Iterable[str]) -> tuple[str, int]:
    sanitized, count = redact_text(text, secrets)
    scan_bytes(sanitized.encode(), secrets)
    return sanitized, count


def _validate_source(path: Path, root: Path) -> os.stat_result:
    if path.is_absolute() and root not in path.resolve().parents and path.resolve() != root:
        raise RedactionError("evidence path is outside the approved root")
    resolved = path.resolve(strict=True)
    if resolved != root and root not in resolved.parents: raise RedactionError("evidence traversal is forbidden")
    info = path.lstat()
    if stat.S_ISLNK(info.st_mode): raise RedactionError("evidence symlinks are forbidden")
    if not stat.S_ISREG(info.st_mode): raise RedactionError("evidence must be a regular file")
    if info.st_nlink != 1: raise RedactionError("evidence hardlinks are forbidden")
    lowered = {part.casefold() for part in path.parts}
    normalized = {re.sub(r"[^a-z0-9]", "", part) for part in lowered}
    forbidden_normalized = {re.sub(r"[^a-z0-9]", "", part) for part in FORBIDDEN_PARTS}
    normalized_singular = {part[:-1] if part.endswith("s") else part for part in normalized}
    if path.name.casefold().startswith(".env") or path.name.casefold() in FORBIDDEN_NAMES or normalized_singular & forbidden_normalized:
        raise RedactionError("forbidden evidence class")
    if path.suffix.casefold() not in SAFE_SUFFIXES: raise RedactionError("unclassified evidence file type")
    if info.st_size > MAX_FILE_BYTES: raise RedactionError("evidence file exceeds byte ceiling")
    return info


def package(root: Path, selected: Iterable[Path], destination: Path, secrets: Iterable[str]) -> dict:
    root = root.resolve(strict=True); files = []; total = 0; scenario_sizes: dict[str, int] = {}
    destination.mkdir(parents=True, exist_ok=True)
    if any(destination.iterdir()): raise RedactionError("destination must be empty")
    for source in sorted({Path(item) for item in selected}, key=lambda item: str(item)):
        info = _validate_source(source, root); relative = source.resolve().relative_to(root)
        data = source.read_bytes(); scan_bytes(data, secrets)
        scenario = relative.parts[0] if len(relative.parts) > 1 else "suite"
        scenario_sizes[scenario] = scenario_sizes.get(scenario, 0) + info.st_size
        total += info.st_size
        if scenario_sizes[scenario] > MAX_SCENARIO_BYTES: raise RedactionError("scenario evidence exceeds byte ceiling")
        if total > MAX_SUITE_BYTES: raise RedactionError("suite evidence exceeds byte ceiling")
        target = destination / relative; target.parent.mkdir(parents=True, exist_ok=True); target.write_bytes(data)
        digest = hashlib.sha256(data).hexdigest()
        files.append({"path": relative.as_posix(), "bytes": info.st_size, "sha256": digest})
    manifest = {"schema": 1, "files": files, "total_bytes": total}
    encoded = json.dumps(manifest, indent=2, sort_keys=True) + "\n"
    (destination / "evidence-manifest.json").write_text(encoded)
    return manifest


class RedactingStream:
    """Apply redaction before a line is emitted to immutable CI logs."""
    def __init__(self, sink, secrets: Iterable[str]): self.sink, self.secrets, self.buffer = sink, secrets, ""
    def write(self, value: str) -> int:
        self.buffer += value; return len(value)
    def flush(self) -> None: self.sink.flush()
    def close(self) -> None:
        sanitized, _ = sanitize_text(self.buffer, tuple(self.secrets)); self.sink.write(sanitized); self.buffer = ""; self.sink.flush()
    def __enter__(self): return self
    def __exit__(self, exc_type, exc, traceback): self.close()


class CredentialMasker:
    """Register credentials at acquisition and expose one boundary-safe stream."""
    def __init__(self, control_sink): self.control_sink, self.secrets = control_sink, []
    def acquire(self, value: str) -> str:
        if not value or "\x00" in value: raise RedactionError("dynamic credential is invalid")
        escaped = value.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
        self.control_sink.write(f"::add-mask::{escaped}\n"); self.control_sink.flush(); self.secrets.append(value); return value
    def stream(self, sink) -> RedactingStream: return RedactingStream(sink, self.secrets)


def validate_command(argv: Iterable[str], secrets: Iterable[str]) -> None:
    values = tuple(argv); joined = " ".join(values); lowered = joined.casefold()
    if "set -x" in lowered or "set -o xtrace" in lowered or "curl -v" in lowered or "curl --verbose" in lowered:
        raise RedactionError("debug command mode is forbidden")
    if any(item in {"env", "printenv"} for item in values): raise RedactionError("raw environment dump is forbidden")
    if "--trace" in values or "--trace-ascii" in values: raise RedactionError("full HTTP trace is forbidden")
    for secret in secrets:
        if secret and any(variant in joined for variant in _variants(secret)):
            raise RedactionError("dynamic credential in argv or URL is forbidden")
