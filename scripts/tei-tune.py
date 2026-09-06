#!/usr/bin/env python3
"""Safely inspect, tune, benchmark, and roll back TEI on tootie."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time
import urllib.error
import urllib.request

from tei_tune_benchmark import (
    benchmark,
    command_for,
    sweep_client,
)
from tei_tune_remote import TeiRemote

from tei_tune_runtime import (
    PRESETS,
    docker_run_from_snapshot,
    option_value,
    resolve_config,
    secondary_network_commands,
)

# LEARNED: importing helpers only so tests can reach them makes the production
# entrypoint advertise dependencies it does not use.
# PATTERN: tests import helper modules directly; this entrypoint imports only
# names referenced by its runtime implementation.



class Tei(TeiRemote):
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
        snapshot_command = snapshot.get("cmd") or []
        snapshot_model = option_value(snapshot_command, "--model-id")
        device_requests = snapshot.get("host_config", {}).get("DeviceRequests") or []
        device_ids = device_requests[0].get("DeviceIDs") or [] if device_requests else []
        ok, detail = self.ready(expected={
            "image": snapshot.get("image"),
            "image_id": snapshot.get("image_id"),
            "model_id": snapshot_model,
            "gpu": str(device_ids[0]) if device_ids else "",
        })
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
            "-p", f"127.0.0.1:{self.port}:80", "-e", "HF_HUB_CACHE=/data",
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

    def readiness_attestation(self, expected: dict, info: dict) -> tuple[bool, str]:
        inspected = self.inspect()
        if not inspected.get("State", {}).get("Running"):
            return False, "replacement container is not running"
        actual_image = inspected.get("Config", {}).get("Image")
        if actual_image != expected.get("image"):
            return False, f"image mismatch: expected {expected.get('image')}, got {actual_image}"
        expected_image_id = expected.get("image_id")
        actual_image_id = inspected.get("Image")
        if "image_id" in expected and (
            not expected_image_id or actual_image_id != expected_image_id
        ):
            return False, f"image ID mismatch: expected {expected_image_id}, got {actual_image_id}"
        if "image_id" in expected:
            published = inspected.get("NetworkSettings", {}).get("Ports", {}).get("80/tcp") or []
            if not any(
                binding.get("HostPort") == str(self.port)
                and binding.get("HostIp") in ("127.0.0.1", "::1")
                for binding in published
            ):
                return False, f"published port mismatch: expected loopback:{self.port}, got {published}"
        command = inspected.get("Config", {}).get("Cmd") or []
        expected_model = expected.get("model_id")
        actual_model = option_value(command, "--model-id")
        served_model = info.get("model_id") or info.get("modelId")
        if expected_model and (actual_model != expected_model or served_model != expected_model):
            return False, (
                f"model mismatch: expected {expected_model}, "
                f"container={actual_model}, endpoint={served_model}"
            )
        requests = inspected.get("HostConfig", {}).get("DeviceRequests") or []
        expected_gpu = str(expected.get("gpu", ""))
        if not requests or requests[0].get("Driver") != "nvidia":
            return False, "replacement container has no NVIDIA device request"
        device_ids = [str(value) for value in requests[0].get("DeviceIDs") or []]
        if expected_gpu and device_ids and expected_gpu not in device_ids:
            return False, f"GPU mismatch: expected device {expected_gpu}, got {device_ids}"
        activity = self.remote([
            "docker", "exec", self.container, "nvidia-smi",
            "--query-compute-apps=process_name", "--format=csv,noheader,nounits",
        ], check=False)
        if activity.returncode or not activity.stdout.strip():
            return False, "replacement container has no observed NVIDIA compute activity"
        return True, json.dumps(info)

    def ready(self, timeout: int = 90, expected: dict | None = None) -> tuple[bool, str]:
        deadline = time.monotonic() + timeout
        last = "no response"
        while time.monotonic() < deadline:
            try:
                with urllib.request.urlopen(f"{self.url}/info", timeout=3) as response:
                    if response.status == 200:
                        body = response.read().decode()
                        if expected is None:
                            return True, body
                        try:
                            info = json.loads(body)
                        except json.JSONDecodeError as error:
                            last = f"invalid TEI info response: {error}"
                            continue
                        attested, attestation = self.readiness_attestation(expected, info)
                        if attested:
                            return True, attestation
                        last = attestation
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
        model_id = option_value(command, "--model-id")
        image_id = self.resolve_image_id(image)
        ok, detail = self.ready(expected={
            "image": image,
            "image_id": image_id,
            "model_id": model_id,
            "gpu": str(getattr(self, "gpu", "")),
        })
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
                         positive_int(str(args.sample_chars)), args.seed)
        return 0
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
    sweep.add_argument("--seed", type=int, default=0, help="Recorded shuffle seed for interleaved trials")
