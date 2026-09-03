#!/usr/bin/env python3
"""Safely inspect, tune, benchmark, and roll back TEI on tootie."""

from __future__ import annotations

import argparse
import concurrent.futures
import contextlib
import fcntl
import json
import os
from pathlib import Path
import math
import statistics
import shlex
import subprocess
import sys
import time
import urllib.error
import urllib.request


PRESETS = {
    "rtx4070-axon": {
        "max-concurrent-requests": 1024,
        "max-batch-tokens": 163840,
        "max-batch-requests": 16,
        "max-client-batch-size": 128,
        "tokenization-workers": 16,
    },
    "stable": {
        "max-concurrent-requests": 1024,
        "max-batch-tokens": 163840,
        "max-batch-requests": 16,
        "max-client-batch-size": 128,
        "tokenization-workers": 16,
    },
    "admission": {
        "max-concurrent-requests": 1024,
        "max-batch-tokens": 196608,
        "max-batch-requests": 1024,
        "max-client-batch-size": 256,
        "tokenization-workers": 32,
    },
    "probe-212k": {
        "max-concurrent-requests": 1024,
        "max-batch-tokens": 212992,
        "max-batch-requests": 1024,
        "max-client-batch-size": 256,
        "tokenization-workers": 32,
    },
}
KNOBS = frozenset(next(iter(PRESETS.values())))
FIXED_ARGS = [
    "--model-id", "Qwen/Qwen3-Embedding-0.6B",
    "--dtype", "float16",
    "--pooling", "last-token",
    "--auto-truncate",
]


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise ValueError("must be a positive integer")
    return parsed


def resolve_config(preset: str, overrides: list[str], allow_unsafe: bool) -> dict[str, int]:
    config = dict(PRESETS[preset])
    for item in overrides:
        if "=" not in item:
            raise ValueError(f"override must be KEY=VALUE: {item}")
        key, value = item.split("=", 1)
        key = key.strip().replace("_", "-")
        if key not in KNOBS:
            raise ValueError(f"unknown knob {key!r}; choose from {', '.join(sorted(KNOBS))}")
        config[key] = positive_int(value)
    if config["max-batch-tokens"] > 212992 and not allow_unsafe:
        raise ValueError(
            "max-batch-tokens above 212992 requires --allow-unsafe; 262144 caused CUDA OOM on the RTX 4070"
        )
    return config


def command_for(config: dict[str, int]) -> list[str]:
    args = FIXED_ARGS[:4]
    for key in (
        "max-concurrent-requests", "max-batch-tokens", "max-batch-requests",
        "max-client-batch-size", "tokenization-workers",
    ):
        args.extend((f"--{key}", str(config[key])))
    args.extend(FIXED_ARGS[4:])
    return args


def docker_run_from_snapshot(container: str, snapshot: dict) -> list[str]:
    config = snapshot.get("config", {})
    host = snapshot.get("host_config", {})
    run = ["docker", "run", "-d", "--name", container]
    restart = host.get("RestartPolicy", {}).get("Name")
    if restart and restart != "no":
        run.extend(("--restart", restart))
    network = snapshot.get("network_mode")
    if network:
        run.extend(("--network", network))
    runtime = host.get("Runtime")
    if runtime:
        run.extend(("--runtime", runtime))
    requests = host.get("DeviceRequests") or []
    if requests and requests[0].get("Driver") == "nvidia":
        device_ids = requests[0].get("DeviceIDs") or []
        run.extend(("--gpus", f"device={','.join(device_ids)}" if device_ids else "all"))
    for container_port, bindings in (host.get("PortBindings") or {}).items():
        for binding in bindings or []:
            published = f"{binding.get('HostIp')}:{binding['HostPort']}:{container_port}" if binding.get("HostIp") else f"{binding['HostPort']}:{container_port}"
            run.extend(("-p", published))
    for bind in host.get("Binds") or []:
        run.extend(("-v", bind))
    for env in config.get("Env") or []:
        run.extend(("-e", env))
    for key, value in (config.get("Labels") or {}).items():
        run.extend(("--label", f"{key}={value}"))
    if config.get("User"):
        run.extend(("--user", config["User"]))
    if config.get("WorkingDir"):
        run.extend(("--workdir", config["WorkingDir"]))
    entrypoint = snapshot.get("entrypoint") or []
    if entrypoint:
        run.extend(("--entrypoint", entrypoint[0]))
    run.append(snapshot["image"])
    run.extend(entrypoint[1:])
    run.extend(snapshot.get("cmd") or [])
    return run


