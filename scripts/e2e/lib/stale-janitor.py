#!/usr/bin/env python3
"""Ownership-limited stale-run janitor. Preview is the mandatory default."""

from __future__ import annotations

import argparse
import hashlib
import hmac
import importlib.util
import json
import os
import sys
import time
from pathlib import Path
from typing import Any, Callable


def _load(filename: str, name: str):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    if spec is None or spec.loader is None: raise RuntimeError(f"{filename} unavailable")
    module = importlib.util.module_from_spec(spec); sys.modules[name] = module; spec.loader.exec_module(module)
    return module


manifest_api = _load("resource-manifest.py", "axon_e2e_resource_manifest_janitor")
teardown = _load("teardown.py", "axon_e2e_teardown_janitor")


class JanitorError(RuntimeError): pass


class CleanupLease:
    def __init__(self, path: Path): self.path = path
    def __enter__(self):
        try: self.path.mkdir(mode=0o700)
        except FileExistsError as error: raise JanitorError("cleanup lease is already held") from error
        (self.path / "owner.json").write_text(json.dumps({"pid": os.getpid(), "started_unix_ms": int(time.time() * 1000)}))
        return self
    def __exit__(self, *_args):
        (self.path / "owner.json").unlink(missing_ok=True); self.path.rmdir()


def _registry_key(path: Path) -> bytes:
    key_path = path.with_suffix(path.suffix + ".key")
    try:
        if os.name != "nt" and key_path.stat().st_mode & 0o077: raise JanitorError("janitor registry key permissions are unsafe")
        return key_path.read_bytes()
    except OSError as error: raise JanitorError("janitor registry key is unavailable") from error


