#!/usr/bin/env python3
"""Portable allocation and append-only registration primitives for Axon E2E.

This module intentionally does not delete resources. Teardown, residual audit,
and stale-run recovery consume its manifest and are owned by the teardown suite.
"""

from __future__ import annotations

import argparse
import contextlib
import ctypes
import getpass
import hashlib
import hmac
import json
import os
import re
import secrets
import subprocess
import socket
import stat
import sys
import tempfile
import time
from pathlib import Path
from typing import Any, Iterator

MANIFEST_VERSION = 1
RUN_PREFIX = "axon_e2e_"
RESOURCE_TYPES = {
    "artifact", "collection", "compose_project", "data_dir", "evidence", "job", "network",
    "operation", "port", "process", "source", "sqlite", "upload", "watch",
}
PROVIDER_DEFAULT_CEILINGS = {"chrome": 2, "llm": 2, "qdrant": 4, "tei": 2}


class IsolationError(RuntimeError):
    """An unsafe or internally inconsistent isolation request."""


def _is_windows() -> bool:
    return os.name == "nt"


class _WindowsFileTime(ctypes.Structure):
    _fields_ = [("low", ctypes.c_uint32), ("high", ctypes.c_uint32)]


def _configure_windows_kernel32(api: Any) -> Any:
    filetime_pointer = ctypes.POINTER(_WindowsFileTime)
    api.OpenProcess.argtypes = [ctypes.c_uint32, ctypes.c_int, ctypes.c_uint32]
    api.OpenProcess.restype = ctypes.c_void_p
    api.GetProcessTimes.argtypes = [
        ctypes.c_void_p, filetime_pointer, filetime_pointer, filetime_pointer, filetime_pointer,
    ]
    api.GetProcessTimes.restype = ctypes.c_int
    api.CloseHandle.argtypes = [ctypes.c_void_p]
    api.CloseHandle.restype = ctypes.c_int
    return api


def _windows_kernel32():
    try:
        return _configure_windows_kernel32(ctypes.WinDLL("kernel32", use_last_error=True))
    except (AttributeError, OSError) as error:
        raise IsolationError("Windows process identity APIs are unavailable") from error


def _windows_process_start_time(pid: int, kernel32: Any | None = None) -> str:
    """Return the process creation FILETIME using native Windows handles."""
    api = kernel32 or _windows_kernel32()
    process = api.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
    if not process:
        raise IsolationError("Windows process handle could not be opened")
    creation = _WindowsFileTime()
    exit_time = _WindowsFileTime()
    kernel = _WindowsFileTime()
    user = _WindowsFileTime()
    try:
        ok = api.GetProcessTimes(
            process, ctypes.byref(creation), ctypes.byref(exit_time), ctypes.byref(kernel), ctypes.byref(user),
        )
        if not ok:
            raise IsolationError("Windows process creation time is unavailable")
        return str((creation.high << 32) | creation.low)
    finally:
        api.CloseHandle(process)


def _windows_acl(path: Path, *, apply: bool) -> None:
    """Apply or verify a private owner-only Windows DACL using icacls.

    icacls is part of supported Windows installations. Refusing to proceed when
    it is absent or its output cannot be verified prevents permission fallback.
    """
    owner = getpass.getuser()
    if not owner or any(char in owner for char in "\r\n"):
        raise IsolationError("Windows ACL owner identity is unavailable")
    if apply:
        command = ["icacls", str(path), "/inheritance:r", "/grant:r", f"{owner}:(F)"]
        result = subprocess.run(command, capture_output=True, text=True, check=False)
        if result.returncode:
            raise IsolationError(f"failed to apply private Windows DACL: {result.stderr.strip()}")
    result = subprocess.run(["icacls", str(path)], capture_output=True, text=True, check=False)
    acl = result.stdout.casefold()
    if result.returncode or owner.casefold() not in acl or "(f)" not in acl:
        raise IsolationError("private Windows DACL could not be verified")
    forbidden = ("everyone:", "authenticated users:", "builtin\\users:", " users:")
    if any(principal in acl for principal in forbidden):
        raise IsolationError("Windows DACL grants access beyond the current owner")


