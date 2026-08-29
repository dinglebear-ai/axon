#!/usr/bin/env python3
"""Secure TEI-compatible MLX embedding server with aggregate telemetry."""

from __future__ import annotations

import argparse
import hmac
import os
import secrets
import threading
import time
from dataclasses import asdict, dataclass
from ipaddress import ip_address
from typing import Any

TEST_MODE = os.getenv("AXON_MLX_TEST_MODE") == "1"

MODEL_ID = os.getenv("MLX_TEI_MODEL_ID", "LiquidAI/LFM2.5-Embedding-350M")
BATCH_SIZE = max(1, int(os.getenv("MLX_TEI_BATCH_SIZE", "16")))
MAX_BATCH_TOKENS = max(1, int(os.getenv("MLX_TEI_MAX_BATCH_TOKENS", "8192")))
CACHE_LIMIT = max(0, int(os.getenv("MLX_TEI_CACHE_LIMIT_BYTES", str(4 * 1024**3))))
PIPELINE_DEPTH = max(1, int(os.getenv("MLX_TEI_PIPELINE_DEPTH", "2")))
DTYPE = os.getenv("MLX_TEI_DTYPE", "bfloat16")
AUTH_TOKEN = os.getenv("MLX_TEI_AUTH_TOKEN", "")
MAX_BODY_BYTES = max(1, int(os.getenv("MLX_TEI_MAX_BODY_BYTES", str(64 * 1024**2))))
MAX_INPUTS = max(1, int(os.getenv("MLX_TEI_MAX_INPUTS", "4096")))
MAX_INPUT_BYTES = max(1, int(os.getenv("MLX_TEI_MAX_INPUT_BYTES", str(8 * 1024**2))))
MAX_REQUEST_TOKENS = max(1, int(os.getenv("MLX_TEI_MAX_REQUEST_TOKENS", "1048576")))
MAX_JSON_DEPTH = max(1, int(os.getenv("MLX_TEI_MAX_JSON_DEPTH", "8")))
DIM = 1024


class RequestLimitError(ValueError):
    """A request exceeded a declared, non-truncating resource limit."""


@dataclass(frozen=True)
class BatchShape:
    rows: int
    lengths: tuple[int, ...]


@dataclass(frozen=True)
class PackingSummary:
    useful_tokens: int
    padded_tokens: int
    dispatches: int
    partial_dispatches: int
    rows_total: int
    row_capacity: int
    token_capacity: int

    @property
    def padding_ratio(self) -> float:
        return 0.0 if self.padded_tokens == 0 else 1.0 - self.useful_tokens / self.padded_tokens

    @property
    def row_occupancy(self) -> float:
        return 0.0 if self.row_capacity == 0 else self.rows_total / self.row_capacity

    @property
    def token_occupancy(self) -> float:
        return 0.0 if self.token_capacity == 0 else self.padded_tokens / self.token_capacity

def summarize_shapes(
    shapes: list[BatchShape], configured_batch_size: int, max_batch_tokens: int
) -> PackingSummary:
    useful = sum(sum(shape.lengths) for shape in shapes)
    padded = sum(shape.rows * max(shape.lengths, default=0) for shape in shapes)
    summary = PackingSummary(
        useful_tokens=useful,
        padded_tokens=padded,
        dispatches=len(shapes),
        partial_dispatches=sum(shape.rows < configured_batch_size for shape in shapes),
        rows_total=sum(shape.rows for shape in shapes),
        row_capacity=len(shapes) * configured_batch_size,
        token_capacity=len(shapes) * max_batch_tokens,
    )
    return summary


def interval_union_us(intervals_ns: list[tuple[int, int]]) -> int:
    valid = sorted((start, end) for start, end in intervals_ns if end >= start)
    if not valid:
        return 0
    total = 0
    current_start, current_end = valid[0]
    for start, end in valid[1:]:
        if start <= current_end:
            current_end = max(current_end, end)
        else:
            total += current_end - current_start
            current_start, current_end = start, end
    total += current_end - current_start
    return total // 1_000


def interval_idle_us(intervals_ns: list[tuple[int, int]], start_ns: int, end_ns: int) -> int:
    span_us = max(0, end_ns - start_ns) // 1_000
    return max(0, span_us - interval_union_us(intervals_ns))