class Tei:
    def __init__(self, args: argparse.Namespace):
        self.host = args.host
        self.container = args.container
        self.url = args.url.rstrip("/")
        self.image = args.image
        self.port = args.port
        self.network = args.network
        self.gpu = args.gpu
        self.cache = args.cache
        self.entrypoint = args.entrypoint
        state_name = f"{self.host}-{self.container}".replace("/", "_") + ".json"
        self.state = Path(args.state_dir).expanduser() / state_name

    def remote(self, argv: list[str], check: bool = True) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["ssh", self.host, shlex.join(argv)], text=True, capture_output=True, check=check
        )

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
            "network_mode": next(iter(inspected["NetworkSettings"]["Networks"]), "bridge"),
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
                yield
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
            self.discard_parked()

    def _rollback_to_snapshot(self, snapshot: dict, *, discard_parked: bool = True) -> None:
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
                self.discard_parked()
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
        self.remote(["docker", "rm", "-f", self.container], check=False)
        result = self.remote(docker_run_from_snapshot(self.container, snapshot), check=False)
        if result.returncode:
            raise RuntimeError(result.stderr.strip() or result.stdout.strip())

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
            self.discard_parked()
            print("TEI is ready; new configuration retained.")
            return
        print(f"New configuration failed readiness: {detail}", file=sys.stderr)
        print("Rolling back the previous image and command…", file=sys.stderr)
        self.restore_after_failure(f"replacement failed readiness ({detail})")
        restored, restore_detail = self.ready()
        if not restored:
            raise RuntimeError(f"automatic rollback also failed: {restore_detail}")
        raise RuntimeError("new configuration rejected; previous TEI configuration restored")


def request_embeddings(url: str, inputs: list[str]) -> int:
    body = json.dumps({"inputs": inputs, "truncate": True}).encode()
    request = urllib.request.Request(
        f"{url}/embed", data=body, headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(request, timeout=120) as response:
        payload = json.loads(response.read())
    return len(payload)


def entrypoint_from_snapshot(snapshot: dict) -> str | None:
    entrypoint = snapshot.get("entrypoint")
    if not entrypoint:
        return None
    if not isinstance(entrypoint, list) or len(entrypoint) != 1:
        raise ValueError(f"unsupported Docker entrypoint snapshot: {entrypoint!r}")
    return str(entrypoint[0])


def percentile(values: list[float], quantile: float) -> float:
    ordered = sorted(values)
    return ordered[max(0, math.ceil(len(ordered) * quantile) - 1)]


def fixed_input_shape(total_inputs: int, batch_size: int) -> tuple[int, int]:
    requests = math.ceil(total_inputs / batch_size)
    return requests, requests * batch_size


def benchmark_sample(sample_chars: int) -> str:
    base = (
        "Text embeddings inference benchmark for source acquisition, document chunking, "
        "hybrid retrieval, vector publication, and technical documentation. "
    )
    return (base * math.ceil(sample_chars / len(base)))[:sample_chars]


def benchmark_once(tei: Tei, requests: int, batch_size: int, concurrency: int,
                   sample_chars: int = 1168) -> dict:
    sample = benchmark_sample(sample_chars)
    batch = [sample] * batch_size
    latencies: list[float] = []
    errors: list[str] = []

    def measured_request(_: int) -> int:
        started = time.perf_counter()
        try:
            return request_embeddings(tei.url, batch)
        except Exception as error:  # Report every failed candidate instead of losing the sweep.
            errors.append(str(error))
            return 0
        finally:
            latencies.append((time.perf_counter() - started) * 1000)

    started = time.perf_counter()
    with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as pool:
        counts = list(pool.map(measured_request, range(requests)))
    elapsed = time.perf_counter() - started
    total = sum(counts)
    return {
        "requests": requests, "batch_size": batch_size, "concurrency": concurrency,
        "sample_chars": sample_chars,
        "inputs": total, "seconds": round(elapsed, 3),
        "inputs_per_second": round(total / elapsed, 2) if elapsed else 0,
        "latency_ms_p50": round(percentile(latencies, 0.50), 2),
        "latency_ms_p95": round(percentile(latencies, 0.95), 2),
        "latency_ms_p99": round(percentile(latencies, 0.99), 2),
        "errors": len(errors), "error_samples": errors[:3],
    }


def benchmark(tei: Tei, requests: int, batch_size: int, concurrency: int,
              sample_chars: int) -> None:
    print(json.dumps(
        benchmark_once(tei, requests, batch_size, concurrency, sample_chars), indent=2
    ))


def sweep_client(tei: Tei, total_inputs: int, repeats: int, batch_sizes: list[int],
                 concurrencies: list[int], output: Path | None,
                 sample_chars: int = 1168) -> None:
    results = []
    for batch_size in batch_sizes:
        requests, actual_inputs = fixed_input_shape(total_inputs, batch_size)
        for concurrency in concurrencies:
            benchmark_once(
                tei, max(1, concurrency), batch_size, concurrency, sample_chars
            )
            trials = [
                benchmark_once(tei, requests, batch_size, concurrency, sample_chars)
                for _ in range(repeats)
            ]
            rates = [trial["inputs_per_second"] for trial in trials if trial["errors"] == 0]
            result = {
                "batch_size": batch_size, "concurrency": concurrency,
                "requested_inputs": total_inputs, "actual_inputs": actual_inputs,
                "repeats": repeats, "trials": trials,
                "median_inputs_per_second": round(statistics.median(rates), 2) if rates else 0,
            }
            results.append(result)
            print(json.dumps(result), flush=True)
    report = {"kind": "tei-http-client-sweep", "results": results}
    if output:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(report, indent=2) + "\n")


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