def _canonical(path: Path) -> Path:
    return path.expanduser().resolve(strict=False)


def _is_within(path: Path, parent: Path) -> bool:
    try:
        _canonical(path).relative_to(_canonical(parent))
        return True
    except ValueError:
        return False


def validate_run_paths(run_root: Path, data_dir: Path, manifest_root: Path) -> None:
    home_state = _canonical(Path.home() / ".axon")
    run_root, data_dir, manifest_root = map(_canonical, (run_root, data_dir, manifest_root))
    if run_root == home_state or _is_within(run_root, home_state):
        raise IsolationError(f"run root overlaps production state: {run_root}")
    if not _is_within(data_dir, run_root):
        raise IsolationError(f"AXON_DATA_DIR must be inside the owned run root: {data_dir}")
    if _is_within(manifest_root, data_dir) or _is_within(data_dir, manifest_root):
        raise IsolationError("manifest root and tested AXON_DATA_DIR must not overlap")


def validate_owned_name(value: str) -> None:
    if not value.startswith(RUN_PREFIX) or len(value) > 128:
        raise IsolationError(f"resource name must use {RUN_PREFIX!r} prefix")
    if not all(char.isalnum() or char in "_-" for char in value):
        raise IsolationError("resource name contains unsafe characters")


def _process_start_time(pid: int) -> str:
    if pid < 1:
        raise IsolationError("process PID must be positive")
    if _is_windows():
        return _windows_process_start_time(pid)
    proc_stat = Path(f"/proc/{pid}/stat")
    if proc_stat.exists():
        fields = proc_stat.read_text(encoding="utf-8").split()
        if len(fields) < 22:
            raise IsolationError("process start time is unavailable")
        return fields[21]
    result = subprocess.run(
        ["ps", "-o", "lstart=", "-p", str(pid)], capture_output=True, text=True, check=False,
    )
    value = result.stdout.strip()
    if result.returncode or not value:
        raise IsolationError("process start time is unavailable")
    return value


def new_run_id() -> str:
    return f"{RUN_PREFIX}{int(time.time())}_{secrets.token_hex(12)}"


def _private_write(path: Path, data: bytes) -> None:
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    descriptor = os.open(path, flags, 0o600)
    try:
        os.write(descriptor, data)
    finally:
        os.close(descriptor)
    if _is_windows():
        _windows_acl(path, apply=True)