def _write_registry_unlocked(path: Path, runs: list[dict[str, Any]], key: bytes | None = None) -> None:
    path.parent.mkdir(parents=True, exist_ok=True); key_path = path.with_suffix(path.suffix + ".key")
    if key is not None and not key_path.exists():
        descriptor = os.open(key_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        try: os.write(descriptor, key)
        finally: os.close(descriptor)
    key = _registry_key(path)
    payload = {"schema": 1, "runs": runs}; encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    envelope = {"payload": payload, "hmac": hmac.new(key, encoded, hashlib.sha256).hexdigest()}
    temporary = path.with_suffix(path.suffix + f".{os.getpid()}.tmp")
    temporary.write_text(json.dumps(envelope, sort_keys=True) + "\n"); os.chmod(temporary, 0o600); os.replace(temporary, path)


def write_registry(path: Path, runs: list[dict[str, Any]], key: bytes | None = None) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with manifest_api.isolation._directory_lock(path.with_suffix(path.suffix + ".lock")):
        _write_registry_unlocked(path, runs, key)


def load_registry(path: Path) -> list[dict[str, Any]]:
    envelope = json.loads(path.read_text()); value = envelope.get("payload") if isinstance(envelope, dict) else None
    if not isinstance(value, dict) or value.get("schema") != 1 or not isinstance(value.get("runs"), list):
        raise JanitorError("invalid explicit janitor registry")
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    expected = hmac.new(_registry_key(path), encoded, hashlib.sha256).hexdigest()
    if not hmac.compare_digest(expected, str(envelope.get("hmac", ""))): raise JanitorError("janitor registry integrity failure")
    return value["runs"]


def register_run(path: Path, manifest: Path, *, heartbeat_unix_ms: int, expires_unix_ms: int) -> None:
    header, _ = manifest_api.load(manifest); path.parent.mkdir(parents=True, exist_ok=True)
    with manifest_api.isolation._directory_lock(path.with_suffix(path.suffix + ".lock")):
        runs = load_registry(path) if path.exists() else []
        entry = {"run_id": header.run_id, "manifest": str(manifest.resolve()), "manifest_digest": header.digest,
                 "heartbeat_unix_ms": heartbeat_unix_ms, "expires_unix_ms": expires_unix_ms}
        runs = [item for item in runs if item.get("run_id") != header.run_id] + [entry]
        key = os.urandom(32) if not path.with_suffix(path.suffix + ".key").exists() else None
        _write_registry_unlocked(path, runs, key)


def select_stale(registry: Path, *, now_ms: int, clock_skew_ms: int = 300_000) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    selected, refused = [], []
    for entry in load_registry(registry):
        try:
            manifest = Path(entry["manifest"]).resolve(strict=True)
            header, _ = manifest_api.load(manifest)
            if header.run_id != entry["run_id"] or header.digest != entry["manifest_digest"]:
                raise JanitorError("registry identity or digest mismatch")
            heartbeat = int(entry["heartbeat_unix_ms"]); expiry = int(entry["expires_unix_ms"])
            if heartbeat > now_ms + clock_skew_ms: raise JanitorError("heartbeat is in the future beyond clock skew")
            if now_ms <= expiry + clock_skew_ms: raise JanitorError("run is active or inside clock-skew guard")
            selected.append({**entry, "manifest": str(manifest)})
        except Exception as error:
            refused.append({"run_id": str(entry.get("run_id", "unknown")), "reason": str(error)})
    return selected, refused


def run(registry: Path, lease: Path, *, execute: bool = False, now_ms: int | None = None,
        engine_factory: Callable[[Path], Any] = teardown.Engine) -> dict[str, Any]:
    fixed_now = now_ms
    now_ms = now_ms or int(time.time() * 1000)
    with CleanupLease(lease):
        selected, refused = select_stale(registry, now_ms=now_ms)
        report: dict[str, Any] = {"mode": "execute" if execute else "preview", "selected": selected,
                                  "refused": refused, "cleanups": []}
        if execute:
            for entry in selected:
                # Re-open and revalidate immediately before deletion (TOCTOU boundary).
                current_now = fixed_now if fixed_now is not None else int(time.time() * 1000)
                current, _ = select_stale(registry, now_ms=current_now)
                if not any(item["run_id"] == entry["run_id"] and item["manifest_digest"] == entry["manifest_digest"] for item in current):
                    report["refused"].append({"run_id": entry["run_id"], "reason": "stale state changed before deletion"}); continue
                engine = engine_factory(Path(entry["manifest"]))
                try:
                    provider = engine.provider_lease_state()
                    heartbeat, expiry = int(provider["heartbeat_unix_ms"]), int(provider["expires_unix_ms"])
                    if heartbeat > current_now + 300_000 or current_now <= expiry + 300_000:
                        raise JanitorError("provider-native lease is active or inside clock-skew guard")
                    if heartbeat != int(entry["heartbeat_unix_ms"]) or expiry != int(entry["expires_unix_ms"]):
                        raise JanitorError("provider-native lease differs from the signed registry")
                except Exception as error:
                    report["refused"].append({"run_id": entry["run_id"], "reason": str(error)}); continue
                cleanup = engine.run().json()
                report["cleanups"].append(cleanup)
        report["success"] = not report["refused"] and all(item.get("success") for item in report["cleanups"])
        return report


def main() -> int:
    parser = argparse.ArgumentParser(); parser.add_argument("registry", type=Path); parser.add_argument("--lease", type=Path, required=True)
    parser.add_argument("--execute", action="store_true"); parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--register-manifest", type=Path); parser.add_argument("--heartbeat-unix-ms", type=int)
    parser.add_argument("--expires-unix-ms", type=int); parser.add_argument("--provider-config", type=Path); args = parser.parse_args()
    try:
        if args.register_manifest:
            if args.heartbeat_unix_ms is None or args.expires_unix_ms is None:
                raise JanitorError("registration requires heartbeat and expiry")
            register_run(args.registry, args.register_manifest, heartbeat_unix_ms=args.heartbeat_unix_ms,
                         expires_unix_ms=args.expires_unix_ms)
            report = {"success": True, "mode": "register"}
        else:
            def configured_engine(path: Path):
                header, _ = manifest_api.load(path)
                adapters = teardown.provider_api.build(args.provider_config, header, manifest_api) if args.provider_config else None
                return teardown.Engine(path, adapters)
            report = run(args.registry, args.lease, execute=args.execute, engine_factory=configured_engine)
    except Exception as error: report = {"success": False, "fatal": str(error)}
    args.report.parent.mkdir(parents=True, exist_ok=True); args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0 if report.get("success") else 2


if __name__ == "__main__": raise SystemExit(main())