def validate_bind(host: str, token: str) -> None:
    try:
        loopback = ip_address(host).is_loopback
    except ValueError:
        loopback = host.lower() == "localhost"
    if not loopback and not token:
        raise ValueError("non-loopback MLX bind requires MLX_TEI_AUTH_TOKEN")


def authorized(authorization: str | None, token: str) -> bool:
    if not token:
        return True
    supplied = authorization or ""
    prefix = "Bearer "
    if not supplied.startswith(prefix):
        return False
    return hmac.compare_digest(supplied[len(prefix) :].encode(), token.encode())


def json_depth(value: Any, depth: int = 0) -> int:
    if isinstance(value, dict):
        return max([depth] + [json_depth(v, depth + 1) for v in value.values()])
    if isinstance(value, list):
        return max([depth] + [json_depth(v, depth + 1) for v in value])
    return depth


def validate_payload(payload: Any) -> list[str]:
    if json_depth(payload) > MAX_JSON_DEPTH:
        raise RequestLimitError("json nesting exceeds configured limit")
    if not isinstance(payload, dict) or set(payload) - {"inputs", "truncate"}:
        raise RequestLimitError("body must contain only inputs and optional truncate")
    if payload.get("truncate") not in (None, False):
        raise RequestLimitError("truncation is not supported")
    texts = payload.get("inputs")
    if not isinstance(texts, list) or not texts or not all(isinstance(text, str) for text in texts):
        raise RequestLimitError("inputs must be a non-empty array of strings")
    if len(texts) > MAX_INPUTS:
        raise RequestLimitError("input row count exceeds configured limit")
    if any(len(text.encode("utf-8")) > MAX_INPUT_BYTES for text in texts):
        raise RequestLimitError("single input exceeds configured byte limit")
    return texts


