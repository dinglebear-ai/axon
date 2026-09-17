#!/usr/bin/env python3
from __future__ import annotations

import argparse
import collections
import contextlib
import hmac
import hashlib
import json
import os
import threading
import time
import urllib.parse
import ipaddress
import re
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

from providers import ProviderAdapter, ProviderFailure
from chrome import ChromeAdapter
from state import LeaseStore, StateError, format_time


class Limits:
    def __init__(self):
        self.lock = threading.Lock(); self.serial = collections.defaultdict(threading.Lock); self.requests = collections.defaultdict(collections.deque)

    @contextlib.contextmanager
    def admit(self, lease):
        lease_id = lease["lease_id"]; now = time.monotonic()
        with self.serial[lease_id]:
            with self.lock:
                history = self.requests[lease_id]
                while history and history[0] <= now - 1: history.popleft()
                delay = max(0, history[-1] + 1 / lease["qps"] - now) if history else 0
            if delay: time.sleep(min(delay, 1))
            with self.lock: self.requests[lease_id].append(time.monotonic())
            yield


class Gateway(ThreadingHTTPServer):
    daemon_threads = True
    def __init__(self, address, *, service, peer, tag, token, store, provider):
        super().__init__(address, Handler)
        self.service, self.peer, self.tag, self.token = service, peer, tag, token
        self.store, self.provider, self.limits = store, provider, Limits()
        self.cleanup_lock = threading.Lock(); self.resource_lock = threading.RLock(); self.stop_janitor = threading.Event()
        self.janitor = threading.Thread(target=self._reap_loop, name="gateway-lease-janitor", daemon=True)
        self.janitor.start()

    def _reap_loop(self):
        while not self.stop_janitor.wait(5):
            for lease in self.store.expired():
                try: self.cleanup(lease)
                except (ProviderFailure, StateError, OSError): pass

    def server_close(self):
        self.stop_janitor.set(); self.janitor.join(timeout=2); super().server_close()

    def cleanup(self, lease):
        with self.cleanup_lock:
            self.store.begin_delete(lease["lease_id"])
            with self.resource_lock: residuals = self.provider.cleanup(lease)
            if residuals: raise ProviderFailure("provider residual audit is not empty")
            self.store.retire(lease["lease_id"])
            return []


