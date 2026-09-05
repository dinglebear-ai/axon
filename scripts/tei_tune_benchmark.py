"""HTTP benchmark helpers for the TEI tuning tool."""

from __future__ import annotations

import concurrent.futures
import json
import math
from pathlib import Path
import random
import statistics
import time
import urllib.request


FIXED_ARGS = [
    "--model-id", "Qwen/Qwen3-Embedding-0.6B",
    "--dtype", "float16",
    "--pooling", "last-token",
    "--auto-truncate",
]


def command_for(config: dict[str, int]) -> list[str]:
    args = FIXED_ARGS[:4]
    for key in (
        "max-concurrent-requests", "max-batch-tokens", "max-batch-requests",
        "max-client-batch-size", "tokenization-workers",
    ):
        args.extend((f"--{key}", str(config[key])))
    args.extend(FIXED_ARGS[4:])
    return args


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


def benchmark_once(tei, requests: int, batch_size: int, concurrency: int,
                   sample_chars: int = 1168) -> dict:
    sample = benchmark_sample(sample_chars)
    batch = [sample] * batch_size
    latencies: list[float] = []
    errors: list[str] = []

    def measured_request(_: int) -> int:
        started = time.perf_counter()
        try:
            return request_embeddings(tei.url, batch)
        except Exception as error:
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
        "sample_chars": sample_chars, "inputs": total, "seconds": round(elapsed, 3),
        "inputs_per_second": round(total / elapsed, 2) if elapsed else 0,
        "latency_ms_p50": round(percentile(latencies, 0.50), 2),
        "latency_ms_p95": round(percentile(latencies, 0.95), 2),
        "latency_ms_p99": round(percentile(latencies, 0.99), 2),
        "errors": len(errors), "error_samples": errors[:3],
    }


def benchmark(tei, requests: int, batch_size: int, concurrency: int,
              sample_chars: int) -> None:
    print(json.dumps(
        benchmark_once(tei, requests, batch_size, concurrency, sample_chars), indent=2
    ))


def sweep_client(tei, total_inputs: int, repeats: int, batch_sizes: list[int],
                 concurrencies: list[int], output: Path | None,
                 sample_chars: int = 1168, seed: int = 0) -> None:
    candidates = [(batch, concurrency) for batch in batch_sizes for concurrency in concurrencies]
    trials_by_candidate = {candidate: [] for candidate in candidates}
    rng = random.Random(seed)
    for batch_size, concurrency in candidates:
        requests, actual_inputs = fixed_input_shape(total_inputs, batch_size)
        benchmark_once(tei, max(1, concurrency), batch_size, concurrency, sample_chars)
    schedule = []
    for round_index in range(repeats):
        ordered = candidates.copy()
        rng.shuffle(ordered)
        for batch_size, concurrency in ordered:
            requests, _ = fixed_input_shape(total_inputs, batch_size)
            trials_by_candidate[(batch_size, concurrency)].append(
                benchmark_once(tei, requests, batch_size, concurrency, sample_chars)
            )
            schedule.append({"round": round_index, "batch_size": batch_size, "concurrency": concurrency})
    results = []
    for batch_size, concurrency in candidates:
        _, actual_inputs = fixed_input_shape(total_inputs, batch_size)
        trials = trials_by_candidate[(batch_size, concurrency)]
        rates = [trial["inputs_per_second"] for trial in trials if trial["errors"] == 0]
        result = {
            "batch_size": batch_size, "concurrency": concurrency,
            "requested_inputs": total_inputs, "actual_inputs": actual_inputs,
            "repeats": repeats, "trials": trials, "successful_trials": len(rates),
            "median_inputs_per_second": round(statistics.median(rates), 2) if rates else 0,
            "stdev_inputs_per_second": round(statistics.stdev(rates), 2) if len(rates) > 1 else 0,
        }
        results.append(result)
        print(json.dumps(result), flush=True)
    eligible = [result for result in results if result["successful_trials"] == repeats]
    eligible.sort(key=lambda result: result["median_inputs_per_second"], reverse=True)
    confirmation = []
    if len(eligible) >= 2:
        finalists = [(item["batch_size"], item["concurrency"]) for item in eligible[:2]]
        for round_index in range(2):
            for batch_size, concurrency in (finalists if round_index == 0 else finalists[::-1]):
                requests, _ = fixed_input_shape(total_inputs, batch_size)
                trial = benchmark_once(tei, requests, batch_size, concurrency, sample_chars)
                confirmation.append({"round": round_index, "batch_size": batch_size,
                                     "concurrency": concurrency, "trial": trial})
    report = {"kind": "tei-http-client-sweep", "seed": seed, "schedule": schedule,
              "results": results, "paired_confirmation": confirmation}
    if output:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(report, indent=2) + "\n")