class Manifest:
    """A versioned, chained-HMAC, append-only resource registration ledger."""

    def __init__(self, path: Path, key_path: Path):
        self.path = path
        self.key_path = key_path

    @classmethod
    def create(cls, manifest_root: Path, run_id: str, data_dir: Path) -> "Manifest":
        validate_owned_name(run_id)
        manifest_root.mkdir(parents=True, exist_ok=True, mode=0o700)
        run_manifest_root = manifest_root / run_id
        run_manifest_root.mkdir(mode=0o700)
        manifest = cls(run_manifest_root / "resources.jsonl", run_manifest_root / "manifest.key")
        _private_write(manifest.key_path, secrets.token_bytes(32))
        manifest._append({
            "kind": "header", "version": MANIFEST_VERSION, "run_id": run_id,
            "data_dir": str(_canonical(data_dir)), "created_unix_ms": int(time.time() * 1000),
        })
        return manifest

    @classmethod
    def open(cls, path: Path) -> "Manifest":
        return cls(path, path.with_name("manifest.key"))

    def _key(self) -> bytes:
        if _is_windows():
            _windows_acl(self.key_path, apply=False)
            return self.key_path.read_bytes()
        mode = stat.S_IMODE(self.key_path.stat().st_mode)
        if mode & 0o077:
            raise IsolationError("manifest key must not be accessible by group or other users")
        return self.key_path.read_bytes()

    def _records(self) -> list[dict[str, Any]]:
        if not self.path.exists():
            return []
        records = []
        for line_number, line in enumerate(self.path.read_text(encoding="utf-8").splitlines(), 1):
            try:
                records.append(json.loads(line))
            except json.JSONDecodeError as error:
                raise IsolationError(f"invalid manifest JSON on line {line_number}") from error
        return records

    def verify(self) -> list[dict[str, Any]]:
        previous = "0" * 64
        key = self._key()
        records = self._records()
        if not records or records[0].get("payload", {}).get("kind") != "header":
            raise IsolationError("manifest header is missing")
        for index, record in enumerate(records):
            if record.get("sequence") != index or record.get("previous") != previous:
                raise IsolationError(f"manifest chain is broken at sequence {index}")
            unsigned = {key_: record[key_] for key_ in ("sequence", "previous", "payload")}
            encoded = json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()
            expected = hmac.new(key, encoded, hashlib.sha256).hexdigest()
            if not hmac.compare_digest(expected, str(record.get("hmac", ""))):
                raise IsolationError(f"manifest integrity failure at sequence {index}")
            previous = expected
        return records

    def _append(self, payload: dict[str, Any]) -> None:
        with _directory_lock(self.path.with_name(".manifest-lock")):
            records = self.verify() if self.path.exists() else []
            previous = records[-1]["hmac"] if records else "0" * 64
            unsigned = {"sequence": len(records), "previous": previous, "payload": payload}
            encoded = json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()
            record = {**unsigned, "hmac": hmac.new(self._key(), encoded, hashlib.sha256).hexdigest()}
            descriptor = os.open(self.path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
            try:
                os.write(descriptor, (json.dumps(record, sort_keys=True) + "\n").encode())
                os.fsync(descriptor)
            finally:
                os.close(descriptor)

    def register(self, resource_type: str, identity: str, metadata: dict[str, Any] | None = None) -> None:
        if resource_type not in RESOURCE_TYPES:
            raise IsolationError(f"unsupported resource type: {resource_type}")
        metadata = metadata or {}
        records = self.verify()
        header = records[0]["payload"]
        namespaced_types = {"collection", "compose_project", "evidence", "network", "operation", "watch"}
        if resource_type in namespaced_types:
            validate_owned_name(identity)
        if resource_type in {"artifact", "upload"} and not identity.startswith(RUN_PREFIX):
            prefix = "art_" if resource_type == "artifact" else "upl_"
            required = {"run_id", "attempt", "scenario_id", "request_id", "origin",
                        "parent_resource_type", "parent_identity"}
            if not re.fullmatch(rf"{prefix}[A-Za-z0-9_-]{{3,128}}", identity):
                raise IsolationError(f"opaque {resource_type} identity has an invalid production format")
            if not required.issubset(metadata) or metadata.get("origin") != "server_response":
                raise IsolationError(f"opaque {resource_type} registration lacks trusted server binding")
            if metadata.get("run_id") != header["run_id"] or not isinstance(metadata.get("attempt"), int) or metadata["attempt"] < 1:
                raise IsolationError(f"opaque {resource_type} registration is not bound to this run attempt")
            parent_type, parent_identity = metadata["parent_resource_type"], metadata["parent_identity"]
            if parent_type not in {"operation", "evidence"}:
                raise IsolationError(f"opaque {resource_type} parent must be an operation or evidence record")
            parent_found = any(record.get("payload", {}).get("kind") == "resource"
                               and record["payload"].get("resource_type") == parent_type
                               and record["payload"].get("identity") == parent_identity
                               for record in records)
            if not parent_found:
                raise IsolationError(f"opaque {resource_type} parent is not registered in this manifest")
        elif resource_type in {"artifact", "upload"}:
            validate_owned_name(identity)
        data_dir = Path(header["data_dir"])
        if resource_type in {"job", "source"}:
            if not identity or any(char in identity for char in "\r\n\t"):
                raise IsolationError(f"{resource_type} identity must be non-empty and single-line")
            if metadata.get("run_id") != header["run_id"]:
                raise IsolationError(f"{resource_type} registration must be bound to the manifest run")
        if resource_type == "data_dir" and _canonical(Path(identity)) != _canonical(data_dir):
            raise IsolationError("data_dir identity does not match the owned manifest header")
        if resource_type == "sqlite" and not _is_within(Path(identity), data_dir):
            raise IsolationError("SQLite identity must remain inside the owned AXON_DATA_DIR")
        if resource_type == "port":
            try:
                port = int(identity)
            except ValueError as error:
                raise IsolationError("port identity must be an integer") from error
            if not 1 <= port <= 65535 or metadata.get("host") not in {"127.0.0.1", "::1"}:
                raise IsolationError("port identity must be an owned loopback endpoint")
            lease = Path(str(metadata.get("lease", "")))
            try:
                lease_owner = json.loads((lease / "owner.json").read_text(encoding="utf-8"))["run_id"]
            except (OSError, KeyError, json.JSONDecodeError) as error:
                raise IsolationError("port registration requires a readable cooperative lease") from error
            if lease_owner != header["run_id"]:
                raise IsolationError("port lease is not owned by this manifest run")
        if resource_type == "process":
            try:
                pid = int(identity)
            except ValueError as error:
                raise IsolationError("process identity must be a PID") from error
            required = {"start_time", "nonce", "nonce_file"}
            if pid < 1 or not required.issubset(metadata) or len(str(metadata["nonce"])) < 32:
                raise IsolationError("process registration requires PID, start time, and strong nonce ownership")
            nonce_file = Path(str(metadata["nonce_file"]))
            run_root = data_dir.parent
            try:
                nonce_matches = nonce_file.read_text(encoding="utf-8") == metadata["nonce"]
            except OSError as error:
                raise IsolationError("process nonce ownership marker is unreadable") from error
            if not _is_within(nonce_file, run_root) or not nonce_matches:
                raise IsolationError("process nonce marker is not owned by this run")
            if _process_start_time(pid) != str(metadata["start_time"]):
                raise IsolationError("process PID start time does not match the live process")
        self._append({
            "kind": "resource", "resource_type": resource_type, "identity": identity,
            "metadata": metadata, "registered_unix_ms": int(time.time() * 1000),
        })


@contextlib.contextmanager
def _directory_lock(path: Path, timeout: float = 10.0) -> Iterator[None]:
    deadline = time.monotonic() + timeout
    while True:
        try:
            path.mkdir()
            if _is_windows():
                _windows_acl(path, apply=True)
            break
        except FileExistsError:
            if time.monotonic() >= deadline:
                raise IsolationError(f"timed out acquiring state lock: {path}")
            time.sleep(0.02)
    try:
        yield
    finally:
        path.rmdir()


class PortReservation:
    """A cooperative lease plus held socket; callers hand off before closing."""

    def __init__(self, port: int, sock: socket.socket, lease_path: Path):
        self.port, self.socket, self.lease_path = port, sock, lease_path

    def close(self) -> None:
        self.socket.close()

    def __enter__(self) -> "PortReservation": return self
    def __exit__(self, *_args: object) -> None: self.close()


def allocate_port(lease_root: Path, run_id: str, manifest: Manifest) -> PortReservation:
    validate_owned_name(run_id)
    lease_root.mkdir(parents=True, exist_ok=True)
    for _ in range(256):
        candidate = secrets.randbelow(20000) + 30000
        lease = lease_root / str(candidate)
        try:
            lease.mkdir()
        except FileExistsError:
            continue
        sock = socket.socket()
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 0)
        try:
            sock.bind(("127.0.0.1", candidate))
        except OSError:
            sock.close()
            lease.rmdir()
            continue
        (lease / "owner.json").write_text(json.dumps({"run_id": run_id}), encoding="utf-8")
        manifest.register("port", str(candidate), {"lease": str(lease), "host": "127.0.0.1"})
        return PortReservation(candidate, sock, lease)
    raise IsolationError("unable to allocate a loopback port lease")