if not TEST_MODE:
    import queue
    import sys
    from contextlib import asynccontextmanager

    tuned_python = os.getenv("MLX_TEI_TUNED_PYTHON_PATH", "")
    if tuned_python:
        sys.path.insert(0, tuned_python)

    import anyio
    import mlx.core as mx
    import numpy as np
    import orjson
    from fastapi import FastAPI, Request
    from fastapi.responses import ORJSONResponse, Response
    from mlx_embeddings import load

    MODEL = None
    TOKENIZER = None
    RAW_TOKENIZER = None
    PAD_ID = 0
    EMBED_STREAM = None
    WORK: queue.Queue = queue.Queue()
    METRICS_LOCK = threading.Lock()
    METRICS: dict[str, int | str] = {
        "epoch": secrets.token_hex(16),
        "requests": 0,
        "useful_tokens": 0,
        "padded_tokens": 0,
        "dispatches": 0,
        "partial_dispatches": 0,
        "rows_total": 0,
        "row_capacity": 0,
        "token_capacity": 0,
        "tokenize_us": 0,
        "serialize_us": 0,
        "request_wall_us": 0,
        "metal_busy_us": 0,
        "dispatcher_idle_us": 0,
    }

    class RequestState:
        __slots__ = (
            "vectors", "remaining", "done", "error", "intervals", "lock"
        )

        def __init__(self, count: int, batches: int):
            self.vectors = np.empty((count, DIM), dtype=np.float32)
            self.remaining = batches
            self.done = threading.Event()
            self.error: Exception | None = None
            self.intervals: list[tuple[int, int]] = []
            self.lock = threading.Lock()

    def cls_forward(ids: mx.array, mask: mx.array) -> mx.array:
        inner = MODEL.model
        h = inner.embed_tokens(ids)
        padding_mask = mask.astype(mx.bool_)
        additive = mx.where(
            padding_mask[:, None, None, :],
            mx.array(0, dtype=h.dtype),
            mx.array(-1e9, dtype=h.dtype),
        )
        for layer in inner.layers:
            normalized = layer.operator_norm(h)
            residual = (
                layer.self_attn(normalized, mask=additive, cache=None)
                if layer.is_attention_layer
                else MODEL._noncausal_conv(layer.conv, normalized, padding_mask)
            )
            h = h + residual
            h = h + layer.feed_forward(layer.ffn_norm(h))
        out = inner.embedding_norm(h[:, :1, :])
        for dense in MODEL.dense:
            out = dense(out)
        vectors = out[:, 0, :]
        vectors = vectors / mx.maximum(mx.linalg.norm(vectors, axis=-1, keepdims=True), 1e-9)
        return vectors.astype(mx.float32)

    def dispatcher() -> None:
        global EMBED_STREAM
        try:
            EMBED_STREAM = mx.new_stream(mx.gpu)
            with mx.stream(EMBED_STREAM):
                mx.eval(mx.zeros((1,)))
        except RuntimeError:
            EMBED_STREAM = mx.default_stream(mx.gpu)
        inflight: list[tuple[RequestState, np.ndarray, mx.array, int]] = []

        def drain_one() -> None:
            state, indices, out, start_ns = inflight.pop(0)
            try:
                mx.eval(out)
                state.vectors[indices] = np.asarray(out)
                end_ns = time.perf_counter_ns()
                with state.lock:
                    state.intervals.append((start_ns, end_ns))
            except Exception as exc:  # noqa: BLE001
                state.error = exc
            state.remaining -= 1
            if state.remaining == 0:
                state.done.set()

        while True:
            if inflight:
                try:
                    item = WORK.get_nowait()
                except queue.Empty:
                    drain_one()
                    continue
            else:
                item = WORK.get()
            state, indices, np_ids, np_mask = item
            try:
                start_ns = time.perf_counter_ns()
                with mx.stream(EMBED_STREAM):
                    out = cls_forward(mx.array(np_ids), mx.array(np_mask))
                    mx.async_eval(out)
            except Exception as exc:  # noqa: BLE001
                state.error = exc
                state.remaining -= 1
                if state.remaining == 0:
                    state.done.set()
                continue
            inflight.append((state, indices, out, start_ns))
            while len(inflight) >= PIPELINE_DEPTH:
                drain_one()

    def build_batches(token_rows: list[list[int]]):
        order = sorted(range(len(token_rows)), key=lambda index: len(token_rows[index]))
        groups: list[list[int]] = []
        batch: list[int] = []
        for index in order:
            rows = len(batch) + 1
            if batch and (rows > BATCH_SIZE or rows * len(token_rows[index]) > MAX_BATCH_TOKENS):
                groups.append(batch)
                batch = []
            batch.append(index)
        if batch:
            groups.append(batch)
        output = []
        for indices in groups:
            max_length = max(len(token_rows[index]) for index in indices)
            ids = np.full((len(indices), max_length), PAD_ID, dtype=np.int32)
            mask = np.zeros((len(indices), max_length), dtype=np.int32)
            for row_index, source_index in enumerate(indices):
                row = token_rows[source_index]
                ids[row_index, : len(row)] = row
                mask[row_index, : len(row)] = 1
            shape = BatchShape(len(indices), tuple(len(token_rows[index]) for index in indices))
            output.append((np.array(indices), ids, mask, shape))
        return output

    def embed_texts(texts: list[str]) -> tuple[np.ndarray, dict[str, int]]:
        request_start = time.perf_counter_ns()
        tokenize_start = request_start
        encodings = RAW_TOKENIZER.encode_batch_fast(texts)
        token_rows = [encoding.ids for encoding in encodings]
        tokenize_us = (time.perf_counter_ns() - tokenize_start) // 1_000
        total_tokens = sum(len(row) for row in token_rows)
        if total_tokens > MAX_REQUEST_TOKENS:
            raise RequestLimitError("aggregate tokens exceed configured limit")
        batches = build_batches(token_rows)
        state = RequestState(len(texts), len(batches))
        shapes = []
        for indices, ids, mask, shape in batches:
            shapes.append(shape)
            WORK.put((state, indices, ids, mask))
        state.done.wait()
        if state.error is not None:
            raise state.error
        request_end = time.perf_counter_ns()
        packing = summarize_shapes(shapes, BATCH_SIZE, MAX_BATCH_TOKENS)
        measurement = {
            "useful_tokens": packing.useful_tokens,
            "padded_tokens": packing.padded_tokens,
            "dispatches": packing.dispatches,
            "partial_dispatches": packing.partial_dispatches,
            "rows_total": packing.rows_total,
            "row_capacity": packing.row_capacity,
            "token_capacity": packing.token_capacity,
            "tokenize_us": tokenize_us,
            "request_wall_us": (request_end - request_start) // 1_000,
            "metal_busy_us": interval_union_us(state.intervals),
            "dispatcher_idle_us": interval_idle_us(state.intervals, request_start, request_end),
        }
        return state.vectors, measurement

    def record_metrics(measurement: dict[str, int]) -> None:
        with METRICS_LOCK:
            METRICS["requests"] = int(METRICS["requests"]) + 1
            for key, value in measurement.items():
                METRICS[key] = int(METRICS.get(key, 0)) + value

    def load_model() -> None:
        global MODEL, TOKENIZER, RAW_TOKENIZER, PAD_ID
        mx.set_cache_limit(CACHE_LIMIT)
        MODEL, wrapped = load(MODEL_ID)
        if DTYPE == "float16":
            MODEL.set_dtype(mx.float16)
        elif DTYPE == "float32":
            MODEL.set_dtype(mx.float32)
        TOKENIZER = getattr(wrapped, "_tokenizer", wrapped)
        RAW_TOKENIZER = getattr(TOKENIZER, "_tokenizer", None) or TOKENIZER.backend_tokenizer
        RAW_TOKENIZER.no_padding()
        RAW_TOKENIZER.no_truncation()
        probe = [encoding.ids for encoding in RAW_TOKENIZER.encode_batch_fast(["a", "a " * 700])]
        if len(probe[0]) == len(probe[1]) or max(map(len, probe)) < 600:
            raise RuntimeError("tokenizer padding/truncation still active")
        PAD_ID = TOKENIZER.pad_token_id
        threading.Thread(target=dispatcher, daemon=True, name="mlx-dispatcher").start()
        embed_texts(["warmup " * count for count in (4, 64, 256)])

    @asynccontextmanager
    async def lifespan(_: FastAPI):
        load_model()
        yield

    app = FastAPI(lifespan=lifespan, default_response_class=ORJSONResponse)

    def require_auth(request: Request) -> ORJSONResponse | None:
        if authorized(request.headers.get("authorization"), AUTH_TOKEN):
            return None
        return ORJSONResponse({"error": "unauthorized"}, status_code=401)

    async def read_limited_body(request: Request) -> bytes:
        declared = request.headers.get("content-length")
        if declared and int(declared) > MAX_BODY_BYTES:
            raise RequestLimitError("request body exceeds configured limit")
        body = bytearray()
        async for chunk in request.stream():
            body.extend(chunk)
            if len(body) > MAX_BODY_BYTES:
                raise RequestLimitError("request body exceeds configured limit")
        return bytes(body)

    @app.get("/info")
    def info(request: Request):
        denied = require_auth(request)
        if denied:
            return denied
        return {
            "model_id": MODEL_ID,
            "embedding_dimension": DIM,
            "batch_size": BATCH_SIZE,
            "max_batch_tokens": MAX_BATCH_TOKENS,
            "pipeline_depth": PIPELINE_DEPTH,
            "dtype": DTYPE,
            "server": "mlx-tei-direct-v3",
            "truncation": False,
        }

    @app.get("/health")
    def health() -> dict[str, str]:
        return {"status": "ready"}

    @app.get("/metrics")
    def metrics(request: Request):
        denied = require_auth(request)
        if denied:
            return denied
        with METRICS_LOCK:
            return dict(METRICS)

    @app.post("/embed")
    async def embed(request: Request) -> Response:
        denied = require_auth(request)
        if denied:
            return denied
        try:
            payload = orjson.loads(await read_limited_body(request))
            texts = validate_payload(payload)
            vectors, measurement = await anyio.to_thread.run_sync(
                embed_texts, texts, abandon_on_cancel=False
            )
            serialize_start = time.perf_counter_ns()
            encoded = orjson.dumps(vectors, option=orjson.OPT_SERIALIZE_NUMPY)
            measurement["serialize_us"] = (time.perf_counter_ns() - serialize_start) // 1_000
            record_metrics(measurement)
            return Response(encoded, media_type="application/json")
        except (RequestLimitError, orjson.JSONDecodeError, UnicodeDecodeError, ValueError):
            return ORJSONResponse({"error": "invalid or oversized request"}, status_code=400)

    def main() -> None:
        parser = argparse.ArgumentParser()
        parser.add_argument("--host", default=os.getenv("MLX_TEI_HOST", "127.0.0.1"))
        parser.add_argument("--port", type=int, default=int(os.getenv("MLX_TEI_PORT", "8084")))
        args = parser.parse_args()
        validate_bind(args.host, AUTH_TOKEN)
        import uvicorn

        uvicorn.run(app, host=args.host, port=args.port, workers=1, access_log=False)

else:
    app = None

    def main() -> None:
        raise RuntimeError("AXON_MLX_TEST_MODE is not a server mode")


if __name__ == "__main__":
    main()
