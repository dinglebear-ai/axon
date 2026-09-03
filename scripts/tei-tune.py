#!/usr/bin/env python3
"""Safely inspect, tune, benchmark, and roll back TEI on tootie."""

from __future__ import annotations

import argparse
import contextlib
import fcntl
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time
import urllib.error
import urllib.request

import tei_tune_benchmark
from tei_tune_benchmark import (
    benchmark,
    benchmark_once,
    benchmark_sample,
    command_for,
    entrypoint_from_snapshot,
    fixed_input_shape,
    percentile,
    request_embeddings,
    sweep_client,
)

from tei_tune_runtime import (
    PRESETS,
    docker_run_from_snapshot,
    resolve_config,
    secondary_network_commands,
    validate_container_name,
    validate_ssh_host,
)


class Tei:
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

    def snapshot(self) -> dict:
        inspected = self.inspect()
        return {
            "version": 1,
            "created_at": time.time(),
            "image": inspected["Config"]["Image"],
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

    def rollback_to_snapshot(self, snapshot: dict) -> None:
        with self.mutation_lock():
            self._rollback_to_snapshot(snapshot)

    def swap_with_saved_snapshot(self) -> None:
        with self.mutation_lock():
            target = json.loads(self.state.read_text())
            current = self.snapshot()
            self._rollback_to_snapshot(target, discard_parked=False)
            try:
                self.save_snapshot(current)
            except Exception as save_error:
                self.restore_after_failure(
                    f"rollback target became ready but saving the prior configuration failed ({save_error})"
                )
                raise RuntimeError(
                    f"saving the prior configuration failed ({save_error}); current TEI configuration restored"
                ) from save_error
            self.discard_parked_after_success("saved rollback configuration")

    def _rollback_to_snapshot(self, snapshot: dict, *, discard_parked: bool = True) -> None:
        # Validate every unsupported setting before stopping the current service.
        docker_run_from_snapshot(self.container, snapshot)
        secondary_network_commands(self.container, snapshot)
        self.park_current()
        try:
            self.deploy_snapshot(snapshot)
        except RuntimeError as deploy_error:
            self.restore_after_failure(f"rollback target deployment failed ({deploy_error})")
            raise RuntimeError(
                f"rollback target deployment failed ({deploy_error}); current TEI configuration restored"
            ) from deploy_error
        ok, detail = self.ready()
        if ok:
            if discard_parked:
                self.discard_parked_after_success("rollback configuration")
            return
        self.restore_after_failure(f"rollback target failed readiness ({detail})")
        restored, restore_detail = self.ready()
        if not restored:
            raise RuntimeError(
                f"rollback target failed readiness ({detail}); restoring current configuration also failed: {restore_detail}"
            )
        raise RuntimeError(f"rollback target failed readiness; current config restored: {detail}")

    def deploy(
        self, image: str, command: list[str], entrypoint: str | None = None
    ) -> None:
        self.remote(["docker", "rm", "-f", self.container], check=False)
        run = [
            "docker", "run", "-d", "--name", self.container,
            "--restart", "unless-stopped", "--network", self.network,
            "--runtime=nvidia", "--gpus", f"device={self.gpu}",
            "-p", f"{self.port}:80", "-e", "HF_HUB_CACHE=/data",
            "-v", f"{self.cache}:/data",
        ]
        selected_entrypoint = entrypoint or self.entrypoint
        if selected_entrypoint:
            run.extend(("--entrypoint", selected_entrypoint))
        run.extend((image, *command))
        result = self.remote(run, check=False)
        if result.returncode:
            raise RuntimeError(result.stderr.strip() or result.stdout.strip())

    def deploy_snapshot(self, snapshot: dict) -> None:
        run_command = docker_run_from_snapshot(self.container, snapshot)
        network_commands = secondary_network_commands(self.container, snapshot)
        self.remote(["docker", "rm", "-f", self.container], check=False)
        environment = snapshot.get("config", {}).get("Env") or []
        input_text = "\n".join(environment) + ("\n" if environment else "")
        result = self.remote(
            run_command,
            check=False,
            input_text=input_text,
        )
        if result.returncode:
            raise RuntimeError(result.stderr.strip() or result.stdout.strip())
        for command in network_commands:
            result = self.remote(command, check=False)
            if result.returncode:
                detail = result.stderr.strip() or result.stdout.strip()
                raise RuntimeError(f"failed to attach rollback network {command[-2]}: {detail}")

    def ready(self, timeout: int = 90) -> tuple[bool, str]:
        deadline = time.monotonic() + timeout
        last = "no response"
        while time.monotonic() < deadline:
            try:
                with urllib.request.urlopen(f"{self.url}/info", timeout=3) as response:
                    if response.status == 200:
                        return True, response.read().decode()
            except (OSError, urllib.error.URLError) as error:
                last = str(error)
            time.sleep(2)
        logs = self.remote(["docker", "logs", "--tail", "80", self.container], check=False)
        detail = (logs.stdout + logs.stderr).strip()
        lowered = detail.lower()
        if "out of memory" in lowered or "cuda error" in lowered:
            last = f"CUDA startup failure: {detail[-1200:]}"
        return False, last

    def apply(self, image: str, command: list[str], dry_run: bool) -> None:
        if dry_run:
            print(shlex.join(["docker", "run", "…", image, *command]))
            return
        with self.mutation_lock():
            self._apply(image, command)

    def _apply(self, image: str, command: list[str]) -> None:
        previous = self.snapshot()
        self.save_snapshot(previous)
        print(f"Saved rollback snapshot to {self.state}")
        self.park_current()
        try:
            self.deploy(image, command)
        except RuntimeError as deploy_error:
            print(f"New configuration failed deployment: {deploy_error}", file=sys.stderr)
            self.restore_after_failure(f"replacement deployment failed ({deploy_error})")
            restored, restore_detail = self.ready()
            if not restored:
                raise RuntimeError(
                    f"replacement deployment failed ({deploy_error}); automatic rollback also failed: {restore_detail}"
                ) from deploy_error
            raise RuntimeError(
                f"replacement deployment failed ({deploy_error}); previous TEI configuration restored"
            ) from deploy_error
        ok, detail = self.ready()
        if ok:
            self.discard_parked_after_success("new TEI configuration")
            print("TEI is ready; new configuration retained.")
            return
        print(f"New configuration failed readiness: {detail}", file=sys.stderr)
        print("Rolling back the previous image and command…", file=sys.stderr)
        self.restore_after_failure(f"replacement failed readiness ({detail})")
        restored, restore_detail = self.ready()
        if not restored:
            raise RuntimeError(f"automatic rollback also failed: {restore_detail}")
        raise RuntimeError("new configuration rejected; previous TEI configuration restored")


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--host", default=os.getenv("TEI_TUNE_HOST", "tootie"))
    result.add_argument("--container", default=os.getenv("TEI_TUNE_CONTAINER", "axon-tei"))
    result.add_argument("--url", default=os.getenv("TEI_TUNE_URL", "http://10.1.0.2:52000"))
    result.add_argument("--image", default=os.getenv("TEI_TUNE_IMAGE", "ghcr.io/huggingface/text-embeddings-inference:latest"))
    result.add_argument("--port", default=os.getenv("TEI_TUNE_PORT", "52000"))
    result.add_argument("--network", default=os.getenv("TEI_TUNE_NETWORK", "axon"))
    result.add_argument("--gpu", default=os.getenv("TEI_TUNE_GPU", "0"))
    result.add_argument("--cache", default=os.getenv("TEI_TUNE_CACHE", "/mnt/user/appdata/axon/tei"))
    result.add_argument("--entrypoint", default=os.getenv("TEI_TUNE_ENTRYPOINT"))
    result.add_argument("--state-dir", default=os.getenv("TEI_TUNE_STATE_DIR", "~/.axon/tei-tune"))
    commands = result.add_subparsers(dest="action", required=True)
    commands.add_parser("presets")
    commands.add_parser("status")
    apply = commands.add_parser("apply")
    apply.add_argument("preset", choices=sorted(PRESETS))
    apply.add_argument("--set", action="append", default=[], metavar="KEY=VALUE")
    apply.add_argument("--allow-unsafe", action="store_true")
    apply.add_argument("--dry-run", action="store_true")
    commands.add_parser("rollback")
    bench = commands.add_parser("benchmark")
    bench.add_argument("--requests", type=int, default=32)
    bench.add_argument("--batch-size", type=int, default=32)
    bench.add_argument("--concurrency", type=int, default=16)
    bench.add_argument("--sample-chars", type=int, default=1168)
    sweep = commands.add_parser("sweep-client")
    sweep.add_argument("--total-inputs", type=int, default=2048)
    sweep.add_argument("--repeats", type=int, default=3)
    sweep.add_argument("--batch-sizes", default="1,4,8,16,32,64,128")
    sweep.add_argument("--concurrencies", default="1,2,4,8,16")
    sweep.add_argument("--output", type=Path)
    sweep.add_argument("--sample-chars", type=int, default=1168)
    return result


def main() -> int:
    args = parser().parse_args()
    tei = Tei(args)
    try:
        if args.action == "presets":
            print(json.dumps(PRESETS, indent=2))
        elif args.action == "status":
            inspected = tei.inspect()
            ok, info = tei.ready(timeout=5)
            print(json.dumps({
                "host": tei.host, "container": tei.container,
                "image": inspected["Config"]["Image"], "cmd": inspected["Config"]["Cmd"],
                "ready": ok, "info": json.loads(info) if ok else info,
            }, indent=2))
        elif args.action == "apply":
            config = resolve_config(args.preset, args.set, args.allow_unsafe)
            tei.apply(args.image, command_for(config), args.dry_run)
        elif args.action == "rollback":
            tei.swap_with_saved_snapshot()
            print("Rollback target is ready; prior current configuration is now the rollback snapshot.")
        elif args.action == "benchmark":
            for value, name in ((args.requests, "requests"), (args.batch_size, "batch-size"), (args.concurrency, "concurrency")):
                if value <= 0:
                    raise ValueError(f"{name} must be positive")
            benchmark(
                tei, args.requests, args.batch_size, args.concurrency,
                positive_int(str(args.sample_chars)),
            )
        elif args.action == "sweep-client":
            batch_sizes = [positive_int(value) for value in args.batch_sizes.split(",")]
            concurrencies = [positive_int(value) for value in args.concurrencies.split(",")]
            sweep_client(tei, positive_int(str(args.total_inputs)), positive_int(str(args.repeats)),
                         batch_sizes, concurrencies, args.output,
                         positive_int(str(args.sample_chars)))
        return 0
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