class ResourceGovernor:
    """Atomic weighted admission with global and per-provider ceilings."""

    def __init__(self, root: Path, global_ceiling: int, provider_ceilings: dict[str, int] | None = None):
        if global_ceiling < 1:
            raise IsolationError("global ceiling must be positive")
        self.root, self.global_ceiling = root, global_ceiling
        self.provider_ceilings = provider_ceilings or PROVIDER_DEFAULT_CEILINGS
        root.mkdir(parents=True, exist_ok=True)
        self.state_path, self.lock_path = root / "governor.json", root / ".lock"

    def _state(self) -> dict[str, Any]:
        if not self.state_path.exists():
            return {"leases": {}}
        return json.loads(self.state_path.read_text(encoding="utf-8"))

    def acquire(self, run_id: str, provider: str, weight: int) -> str:
        validate_owned_name(run_id)
        if weight < 1 or weight > self.global_ceiling:
            raise IsolationError("weight is outside the global capacity")
        ceiling = self.provider_ceilings.get(provider, self.global_ceiling)
        with _directory_lock(self.lock_path):
            state = self._state()
            leases = state["leases"]
            global_used = sum(item["weight"] for item in leases.values())
            provider_used = sum(item["weight"] for item in leases.values() if item["provider"] == provider)
            if global_used + weight > self.global_ceiling or provider_used + weight > ceiling:
                raise IsolationError(f"capacity unavailable for provider {provider}")
            token = secrets.token_hex(16)
            leases[token] = {"run_id": run_id, "provider": provider, "weight": weight}
            self.state_path.write_text(json.dumps(state, sort_keys=True), encoding="utf-8")
            return token

    def release(self, token: str, run_id: str) -> None:
        with _directory_lock(self.lock_path):
            state = self._state()
            lease = state["leases"].get(token)
            if not lease or lease["run_id"] != run_id:
                raise IsolationError("governor lease is absent or not owned by this run")
            del state["leases"][token]
            self.state_path.write_text(json.dumps(state, sort_keys=True), encoding="utf-8")