class Handler(BaseHTTPRequestHandler):
    server: Gateway
    protocol_version = "HTTP/1.1"
    MAX_BODY = 1024 * 1024

    def log_message(self, fmt, *args):
        # Never let authorization values or request bodies reach logs.
        super().log_message("%s %s", self.command, urllib.parse.urlsplit(self.path).path)

    def _json(self, status, body):
        data = json.dumps(body, separators=(",", ":")).encode()
        self.send_response(status); self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data))); self.end_headers(); self.wfile.write(data)

    def _error(self, status, message): self._json(status, {"error": message})

    def _authorized(self):
        supplied = self.headers.get("Authorization", "")
        expected = "Bearer " + self.server.token
        return hmac.compare_digest(supplied.encode(), expected.encode())

    def _data_token(self, lease):
        message = (self.server.service + "\0" + lease["lease_id"]).encode()
        return "axe1_" + hmac.new(self.server.token.encode(), message, hashlib.sha256).hexdigest()

    def _data_authorized(self, lease):
        expected = self._data_token(lease).encode()
        if self.server.service == "qdrant": supplied = self.headers.get("api-key", "").encode()
        else:
            value = self.headers.get("Authorization", "")
            supplied = value.removeprefix("Bearer ").encode() if value.startswith("Bearer ") else b""
        return hmac.compare_digest(supplied, expected)

    def _presented_data_token(self):
        if self.server.service == "qdrant": return self.headers.get("api-key", "")
        value = self.headers.get("Authorization", "")
        return value.removeprefix("Bearer ") if value.startswith("Bearer ") else ""

    def _valid_framing(self):
        return not self.headers.get("Transfer-Encoding") and len(self.headers.get_all("Content-Length", [])) <= 1

    def _body(self):
        try: length = int(self.headers.get("Content-Length", "0"))
        except ValueError: raise StateError("invalid content length")
        if length < 0 or length > self.MAX_BODY:
            self.close_connection = True
            raise StateError("request body exceeds limit")
        raw = self.rfile.read(length)
        try: value = json.loads(raw) if raw else {}
        except json.JSONDecodeError as error: raise StateError("request JSON is invalid") from error
        if not isinstance(value, dict): raise StateError("request body must be an object")
        return value

    def _lease_id(self, suffix=""):
        path = urllib.parse.urlsplit(self.path).path
        prefix = "/v1/e2e/leases/"
        if not path.startswith(prefix) or not path.endswith(suffix): return None
        value = path[len(prefix):len(path)-len(suffix) if suffix else None]
        if not value or "/" in value or urllib.parse.quote(urllib.parse.unquote(value), safe="") != value:
            return None
        return urllib.parse.unquote(value)

    def _active(self):
        active = self.server.store.active()
        if len(active) != 1: raise StateError("exactly one active lease is required")
        return active[0]

    def _management(self): return urllib.parse.urlsplit(self.path).path.startswith("/v1/e2e/")

    def _dispatch(self):
        path = urllib.parse.urlsplit(self.path).path
        management = path.startswith("/v1/e2e/")
        if management and not self._authorized():
            self.close_connection = True
            return self._error(401, "unauthorized")
        if not self._valid_framing():
            self.close_connection = True
            return self._error(400, "ambiguous request framing")
        if self.command == "GET" and path == "/v1/e2e/identity":
            return self._json(200, {"schema":1,"service":self.server.service,"peer":self.server.peer,
              "tag":self.server.tag,"enforcement":"disposable-tenant-proxy","lease_api":True,
              "application_auth":"bearer-required","version":"1"})
        if self.command == "POST" and path == "/v1/e2e/leases/reap":
            body = self._body()
            if body != {"owner":"dinglebear-ai/axon","expired_only":True,"residual_audit":True}:
                raise StateError("reap request is invalid")
            for lease in self.server.store.expired(): self.server.cleanup(lease)
            return self._json(200, {"status":"passed","residuals":[]})
        if self.command == "POST" and path == "/v1/e2e/leases":
            lease = self.server.store.create(self._body())
            try:
                if hasattr(self.server.provider, "start"): self.server.provider.start(lease)
            except Exception:
                # Reconcile partial startup immediately. If cleanup is
                # uncertain, cleanup() leaves durable deleting authority.
                self.server.cleanup(lease)
                raise
            return self._json(201, {**self.server.store.public(lease), "data_token":self._data_token(lease)})
        lease_id = self._lease_id("/heartbeat")
        if self.command == "PATCH" and lease_id:
            row = self.server.store.heartbeat(lease_id, self._body())
            return self._json(200, {"status":"renewed","heartbeat_at":row["heartbeat_at"],
              "expires_at":row["expires_at"],"namespace":row["namespace"],"owner":row["owner"],
              "run_id":row["run_id"],"run_attempt":row["run_attempt"]})
        lease_id = self._lease_id()
        if lease_id and self.command == "GET":
            # Expired authority remains visible until its provider state has
            # been deleted and audited. Outer cleanup relies on this marker.
            row = self.server.store.get(lease_id, include_expired=True)
            return self._json(200, self.server.store.public(row)) if row else self._error(404, "lease not found")
        if lease_id and self.command == "DELETE":
            row = self.server.store.get(lease_id, include_expired=True); body = self._body()
            if row is None: return self._error(404, "lease not found")
            expected = {"namespace":row["namespace"],"owner":row["owner"],"residual_audit":True}
            if body != expected: raise StateError("lease deletion ownership mismatch")
            residuals = self.server.cleanup(row)
            return self._json(200, {"status":"deleted","residuals":residuals})
        if self._management(): return self._error(404, "route not found")
        presented = self._presented_data_token()
        if len(presented) != 69 or not presented.startswith("axe1_"):
            self.close_connection = True
            return self._error(401, "unauthorized")
        lease = self._active()
        if not self._data_authorized(lease):
            self.close_connection = True
            return self._error(401, "unauthorized")
        if self.headers.get("Upgrade", "").lower() == "websocket":
            if not hasattr(self.server.provider, "websocket"): raise ProviderFailure("websocket route is forbidden")
            with self.server.resource_lock, self.server.limits.admit(lease):
                if not self.server.store.is_active(lease["lease_id"]): raise StateError("lease was revoked")
                self.server.provider.websocket(self, self.path, self.headers, lease)
            self.close_connection = True
            return
        raw_length = int(self.headers.get("Content-Length", "0"))
        if raw_length < 0 or raw_length > self.MAX_BODY:
            self.close_connection = True
            raise StateError("request body exceeds limit")
        raw = self.rfile.read(raw_length)
        with self.server.resource_lock, self.server.limits.admit(lease):
            if not self.server.store.is_active(lease["lease_id"]): raise StateError("lease was revoked")
            status, content_type, data = self.server.provider.proxy(self.command, self.path, self.headers, raw, lease)
        self.send_response(status); self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data))); self.end_headers(); self.wfile.write(data)

    def _safe_dispatch(self):
        try: self._dispatch()
        except StateError as error: self._error(409, str(error))
        except ProviderFailure: self._error(502, "provider operation failed")
        except (OSError, ValueError): self._error(502, "gateway operation failed")

    do_GET = do_POST = do_PATCH = do_PUT = do_DELETE = _safe_dispatch


