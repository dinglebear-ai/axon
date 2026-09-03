#!/usr/bin/env python3
"""Run reproducible Axon/Qdrant write-path sweeps against a frozen corpus."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import statistics
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import NamedTuple


OWNED_PREFIX = "axon_qdrant_bench_"
DEFAULT_QUERIES = (
    "configure MCP servers",
    "Claude Code hooks",
    "permission modes",
    "Agent SDK sessions",
    "sandbox network configuration",
)


class Variant(NamedTuple):
    name: str
    transport: str
    parallelism: int
    async_writes: bool
    bulk_load: bool
    hnsw_m: int
    hnsw_ef_construct: int
    quantization: bool


def default_variants() -> list[Variant]:
    baseline = [Variant(f"rest-p{p}", "rest", p, False, False, 32, 256, True) for p in range(1, 5)]
    return baseline + [
        Variant("rest-p2-quant-off", "rest", 2, False, False, 32, 256, False),
        Variant("grpc-p2", "grpc", 2, False, False, 32, 256, True),
        Variant("grpc-async-p2", "grpc", 2, True, False, 32, 256, True),
        Variant("grpc-async-bulk-p2", "grpc", 2, True, True, 32, 256, True),
        Variant("grpc-p4", "grpc", 4, False, False, 32, 256, True),
        Variant("grpc-async-p4", "grpc", 4, True, False, 32, 256, True),
        Variant("hnsw-32-256", "grpc", 4, True, True, 32, 256, True),
        Variant("hnsw-16-128", "grpc", 4, True, True, 16, 128, True),
        Variant("hnsw-16-100", "grpc", 4, True, True, 16, 100, True),
    ]


def variant_environment(variant: Variant, grpc_url: str) -> dict[str, str]:
    return {
        "AXON_QDRANT_TRANSPORT": variant.transport,
        "QDRANT_GRPC_URL": grpc_url,
        "AXON_QDRANT_UPSERT_PARALLELISM": str(variant.parallelism),
        "AXON_QDRANT_ASYNC_WRITES": str(variant.async_writes).lower(),
        "AXON_QDRANT_BULK_LOAD": str(variant.bulk_load).lower(),
        "AXON_QDRANT_HNSW_M": str(variant.hnsw_m),
        "AXON_QDRANT_HNSW_EF_CONSTRUCT": str(variant.hnsw_ef_construct),
        "AXON_QDRANT_QUANTIZATION_ENABLED": str(variant.quantization).lower(),
    }


def _slug(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "_", value.lower()).strip("_")


def owned_collection(run_id: str, variant_name: str) -> str:
    return f"{OWNED_PREFIX}{_slug(run_id)}_{_slug(variant_name)}"


def assert_owned_collection(collection: str) -> None:
    if not collection.startswith(OWNED_PREFIX):
        raise ValueError(f"collection lacks owned benchmark prefix {OWNED_PREFIX!r}: {collection}")


def mean_overlap(baseline: list[list[str]], candidate: list[list[str]]) -> float:
    if len(baseline) != len(candidate) or not baseline:
        raise ValueError("baseline and candidate query result sets must have equal non-zero length")
    scores = []
    for expected, actual in zip(baseline, candidate, strict=True):
        expected_set = set(expected)
        scores.append(len(expected_set & set(actual)) / len(expected_set) if expected_set else 1.0)
    return sum(scores) / len(scores)


def request_json(url: str, method: str = "GET") -> dict:
    request = urllib.request.Request(url, method=method)
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.loads(response.read())


def delete_owned_collection(qdrant_url: str, collection: str) -> None:
    assert_owned_collection(collection)
    try:
        request_json(f"{qdrant_url.rstrip('/')}/collections/{collection}", "DELETE")
    except urllib.error.HTTPError as error:
        if error.code != 404:
            raise


def result_key(row: dict) -> str:
    return row.get("citation", {}).get("chunk_id") or row.get("url") or row.get("citation", {}).get("canonical_uri") or ""


def query_urls(binary: Path, collection: str, query: str, env: dict[str, str]) -> list[str]:
    result = subprocess.run(
        [str(binary), "query", query, "--collection", collection, "--limit", "10", "--json", "--quiet"],
        env=env, text=True, capture_output=True, check=True,
    )
    rows = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
    return [result_key(row) for row in rows]


def select_variants(variants: list[Variant], names: list[str] | None) -> list[Variant]:
    if not names:
        return variants
    by_name = {variant.name: variant for variant in variants}
    missing = [name for name in names if name not in by_name]
    if missing:
        raise ValueError(f"unknown variants: {', '.join(missing)}")
    return [by_name[name] for name in names]


def frozen_corpus(source: Path, destination: Path) -> int:
    files = sorted(source.glob("code-claude-com-*.md"))
    if not files:
        raise ValueError(f"no code.claude.com Markdown files found in {source}")
    destination.mkdir(parents=True, exist_ok=True)
    for path in files:
        shutil.copy2(path, destination / path.name)
    return len(files)


def corpus_digest(corpus: Path) -> str:
    import hashlib

    digest = hashlib.sha256()
    for path in sorted(corpus.glob("code-claude-com-*.md")):
        digest.update(path.name.encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return digest.hexdigest()


def file_digest(path: Path) -> str:
    import hashlib

    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def interleaved_runs(variants: list[Variant], repetitions: int) -> list[tuple[int, Variant]]:
    if repetitions < 2:
        raise ValueError("repetitions must be at least 2")
    runs = []
    for repetition in range(repetitions):
        order = variants if repetition % 2 == 0 else list(reversed(variants))
        runs.extend((repetition + 1, variant) for variant in order)
    return runs


def summarize_rows(rows: list[dict]) -> list[dict]:
    names = list(dict.fromkeys(row["variant"]["name"] for row in rows))
    summaries = []
    for name in names:
        samples = [row["seconds"] for row in rows if row["variant"]["name"] == name and "seconds" in row]
        if samples:
            summaries.append({
                "variant": name,
                "samples": len(samples),
                "median_seconds": round(statistics.median(samples), 3),
                "min_seconds": round(min(samples), 3),
                "max_seconds": round(max(samples), 3),
            })
    return summaries


def equivalence_report(rows: list[dict], repetitions: int) -> dict:
    successful = [row for row in rows if "seconds" in row]
    reasons = []
    if len(successful) != len(rows):
        reasons.append("one or more runs failed")
    sample_counts = {}
    for row in successful:
        name = row["variant"]["name"]
        sample_counts[name] = sample_counts.get(name, 0) + 1
    if any(count != repetitions for count in sample_counts.values()):
        reasons.append("one or more variants have an incomplete sample set")
    points = {row.get("points") for row in successful}
    if len(points) > 1 or None in points:
        reasons.append("point counts differ or are missing")
    if any(row.get("status") != "green" for row in successful):
        reasons.append("one or more collections are not green")
    return {"valid": not reasons, "reasons": reasons, "point_count": next(iter(points), None)}


def service_identity(url: str) -> dict:
    try:
        return request_json(url.rstrip("/"))
    except Exception as error:
        return {"unavailable": type(error).__name__}


def run_variant(args: argparse.Namespace, variant: Variant, corpus: Path, run_id: str, repetition: int) -> dict:
    collection = owned_collection(run_id, f"{variant.name}-r{repetition}")
    assert_owned_collection(collection)
    env = os.environ.copy()
    env.update(variant_environment(variant, args.grpc_url))
    env.update({"QDRANT_URL": args.qdrant_url, "AXON_QDRANT_URL": args.qdrant_url, "TEI_URL": args.tei_url})
    with tempfile.TemporaryDirectory(prefix=f"axon-qdrant-{_slug(variant.name)}-") as data_dir:
        env["AXON_DATA_DIR"] = data_dir
        delete_owned_collection(args.qdrant_url, collection)
        started = time.perf_counter()
        command = [str(args.binary), "source", str(corpus), "--collection", collection, "--wait", "true", "--json", "--quiet"]
        completed = subprocess.run(command, env=env, text=True, capture_output=True)
        seconds = time.perf_counter() - started
        if completed.returncode:
            raise RuntimeError(f"{variant.name} failed: {completed.stderr[-4000:]}")
        info = request_json(f"{args.qdrant_url.rstrip('/')}/collections/{collection}").get("result", {})
        results = [query_urls(args.binary, collection, query, env) for query in args.queries]
        return {
            "variant": variant._asdict(), "repetition": repetition, "collection": collection, "seconds": round(seconds, 3),
            "points": info.get("points_count"), "indexed_vectors": info.get("indexed_vectors_count"),
            "status": info.get("status"), "optimizer_status": info.get("optimizer_status"),
            "query_results": results,
        }


def checkpoint(path: Path | None, run_id: str, documents: int, queries: tuple[str, ...], rows: list[dict]) -> None:
    if path:
        path.write_text(json.dumps({"run_id": run_id, "documents": documents, "queries": queries, "results": rows}, indent=2) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--execute", action="store_true", help="execute instead of printing the matrix")
    parser.add_argument("--binary", type=Path, default=Path("target/release/axon"))
    parser.add_argument("--source", type=Path, default=Path("~/.axon/output/markdown").expanduser())
    parser.add_argument("--qdrant-url", default="http://127.0.0.1:53333")
    parser.add_argument("--grpc-url", default="http://127.0.0.1:53334")
    parser.add_argument("--tei-url", default="http://tootie:52000")
    parser.add_argument("--run-id", default=time.strftime("%Y%m%d_%H%M%S"))
    parser.add_argument("--keep-collections", action="store_true")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--query", dest="queries", action="append")
    parser.add_argument("--variant", dest="variant_names", action="append")
    parser.add_argument("--repetitions", type=int, default=3)
    args = parser.parse_args()
    args.queries = tuple(args.queries or DEFAULT_QUERIES)
    variants = select_variants(default_variants(), args.variant_names)
    if not args.execute:
        print(json.dumps([variant._asdict() for variant in variants], indent=2))
        return 0
    args.binary = args.binary.resolve()
    if not args.binary.is_file():
        parser.error(f"binary not found: {args.binary}")
    with tempfile.TemporaryDirectory(prefix="axon-code-claude-corpus-") as directory:
        corpus = Path(directory)
        document_count = frozen_corpus(args.source, corpus)
        digest = corpus_digest(corpus)
        rows = []
        try:
            for repetition, variant in interleaved_runs(variants, args.repetitions):
                print(f"benchmarking {variant.name} repetition {repetition}...", flush=True)
                try:
                    rows.append(run_variant(args, variant, corpus, args.run_id, repetition))
                except Exception as error:
                    rows.append({"variant": variant._asdict(), "repetition": repetition, "status": "failed", "error": str(error)})
                    print(f"{variant.name} failed: {error}", flush=True)
                checkpoint(args.output, args.run_id, document_count, args.queries, rows)
        finally:
            if not args.keep_collections:
                for repetition, variant in interleaved_runs(variants, args.repetitions):
                    delete_owned_collection(args.qdrant_url, owned_collection(args.run_id, f"{variant.name}-r{repetition}"))
    baseline = next(row["query_results"] for row in rows if "query_results" in row)
    for row in rows:
        if "query_results" in row:
            row["recall_overlap_at_10"] = round(mean_overlap(baseline, row.pop("query_results")), 4)
    report = {
        "run_id": args.run_id,
        "documents": document_count,
        "corpus_sha256": digest,
        "repetitions": args.repetitions,
        "interleaving": "alternating forward/reverse",
        "queries": args.queries,
        "runtime": {
            "binary": str(args.binary),
            "binary_sha256": file_digest(args.binary),
            "qdrant_url": args.qdrant_url,
            "qdrant_identity": service_identity(args.qdrant_url),
            "tei_url": args.tei_url,
            "tei_identity": service_identity(f"{args.tei_url.rstrip('/')}/info"),
        },
        "equivalence": equivalence_report(rows, args.repetitions),
        "summaries": summarize_rows(rows),
        "results": rows,
    }
    rendered = json.dumps(report, indent=2) + "\n"
    if args.output:
        args.output.write_text(rendered)
    print(rendered, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
