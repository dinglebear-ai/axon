"""Remote container mechanics for the TEI tuning CLI."""
from __future__ import annotations

import argparse
import contextlib
import fcntl
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time

from tei_tune_runtime import (
    docker_run_from_snapshot,
    secondary_network_commands,
    validate_container_name,
    validate_ssh_host,
)


class TeiRemote:
    def __init__(self, args: argparse.Namespace):
        self.host = validate_ssh_host(args.host)
        self.container = validate_container_name(args.container)
        self.url = args.url.rstrip("/")
        self.image = args.image
        self.port = args.port
        self.network = args.network
        self.gpu = args.gpu
        self.cache = args.cache
        self.entrypoint = args.entrypoint
        state_name = f"{self.host}-{self.container}".replace("/", "_") + ".json"
        self.state = Path(args.state_dir).expanduser() / state_name

    def remote(
        self, argv: list[str], check: bool = True, input_text: str | None = None
    ) -> subprocess.CompletedProcess[str]:
        validate_ssh_host(self.host)
        holder = getattr(self, "_remote_lock_holder", None)
        if holder is not None and holder.poll() is not None:
            raise RuntimeError(
                f"remote TEI mutation lock was lost for {self.host}/{self.container}; refusing further mutation"
            )
        operation_lock = getattr(self, "_remote_operation_lock", None)
        remote_argv = (
            ["flock", "-s", operation_lock, "--", *argv]
            if holder is not None and operation_lock is not None
            else argv
        )
        command = ["ssh", self.host, shlex.join(remote_argv)]
        if holder is None:
            return subprocess.run(
                command, text=True, capture_output=True, check=check, input=input_text,
            )
        process = subprocess.Popen(
            command,
            text=True,
            stdin=subprocess.PIPE if input_text is not None else None,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        first_wait = True
        while True:
            try:
                stdout, stderr = process.communicate(
                    input=input_text if first_wait else None,
                    timeout=0.1,
                )
                break
            except subprocess.TimeoutExpired:
                first_wait = False
                if holder.poll() is not None:
                    process.terminate()
                    process.communicate()
                    raise RuntimeError(
                        f"remote TEI mutation lock was lost for {self.host}/{self.container}; "
                        "stopped waiting locally; remote state is uncertain, but the operation "
                        "lock prevents a successor from racing this command"
                    )
        result = subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
        if check and result.returncode:
            raise subprocess.CalledProcessError(
                result.returncode, command, output=stdout, stderr=stderr
            )
        return result

    def inspect(self) -> dict:
        result = self.remote(["docker", "inspect", self.container])
        return json.loads(result.stdout)[0]

    def resolve_image_id(self, image: str) -> str:
        image_id = self.remote(
            ["docker", "image", "inspect", image, "--format={{.Id}}"]
        ).stdout.strip()
        if not image_id:
            raise RuntimeError(f"cannot resolve immutable image ID for {image}")
        return image_id

    def snapshot(self) -> dict:
        inspected = self.inspect()
        return {
            "version": 1,
            "created_at": time.time(),
            "image": inspected["Config"]["Image"],
            "image_id": inspected.get("Image"),
            "cmd": inspected["Config"]["Cmd"],
            "entrypoint": inspected["Config"].get("Entrypoint"),
            "config": inspected["Config"],
            "host_config": inspected["HostConfig"],
            "network_mode": inspected["HostConfig"].get("NetworkMode", "bridge"),
            "networks": inspected["NetworkSettings"].get("Networks", {}),
        }

    def save_snapshot(self, snapshot: dict) -> None:
        self.state.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        temporary = self.state.with_suffix(".tmp")
        temporary.write_text(json.dumps(snapshot, indent=2) + "\n")
        temporary.chmod(0o600)
        temporary.replace(self.state)

    @property
    def parked_container(self) -> str:
        return f"{self.container}-tei-tune-rollback"

    @contextlib.contextmanager
    def mutation_lock(self):
        state = getattr(self, "state", Path("/tmp/tei-tune-state"))
        host = getattr(self, "host", "unknown-host")
        container = getattr(self, "container", "axon-tei")
        lock_path = state.with_name(f"{state.name}.{host}.{container}.lock")
        lock_path.parent.mkdir(parents=True, exist_ok=True)
        with lock_path.open("a+") as lock_file:
            try:
                fcntl.flock(lock_file, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                raise RuntimeError(
                    f"another TEI mutation is already running for {self.host}/{self.container}"
                ) from error
            try:
                lock_prefix = f"/tmp/axon-tei-tune-{validate_container_name(container)}"
                owner_lock = f"{lock_prefix}.owner.lock"
                operation_lock = f"{lock_prefix}.operation.lock"
                ready_command = shlex.join([
                    "flock", operation_lock, "sh", "-c",
                    "printf 'axon-tei-tune-locked\\n'",
                ])
                lock_command = shlex.join([
                    "flock", "-n", owner_lock, "sh", "-c",
                    f"{ready_command}; cat >/dev/null",
                ])
                holder = subprocess.Popen(
                    ["ssh", validate_ssh_host(host), lock_command],
                    text=True,
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                )
                acquired = holder.stdout.readline() if holder.stdout else ""
                if acquired != "axon-tei-tune-locked\n":
                    _, error = holder.communicate()
                    raise RuntimeError(
                        f"another TEI mutation is already running for {self.host}/{self.container}: {error.strip()}"
                    )
                self._remote_lock_holder = holder
                self._remote_operation_lock = operation_lock
                try:
                    if holder.poll() is not None:
                        raise RuntimeError(
                            f"remote TEI mutation lock was lost for {self.host}/{self.container}; refusing mutation"
                        )
                    yield
                    if holder.poll() is not None:
                        raise RuntimeError(
                            f"remote TEI mutation lock was lost for {self.host}/{self.container}; mutation result is uncertain"
                        )
                finally:
                    self._remote_lock_holder = None
                    self._remote_operation_lock = None
                    if holder.stdin:
                        holder.stdin.close()
                    holder.wait()
                    if holder.returncode:
                        error = holder.stderr.read().strip() if holder.stderr else ""
                        print(f"warning: remote TEI mutation lock ended with an error: {error}", file=sys.stderr)
            finally:
                fcntl.flock(lock_file, fcntl.LOCK_UN)

    def park_current(self) -> None:
        self.remote(["docker", "rm", "-f", self.parked_container], check=False)
        self.remote(["docker", "stop", self.container])
        try:
            self.remote(["docker", "rename", self.container, self.parked_container])
        except subprocess.CalledProcessError as rename_error:
            restarted = self.remote(["docker", "start", self.container], check=False)
            if restarted.returncode:
                rename_detail = rename_error.stderr or str(rename_error)
                restart_detail = restarted.stderr.strip() or restarted.stdout.strip()
                raise RuntimeError(
                    f"failed to park TEI ({rename_detail}); restart failed: {restart_detail}"
                ) from rename_error
            raise

    def restore_parked(self) -> None:
        self.remote(["docker", "rm", "-f", self.container], check=False)
        self.remote(["docker", "rename", self.parked_container, self.container])
        self.remote(["docker", "start", self.container])

    def discard_parked(self) -> None:
        self.remote(["docker", "rm", "-f", self.parked_container])

    def discard_parked_after_success(self, outcome: str) -> None:
        try:
            self.discard_parked()
        except Exception as error:
            recovery = shlex.join(["ssh", self.host, "docker", "rm", "-f", self.parked_container])
            raise RuntimeError(
                f"{outcome} succeeded and is live, but cleanup of parked container "
                f"{self.parked_container!r} failed ({error}); retry safely with: {recovery}"
            ) from error

    def restore_after_failure(self, primary: str) -> None:
        try:
            self.restore_parked()
        except Exception as restore_error:
            raise RuntimeError(
                f"{primary}; restoring the parked TEI configuration also failed: {restore_error}"
            ) from restore_error
