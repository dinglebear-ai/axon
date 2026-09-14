from __future__ import annotations

import json
import os
import urllib.error
import urllib.parse
import urllib.request


class ProviderFailure(RuntimeError):
    pass


class ProviderAdapter:
    MAX_RESPONSE = 16 * 1024 * 1024
    def __init__(self, kind: str, upstream: str):
        parsed = urllib.parse.urlsplit(upstream)
        if (parsed.scheme not in {"http", "https"} or not parsed.hostname
                or parsed.username or parsed.password or parsed.query or parsed.fragment):
            raise ProviderFailure("upstream URL is invalid")
        self.kind, self.upstream = kind, upstream.rstrip("/")

    def path(self, incoming: str, lease: dict) -> str:
        parsed = urllib.parse.urlsplit(incoming)
        decoded = urllib.parse.unquote(parsed.path)
        if ".." in decoded.split("/") or "\\" in decoded or "%2f" in parsed.path.lower():
            raise ProviderFailure("encoded or traversal path is forbidden")
        path = parsed.path
        if self.kind == "qdrant":
            if path == "/healthz": return path
            parts = path.strip("/").split("/")
            if path == "/collections":
                return path
            if len(parts) < 2 or parts[0] != "collections" or len(parts) > 5:
                raise ProviderFailure("Qdrant route is not tenant scoped")
            if any(value in parts for value in ("aliases", "snapshots", "cluster")):
                raise ProviderFailure("unsafe Qdrant route is forbidden")
            if len(parts) > 2 and parts[2] not in {"points", "index", "facet"}:
                raise ProviderFailure("unknown Qdrant collection route is forbidden")
            parts[1] = lease["namespace"]
            path = "/" + "/".join(parts)
        elif self.kind == "tei":
            if path not in ("/health", "/info", "/embed"):
                raise ProviderFailure("TEI route is forbidden")
        elif self.kind == "llm":
            if path not in ("/v1/models", "/v1/chat/completions"):
                raise ProviderFailure("LLM route is forbidden")
        else:
            raise ProviderFailure("provider requires a safe adapter")
        return urllib.parse.urlunsplit(("", "", path, parsed.query, ""))

    def _validate_body(self, body, namespace):
        if self.kind != "qdrant" or not body: return
        try: value = json.loads(body)
        except json.JSONDecodeError as error: raise ProviderFailure("Qdrant JSON is invalid") from error
        dangerous = {"collection", "collection_name", "from_collection", "lookup_from", "init_from"}
        def walk(item):
            if isinstance(item, dict):
                for key, child in item.items():
                    if key in dangerous and child not in (None, namespace):
                        raise ProviderFailure("cross-collection Qdrant reference is forbidden")
                    walk(child)
            elif isinstance(item, list):
                for child in item: walk(child)
        walk(value)

    def _opener(self):
        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, req, fp, code, msg, headers, newurl): return None
        return urllib.request.build_opener(NoRedirect)

    def _upstream_headers(self, content_type="application/json"):
        headers = {"Content-Type": content_type}
        secret = os.environ.get("AXON_E2E_GATEWAY_UPSTREAM_TOKEN", "")
        if secret:
            if self.kind == "qdrant": headers["api-key"] = secret
            else: headers["Authorization"] = "Bearer " + secret
        return headers

    def proxy(self, method, incoming, headers, body, lease):
        suffix = self.path(incoming, lease)
        if self.kind == "qdrant" and suffix == "/collections":
            if method != "GET": raise ProviderFailure("Qdrant collection enumeration mutation is forbidden")
            target = self.upstream + "/collections/" + urllib.parse.quote(lease["namespace"], safe="")
            try:
                with self._opener().open(urllib.request.Request(target,headers=self._upstream_headers()),timeout=10): present=True
            except urllib.error.HTTPError as error:
                try:
                    if error.code != 404: raise ProviderFailure("owned Qdrant collection lookup failed") from error
                    present=False
                finally:error.close()
            collections=[{"name":lease["namespace"]}] if present else []
            data = json.dumps({"result":{"collections":collections},"status":"ok","time":0}).encode()
            return 200, "application/json", data
        self._validate_body(body, lease["namespace"])
        if self.kind == "qdrant" and suffix.split("?", 1)[0].endswith("/facet") and method != "POST":
            raise ProviderFailure("Qdrant facet requires POST")
        allowed_methods = {"qdrant":{"GET","PUT","POST","PATCH","DELETE"}, "tei":{"GET","POST"}, "llm":{"GET","POST"}}
        if method not in allowed_methods[self.kind]: raise ProviderFailure("provider method is forbidden")
        upstream_headers = self._upstream_headers(headers.get("Content-Type", "application/json"))
        request = urllib.request.Request(self.upstream + suffix, data=body or None, method=method,
                                         headers=upstream_headers)
        try:
            with self._opener().open(request, timeout=30) as response:
                data = response.read(self.MAX_RESPONSE + 1)
                if len(data) > self.MAX_RESPONSE: raise ProviderFailure("upstream response exceeds limit")
                return response.status, response.headers.get("Content-Type", "application/json"), data
        except urllib.error.HTTPError as error:
            try:
                data = error.read(self.MAX_RESPONSE + 1)
                if len(data) > self.MAX_RESPONSE: raise ProviderFailure("upstream error response exceeds limit")
                return error.code, error.headers.get("Content-Type", "application/json"), data
            finally:
                error.close()

    def cleanup(self, lease):
        if self.kind != "qdrant":
            return []
        target = self.upstream + "/collections/" + urllib.parse.quote(lease["namespace"], safe="")
        try:
            request = urllib.request.Request(target, method="DELETE", headers=self._upstream_headers())
            with self._opener().open(request, timeout=20):
                pass
        except urllib.error.HTTPError as error:
            try:
                if error.code != 404:
                    raise ProviderFailure("Qdrant tenant deletion failed") from error
            finally:
                error.close()
        try:
            with self._opener().open(urllib.request.Request(target, headers=self._upstream_headers()), timeout=10):
                raise ProviderFailure("Qdrant tenant remains after deletion")
        except urllib.error.HTTPError as error:
            try:
                if error.code != 404:
                    raise ProviderFailure("Qdrant residual audit failed") from error
            finally:
                error.close()
        return []
