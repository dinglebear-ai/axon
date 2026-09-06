#!/usr/bin/env python3
"""Loopback-only deterministic fixture server and provider doubles."""

from __future__ import annotations

import argparse
import hashlib
import json
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse


PAGE = b"<!doctype html><title>Axon E2E</title><main>canonical fixture alpha beta</main>"
FEED = b'<?xml version="1.0"?><rss version="2.0"><channel><title>Axon E2E</title><item><guid>fixture-1</guid><title>Alpha</title></item></channel></rss>'


def embedding(text: str, dimensions: int = 8) -> list[float]:
    digest = hashlib.sha256(text.encode("utf-8")).digest()
    return [round((digest[index] - 127.5) / 127.5, 6) for index in range(dimensions)]


class Handler(BaseHTTPRequestHandler):
    server_version = "AxonE2EFixture/1"

    def log_message(self, _format: str, *_args: object) -> None:
        return

    def _json(self, status: int, payload: object) -> None:
        body = json.dumps(payload, sort_keys=True).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _mode(self) -> str:
        return parse_qs(urlparse(self.path).query).get("mode", ["success"])[0]

    def _failure(self) -> bool:
        mode = self._mode()
        if mode == "timeout":
            time.sleep(float(self.server.fixture_timeout))
        if mode == "malformed":
            body = b"{not-json"
            self.send_response(200); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)
            return True
        if mode == "unavailable":
            self._json(503, {"error": "fixture provider unavailable"}); return True
        if mode == "transient" and self.server.bump(self.path) == 1:
            self._json(429, {"error": "retry", "retry_after_ms": 1}); return True
        return False

    def do_GET(self) -> None:
        path = urlparse(self.path).path
        if path == "/health":
            self._json(200, {"fixture_contract": 1, "status": "ok"})
        elif path == "/page":
            self.send_response(200); self.send_header("Content-Type", "text/html"); self.end_headers(); self.wfile.write(PAGE)
        elif path == "/feed.xml":
            self.send_response(200); self.send_header("Content-Type", "application/rss+xml"); self.end_headers(); self.wfile.write(FEED)
        elif path == "/ssrf-sentinel":
            if self.headers.get("X-Axon-E2E-SSRF-Token") != self.server.ssrf_token:
                self._json(403, {"error": "owned sentinel token required"})
            else:
                self._json(200, {"sentinel": "owned", "reached": True})
        else:
            self._json(404, {"error": "unknown deterministic fixture"})

    def do_POST(self) -> None:
        if self._failure():
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
            request = json.loads(self.rfile.read(length) or b"{}")
        except (ValueError, json.JSONDecodeError):
            self._json(400, {"error": "invalid request"}); return
        path, mode = urlparse(self.path).path, self._mode()
        if path == "/provider/tei/embed":
            inputs = request.get("inputs", [])
            if isinstance(inputs, str): inputs = [inputs]
            dimensions = 7 if mode == "wrong-dimension" else 8
            vectors = [embedding(str(item), dimensions) for item in inputs]
            if mode == "partial" and vectors: vectors.pop()
            self._json(200, vectors)
        elif path == "/provider/llm/chat/completions":
            messages = request.get("messages", [])
            content = " | ".join(str(item.get("content", "")) for item in messages)
            self._json(200, {"id": "fixture-completion", "choices": [{"index": 0, "message": {"role": "assistant", "content": f"fixture:{content}"}, "finish_reason": "stop"}]})
        else:
            self._json(404, {"error": "unknown provider boundary"})


class FixtureServer(ThreadingHTTPServer):
    def __init__(self, address: tuple[str, int], token: str, timeout: float):
        if address[0] not in {"127.0.0.1", "::1", "localhost"}:
            raise ValueError("fixture server must bind loopback")
        super().__init__(address, Handler)
        self.ssrf_token, self.fixture_timeout, self._counts = token, timeout, {}

    def bump(self, key: str) -> int:
        self._counts[key] = self._counts.get(key, 0) + 1
        return self._counts[key]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", required=True, type=int)
    parser.add_argument("--ssrf-token", required=True)
    parser.add_argument("--fixture-timeout", type=float, default=2.0)
    args = parser.parse_args()
    FixtureServer((args.host, args.port), args.ssrf_token, args.fixture_timeout).serve_forever()


if __name__ == "__main__": main()
