#!/usr/bin/env python3
"""Last-resort CI cleanup for signed E2E manifests.

The scenario runners remain responsible for normal teardown.  This independent
outer layer exists for job cancellation, runner timeout, and process death.  It
only acts on manifests whose HMAC chain and resource ownership can be verified
by the canonical teardown engine.
"""

from __future__ import annotations

import argparse
import hashlib
import hmac
import importlib.util
import json
import os
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


def _load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


teardown = _load("axon_e2e_outer_teardown", ROOT / "scripts/e2e/lib/teardown.py")
AUTHORITY_FILES = {"resources.jsonl", "manifest.key", "resources.jsonl.provider-ledger",
                   "outer-cleanup-registration.json", "outer-registry-registration.json", "cleanup-report.json"}
EMPTY_SCAFFOLDING = {"runs", "owned-runs", "manifests", "ownership-manifests"}


def _registry_key(path: Path, *, create: bool = False) -> bytes:
    key_path = path.with_suffix(path.suffix + ".key")
    if create and not key_path.exists():
        path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        descriptor = os.open(key_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        try: os.write(descriptor, os.urandom(32))
        finally: os.close(descriptor)
    if os.name != "nt" and key_path.stat().st_mode & 0o077:
        raise RuntimeError("cleanup registry key permissions are unsafe")
    return key_path.read_bytes()


def _registry_payload(path: Path) -> dict:
    envelope = json.loads(path.read_text())
    payload = envelope.get("payload") if isinstance(envelope, dict) else None
    if not isinstance(payload, dict) or payload.get("schema") != 1 or not isinstance(payload.get("runs"), list):
        raise RuntimeError("cleanup registry schema is invalid")
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    expected = hmac.new(_registry_key(path), encoded, hashlib.sha256).hexdigest()
    if not hmac.compare_digest(expected, str(envelope.get("hmac", ""))):
        raise RuntimeError("cleanup registry integrity failure")
    return payload


def _write_registry(path: Path, runs: list[dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    key = _registry_key(path, create=True)
    payload = {"schema": 1, "runs": runs}
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    envelope = {"payload": payload, "hmac": hmac.new(key, encoded, hashlib.sha256).hexdigest()}
    temporary = path.with_suffix(path.suffix + f".{os.getpid()}.tmp")
    temporary.write_text(json.dumps(envelope, sort_keys=True) + "\n"); os.chmod(temporary, 0o600)
    os.replace(temporary, path)


def register(registry: Path, manifest: Path) -> dict:
    header, _ = teardown.manifest_api.load(manifest.resolve(strict=True))
    # The header checkpoint is immutable while the signed append-only resource
    # chain legitimately grows after this mandatory early registration.
    header_checkpoint = teardown.manifest_api.isolation.Manifest.open(manifest).verify()[0]["hmac"]
    registry.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    lock = registry.with_suffix(registry.suffix + ".lock")
    with teardown.manifest_api.isolation._directory_lock(lock):
        runs = _registry_payload(registry)["runs"] if registry.exists() else []
        entry = {"run_id": header.run_id, "manifest": str(manifest.resolve()),
                 "header_checkpoint": header_checkpoint, "created_unix_ms": header.created_unix_ms}
        runs = [item for item in runs if item.get("run_id") != header.run_id] + [entry]
        _write_registry(registry, runs)
    return {"schema": 1, "success": True, "mode": "register", "run_id": header.run_id,
            "header_checkpoint": header_checkpoint}


def manifests(root: Path) -> list[Path]:
    if not root.exists():
        return []
    return sorted(path.resolve() for path in root.rglob("resources.jsonl") if path.is_file())


def _authority_files(manifest: Path) -> list[Path]:
    directory = manifest.parent
    children = list(directory.iterdir())
    unexpected = [item for item in children if item.name not in AUTHORITY_FILES or item.is_symlink() or not item.is_file()]
    if unexpected:
        raise RuntimeError("manifest authority directory contains unexpected entries")
    required = {"resources.jsonl", "manifest.key"}
    if not required.issubset({item.name for item in children}):
        raise RuntimeError("manifest authority directory is incomplete")
    return children


def _retire_authority(files: list[Path]) -> None:
    directory = files[0].parent
    # Retire ancillary signed state first and the manifest/key last.  All paths
    # were preflighted as exact regular children of this one authority.
    order = sorted(files, key=lambda item: (item.name in {"resources.jsonl", "manifest.key"}, item.name))
    for item in order: item.unlink()
    directory.rmdir()


def _prune_empty_scaffolding(root: Path) -> None:
    if not root.exists(): return
    candidates = sorted((item for item in root.rglob("*") if item.is_dir() and not item.is_symlink()),
                        key=lambda item: len(item.parts), reverse=True)
    for item in candidates:
        if item.name in EMPTY_SCAFFOLDING:
            try: item.rmdir()
            except OSError: pass


def _mark_retiring(registry: Path, manifest: Path) -> None:
    with teardown.manifest_api.isolation._directory_lock(registry.with_suffix(registry.suffix + ".lock")):
        runs = _registry_payload(registry)["runs"]
        matched = False
        for item in runs:
            if Path(item["manifest"]).resolve() == manifest:
                item["status"] = "retiring"; matched = True
        if not matched: raise RuntimeError("registered authority disappeared before retirement")
        _write_registry(registry, runs)


def cleanup(root: Path, *, stale_seconds: float | None, live_gateways: bool, registry: Path | None = None,
            now_ms: int | None = None) -> dict:
    now_ms = int(time.time() * 1000) if now_ms is None else now_ms
    results, refused, skipped = [], [], []
    paths = set(manifests(root)); registered: dict[Path, dict] = {}
    if registry is not None and registry.exists():
        try:
            for entry in _registry_payload(registry)["runs"]:
                path = Path(str(entry.get("manifest", ""))).resolve()
                registered[path] = entry; paths.add(path)
        except Exception as error:
            return {"schema": 1, "success": False, "mode": "stale" if stale_seconds is not None else "all",
                    "root": str(root.resolve()), "cleanups": [], "skipped": [],
                    "refused": [{"registry": str(registry), "reason": f"{type(error).__name__}: {error}"}]}
    completed_paths: set[Path] = set()
    for path in sorted(paths):
        try:
            if not path.is_file():
                entry = registered.get(path)
                if entry and entry.get("status") == "retiring" and not path.parent.exists():
                    completed_paths.add(path)
                    skipped.append({"run_id": str(entry.get("run_id")), "reason": "retirement-completed"})
                    continue
                raise RuntimeError("registered manifest is missing")
            header, _ = teardown.manifest_api.load(path)
            if entry := registered.get(path):
                checkpoint = teardown.manifest_api.isolation.Manifest.open(path).verify()[0]["hmac"]
                if (entry.get("run_id"), entry.get("header_checkpoint"), entry.get("created_unix_ms")) != \
                        (header.run_id, checkpoint, header.created_unix_ms):
                    raise RuntimeError("registered manifest identity or immutable header changed")
            age_ms = now_ms - header.created_unix_ms
            if age_ms < 0:
                raise RuntimeError("manifest creation time is in the future")
            if stale_seconds is not None and age_ms < stale_seconds * 1000:
                skipped.append({"run_id": header.run_id, "reason": "active-age-guard"})
                continue
            run_root = header.data_dir.parent
            if not run_root.exists():
                # Canonical teardown removes the run root only after every
                # upstream/provider phase has succeeded.  Its absence is thus
                # a durable completion marker, not a name-based assumption.
                files = _authority_files(path)
                if registry is not None and path in registered: _mark_retiring(registry, path)
                _retire_authority(files)
                results.append({"run_id": header.run_id, "success": True, "outcome": "retired-completed-authority"})
                completed_paths.add(path)
                continue
            descriptors = list(run_root.rglob("descriptor.json"))
            if descriptors:
                for descriptor in descriptors:
                    completed = subprocess.run(
                        [sys.executable, str(ROOT / "scripts/e2e/teardown-hermetic-stack.py"), str(descriptor)],
                        cwd=ROOT, capture_output=True, text=True, timeout=90, check=False,
                    )
                    if completed.returncode:
                        raise RuntimeError("signed stack descriptor teardown failed")
                if not run_root.exists():
                    files = _authority_files(path)
                    if registry is not None and path in registered: _mark_retiring(registry, path)
                    _retire_authority(files)
                    results.append({"run_id": header.run_id, "success": True,
                                    "outcome": "signed-descriptor-teardown-and-authority-retired"})
                    completed_paths.add(path)
                    continue
            adapters = None
            if live_gateways:
                adapter = teardown.provider_api.GatewayLeaseAdapter(header, teardown.manifest_api)
                adapters = {"provider_reservation": adapter}
            receipt = teardown.Engine(path, adapters).run().json()
            results.append(receipt)
            if not receipt.get("success"):
                refused.append({"run_id": header.run_id, "reason": "canonical teardown failed"})
            else:
                files = _authority_files(path)
                if registry is not None and path in registered: _mark_retiring(registry, path)
                _retire_authority(files)
                completed_paths.add(path)
        except Exception as error:
            # An unverified manifest is evidence of unknown ownership.  Never
            # guess or delete by name; fail the cleanup gate for an operator.
            refused.append({"manifest": str(path), "reason": f"{type(error).__name__}: {error}"})
    if registry is not None and registry.exists() and completed_paths:
        with teardown.manifest_api.isolation._directory_lock(registry.with_suffix(registry.suffix + ".lock")):
            current = _registry_payload(registry)["runs"]
            _write_registry(registry, [item for item in current if Path(item["manifest"]).resolve() not in completed_paths])
    _prune_empty_scaffolding(root)
    return {"schema": 1, "success": not refused, "mode": "stale" if stale_seconds is not None else "all",
            "root": str(root.resolve()), "cleanups": results, "skipped": skipped, "refused": refused}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest-root", type=Path, default=ROOT / "target/e2e")
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--stale-seconds", type=float)
    parser.add_argument("--live-gateways", action="store_true")
    parser.add_argument("--registry", type=Path)
    parser.add_argument("--register-manifest", type=Path)
    args = parser.parse_args()
    try:
        if args.register_manifest:
            if args.registry is None: parser.error("--register-manifest requires --registry")
            report = register(args.registry, args.register_manifest)
        else:
            report = cleanup(args.manifest_root, stale_seconds=args.stale_seconds, live_gateways=args.live_gateways,
                             registry=args.registry)
    except Exception as error:
        report = {"schema": 1, "success": False, "mode": "register" if args.register_manifest else "cleanup",
                  "refused": [{"reason": f"{type(error).__name__}: {error}"}]}
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0 if report["success"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