def main():
    parser = argparse.ArgumentParser(); parser.add_argument("--bind", default="127.0.0.1"); parser.add_argument("--port", type=int, default=8443)
    args = parser.parse_args()
    names = ("AXON_E2E_GATEWAY_SERVICE", "AXON_E2E_GATEWAY_PEER", "AXON_E2E_GATEWAY_TAG",
             "AXON_E2E_GATEWAY_TOKEN", "AXON_E2E_GATEWAY_STATE")
    missing = [name for name in names if not os.environ.get(name)]
    if missing: raise SystemExit("missing gateway configuration: " + ", ".join(missing))
    service = os.environ["AXON_E2E_GATEWAY_SERVICE"]
    validate_startup(service, os.environ["AXON_E2E_GATEWAY_PEER"], os.environ["AXON_E2E_GATEWAY_TAG"],
                     os.environ["AXON_E2E_GATEWAY_TOKEN"], args.bind)
    store = LeaseStore(Path(os.environ["AXON_E2E_GATEWAY_STATE"]), service)
    if service == "chrome":
        provider = ChromeAdapter(store.path.parent, os.environ["AXON_E2E_GATEWAY_PEER"])
        provider.lease_active = lambda lease: store.is_active(lease["lease_id"])
    else:
        provider = ProviderAdapter(service, os.environ["AXON_E2E_GATEWAY_UPSTREAM"])
    Gateway((args.bind, args.port), service=service, peer=os.environ["AXON_E2E_GATEWAY_PEER"],
            tag=os.environ["AXON_E2E_GATEWAY_TAG"], token=os.environ["AXON_E2E_GATEWAY_TOKEN"],
            store=store, provider=provider).serve_forever()


def validate_startup(service, peer, tag, token, bind):
    tags={name:f"tag:axon-e2e-{name}-gateway" for name in ("qdrant","tei","chrome","llm")}
    if service not in tags or tag != tags[service]: raise ValueError("gateway service/tag identity mismatch")
    if re.fullmatch(r"[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.ts\.net",peer) is None:
        raise ValueError("gateway peer must be an exact MagicDNS name")
    if len(token) < 32: raise ValueError("gateway root token is too short")
    try: loopback=ipaddress.ip_address(bind).is_loopback
    except ValueError: loopback=bind == "localhost"
    if not loopback: raise ValueError("gateway sidecar must bind loopback")


if __name__ == "__main__": main()