class ManagedProcess:
    """Registered process identity. Signalling/teardown is deliberately absent."""

    def __init__(self, process: subprocess.Popen[Any], start_time: str, nonce: str, nonce_file: Path):
        self.process, self.start_time, self.nonce, self.nonce_file = process, start_time, nonce, nonce_file

    def validate_owner(self) -> bool:
        if self.process.poll() is not None:
            return False
        try:
            return (
                _process_start_time(self.process.pid) == self.start_time
                and self.nonce_file.read_text(encoding="utf-8") == self.nonce
            )
        except (IsolationError, OSError):
            return False


def spawn_owned_process(
    manifest: Manifest, run_root: Path, argv: list[str], *, env: dict[str, str] | None = None,
) -> ManagedProcess:
    if not argv:
        raise IsolationError("owned process argv must not be empty")
    nonce = secrets.token_hex(32)
    nonce_dir = _canonical(run_root) / "process-ownership"
    nonce_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    nonce_file = nonce_dir / f"{nonce}.owner"
    _private_write(nonce_file, nonce.encode())
    process_env = os.environ.copy(); process_env.update(env or {})
    process_env["AXON_E2E_PROCESS_NONCE"] = nonce
    creationflags = getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0x00000200) if _is_windows() else 0
    process = subprocess.Popen(
        argv, env=process_env, start_new_session=not _is_windows(), creationflags=creationflags,
    )
    try:
        start_time = _process_start_time(process.pid)
        manifest.register("process", str(process.pid), {
            "start_time": start_time, "nonce": nonce, "nonce_file": str(nonce_file),
            "process_group": process.pid, "argv0": Path(argv[0]).name,
        })
    except Exception:
        process.terminate(); process.wait(timeout=5)
        raise
    return ManagedProcess(process, start_time, nonce, nonce_file)


