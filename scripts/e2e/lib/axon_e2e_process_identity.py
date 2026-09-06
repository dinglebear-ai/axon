"""Platform process identity primitives for the E2E ownership ledger.

The caller supplies its domain-specific error type so this module can be used
without coupling teardown, allocation, and manifest code to each other.
"""

from __future__ import annotations

import ctypes
import getpass
import os
import subprocess
import sys
from pathlib import Path
from typing import Any


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
    api.GetExitCodeProcess.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32)]
    api.GetExitCodeProcess.restype = ctypes.c_int
    api.CloseHandle.argtypes = [ctypes.c_void_p]
    api.CloseHandle.restype = ctypes.c_int
    return api


def windows_kernel32(error_type: type[Exception]) -> Any:
    try:
        return _configure_windows_kernel32(ctypes.WinDLL("kernel32", use_last_error=True))
    except (AttributeError, OSError) as error:
        raise error_type("Windows process identity APIs are unavailable") from error


def windows_process_start_time(
    pid: int, error_type: type[Exception], kernel32: Any | None = None,
) -> str:
    """Return the process creation FILETIME using native Windows handles."""
    api = kernel32 or windows_kernel32(error_type)
    process = api.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
    if not process:
        raise error_type("Windows process handle could not be opened")
    creation = _WindowsFileTime()
    exit_time = _WindowsFileTime()
    kernel = _WindowsFileTime()
    user = _WindowsFileTime()
    try:
        ok = api.GetProcessTimes(
            process, ctypes.byref(creation), ctypes.byref(exit_time), ctypes.byref(kernel), ctypes.byref(user),
        )
        if not ok:
            raise error_type("Windows process creation time is unavailable")
        return str((creation.high << 32) | creation.low)
    finally:
        api.CloseHandle(process)


def windows_process_alive(
    pid: int, error_type: type[Exception], kernel32: Any | None = None,
) -> bool:
    """Query process state without relying on Windows' limited os.kill shim."""
    api = kernel32 or windows_kernel32(error_type)
    process = api.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
    if not process:
        error = ctypes.get_last_error()
        if error in {87, 1168}:  # invalid PID / process no longer exists
            return False
        raise error_type("Windows process state could not be queried")
    exit_code = ctypes.c_uint32()
    try:
        if not api.GetExitCodeProcess(process, ctypes.byref(exit_code)):
            raise error_type("Windows process exit state is unavailable")
        return exit_code.value == 259  # STILL_ACTIVE
    finally:
        api.CloseHandle(process)


def windows_acl(path: Path, *, apply: bool, error_type: type[Exception], owner_getter=getpass.getuser, runner=subprocess.run) -> None:
    """Apply or verify a private owner-only Windows DACL using icacls."""
    owner = owner_getter()
    if not owner or any(char in owner for char in "\r\n"):
        raise error_type("Windows ACL owner identity is unavailable")
    if apply:
        command = ["icacls", str(path), "/inheritance:r", "/grant:r", f"{owner}:(F)"]
        result = runner(command, capture_output=True, text=True, check=False)
        if result.returncode:
            raise error_type(f"failed to apply private Windows DACL: {result.stderr.strip()}")
    result = runner(["icacls", str(path)], capture_output=True, text=True, check=False)
    acl = result.stdout.casefold()
    if result.returncode or owner.casefold() not in acl or "(f)" not in acl:
        raise error_type("private Windows DACL could not be verified")
    forbidden = ("everyone:", "authenticated users:", "builtin\\users:", " users:")
    if any(principal in acl for principal in forbidden):
        raise error_type("Windows DACL grants access beyond the current owner")


def darwin_process_bsdinfo(pid: int) -> Any | None:
    class ProcBsdInfo(ctypes.Structure):
        _fields_ = [
            ("pbi_flags", ctypes.c_uint32), ("pbi_status", ctypes.c_uint32),
            ("pbi_xstatus", ctypes.c_uint32), ("pbi_pid", ctypes.c_uint32),
            ("pbi_ppid", ctypes.c_uint32), ("pbi_uid", ctypes.c_uint32),
            ("pbi_gid", ctypes.c_uint32), ("pbi_ruid", ctypes.c_uint32),
            ("pbi_rgid", ctypes.c_uint32), ("pbi_svuid", ctypes.c_uint32),
            ("pbi_svgid", ctypes.c_uint32), ("rfu_1", ctypes.c_uint32),
            ("pbi_comm", ctypes.c_char * 16), ("pbi_name", ctypes.c_char * 32),
            ("pbi_nfiles", ctypes.c_uint32), ("pbi_pgid", ctypes.c_uint32),
            ("pbi_pjobc", ctypes.c_uint32), ("e_tdev", ctypes.c_uint32),
            ("e_tpgid", ctypes.c_uint32), ("pbi_nice", ctypes.c_int32),
            ("pbi_start_tvsec", ctypes.c_uint64), ("pbi_start_tvusec", ctypes.c_uint64),
        ]

    info = ProcBsdInfo()
    libproc = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
    size = libproc.proc_pidinfo(pid, 3, 0, ctypes.byref(info), ctypes.sizeof(info))
    return info if size == ctypes.sizeof(info) else None


def darwin_process_group_alive(pgid: int) -> bool:
    """Return whether a Darwin process group has any non-zombie members."""
    libproc = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
    count = libproc.proc_listallpids(None, 0)
    if count <= 0:
        raise OSError(ctypes.get_errno(), "proc_listallpids failed")
    pids = (ctypes.c_int * (count + 32))()
    size = libproc.proc_listallpids(ctypes.byref(pids), ctypes.sizeof(pids))
    if size < 0:
        raise OSError(ctypes.get_errno(), "proc_listallpids failed")
    for pid in pids[:size]:
        if pid <= 0:
            continue
        info = darwin_process_bsdinfo(pid)
        if info is not None and info.pbi_pgid == pgid and info.pbi_status != 5:
            return True
    return False


def process_start_time(pid: int, error_type: type[Exception]) -> str:
    if pid < 1:
        raise error_type("process PID must be positive")
    if os.name == "nt":
        return windows_process_start_time(pid, error_type)
    if sys.platform == "darwin":
        # `/bin/ps` is protected on macOS and unavailable under sandbox-exec.
        info = darwin_process_bsdinfo(pid)
        if info is None or not info.pbi_start_tvsec:
            raise error_type("process start time is unavailable")
        return f"{info.pbi_start_tvsec}.{info.pbi_start_tvusec:06d}"
    proc_stat = Path(f"/proc/{pid}/stat")
    if proc_stat.exists():
        fields = proc_stat.read_text(encoding="utf-8").split()
        if len(fields) < 22:
            raise error_type("process start time is unavailable")
        return fields[21]
    result = subprocess.run(
        ["ps", "-o", "lstart=", "-p", str(pid)], capture_output=True, text=True, check=False,
    )
    value = result.stdout.strip()
    if result.returncode or not value:
        raise error_type("process start time is unavailable")
    return value