def spawn_isolated_python(
    manifest: Manifest, run_root: Path, argv: list[str], allowed_endpoints: list[tuple[str, int]],
    *, env: dict[str, str] | None = None,
) -> ManagedProcess:
    """Launch Python with actual socket enforcement; reject unsupported launchers.

    This is an explicit portable capability. Native binaries need a platform
    sandbox adapter and are refused here so hermetic mode cannot degrade open.
    """
    if not argv or not Path(argv[0]).name.lower().startswith("python"):
        raise IsolationError("portable containment supports only the isolated Python launcher")
    for host, port in allowed_endpoints:
        if host not in {"127.0.0.1", "::1", "localhost"} or not 1 <= port <= 65535:
            raise IsolationError("hermetic allowlist accepts only owned loopback endpoints")
    guard_dir = _canonical(run_root) / "network-guard"
    guard_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    guard = guard_dir / "sitecustomize.py"
    guard.write_text(
        "import json, os, socket\n"
        "_allowed={tuple(x) for x in json.loads(os.environ['AXON_E2E_ALLOWED_ENDPOINTS'])}\n"
        "_connect=socket.socket.connect\n"
        "_connect_ex=socket.socket.connect_ex\n"
        "def _guard(self,address):\n"
        "  host,port=address[0],int(address[1])\n"
        "  if (host,port) not in _allowed: raise PermissionError('Axon E2E denied outbound destination')\n"
        "  return _connect(self,address)\n"
        "socket.socket.connect=_guard\n"
        "socket.socket.connect_ex=lambda self,address: _guard(self,address) or 0\n",
        encoding="utf-8",
    )
    isolated_env = dict(env or {})
    inherited = isolated_env.get("PYTHONPATH", os.environ.get("PYTHONPATH", ""))
    isolated_env["PYTHONPATH"] = str(guard_dir) + (os.pathsep + inherited if inherited else "")
    isolated_env["AXON_E2E_ALLOWED_ENDPOINTS"] = json.dumps(allowed_endpoints)
    isolated_env["AXON_E2E_NETWORK_CAPABILITY"] = "python-socket-guard-v1"
    return spawn_owned_process(manifest, run_root, argv, env=isolated_env)


def allocate(run_base: Path, manifest_base: Path) -> dict[str, str]:
    run_id = new_run_id()
    run_root = _canonical(run_base / run_id)
    data_dir = run_root / "data"
    validate_run_paths(run_root, data_dir, manifest_base)
    data_dir.mkdir(parents=True, mode=0o700)
    sqlite = data_dir / "jobs.db"
    manifest = Manifest.create(_canonical(manifest_base), run_id, data_dir)
    manifest.register("data_dir", str(data_dir))
    manifest.register("sqlite", str(sqlite))
    namespace = run_id
    for kind in ("collection", "compose_project", "network"):
        manifest.register(kind, namespace)
    return {
        "run_id": run_id, "run_root": str(run_root), "data_dir": str(data_dir),
        "sqlite": str(sqlite), "manifest": str(manifest.path), "namespace": namespace,
        "network_policy": "deny-external", "ssrf_token": secrets.token_urlsafe(24),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)
    create = sub.add_parser("allocate")
    create.add_argument("--run-base", type=Path, default=Path(tempfile.gettempdir()) / "axon-e2e-runs")
    create.add_argument("--manifest-base", type=Path, default=Path(tempfile.gettempdir()) / "axon-e2e-manifests")
    register = sub.add_parser("register")
    register.add_argument("manifest", type=Path)
    register.add_argument("resource_type", choices=sorted(RESOURCE_TYPES))
    register.add_argument("identity")
    register.add_argument("--metadata-json", default="{}")
    verify = sub.add_parser("verify")
    verify.add_argument("manifest", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "allocate":
            print(json.dumps(allocate(args.run_base, args.manifest_base), sort_keys=True))
        elif args.command == "register":
            metadata = json.loads(args.metadata_json)
            if not isinstance(metadata, dict):
                raise IsolationError("registration metadata must be a JSON object")
            Manifest.open(args.manifest).register(args.resource_type, args.identity, metadata)
        else:
            records = Manifest.open(args.manifest).verify()
            print(json.dumps({"records": len(records), "valid": True}))
    except IsolationError as error:
        print(f"isolation error: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
