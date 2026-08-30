#!/usr/bin/env python3
"""Deterministic loopback Google-compatible OIDC provider for Axon E2E."""
from __future__ import annotations

import argparse
import base64
import json
import subprocess
import tempfile
import time
import urllib.parse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

KID = "axon-e2e-google"
MODULUS = int("B30E063EBC995CC86B21AADF8EE7C7FC3817274B77F18EC9E9CDDBF298454C20057148378004C57BB9E2BAD5F65B6DC865B69F4B2E21023B0B6CB2F8ECFB2B7A7BEBD7465669041569971C62CDA4D957101AE1DA5BE38EEEA4B37261DF0D0CE1AA33EB217CAC0367E90B4A42D783227627D6AF2D8B9D60F28ED13D3A8CF0B68E83334D4EF5217AAF556A6C490A3EEF1329993C458E033B24BF0C74876D25580BB931F15E2BDB5C66B3CA00B71BB16754CF99905B4912BD8A449812B712799314ACE702CEA211A1FAE6A9AF9FD2B428672A1AD140FE039C65B2A14B55A720AD6EAB378AEEBD4F85ECD17640ED785BF58C695ECA8AF0C7536833AA433C6F32BB63", 16)
PRIVATE_KEY = """-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCzDgY+vJlcyGsh
qt+O58f8OBcnS3fxjsnpzdvymEVMIAVxSDeABMV7ueK61fZbbchltp9LLiECOwts
svjs+yt6e+vXRlZpBBVplxxizaTZVxAa4dpb447upLNyYd8NDOGqM+shfKwDZ+kL
SkLXgyJ2J9avLYudYPKO0T06jPC2joMzTU71IXqvVWpsSQo+7xMpmTxFjgM7JL8M
dIdtJVgLuTHxXivbXGazygC3G7FnVM+ZkFtJEr2KRJgStxJ5kxSs5wLOohGh+uap
r5/StChnKhrRQP4DnGWyoUtVpyCtbqs3iu69T4Xs0XZA7Xhb9YxpXsqK8MdTaDOq
QzxvMrtjAgMBAAECggEALSJz4IyZ/BFpL+tqvxMeDi31aCpV6cYcj5scvmIz1aSc
upmBo/uP7EhHJuGYYCOkSD9omALgvzczAgt7RAFsTEvAf1tznLUy0JMOzLkZvM99
d8lGybLq7K0HruWM3DVLDSRZOO+8TH989yOZBcpAfZg9PZs1fk5Z1jZYQNIWO231
FI9NBLUUePwp3zb4JLXGQClr0tgvaauMXlDEI6TaDnaTW8ezR9pNjRLB4oAef5sh
0B7IVeEr2rZwbU4rIt7JiAjaecz1ztnfmRtxyKxBoj6DiJ5fxtZHWPEN8OnCC4wH
wSfbfY6ksrv14tr83NahhKyjDIYlfUkNSa1pH8bsgQKBgQDdgCfgdwak0SQUEn8S
EEpLKWb86sEd5lQ3u1guQiQe3gmMwes1KTH8GbzrgpoatSi3xhHv4rpGflUzaTFW
snx8JcKh8yKPBSNwPhGfSiJOmbsjCm34Uh+nA9SvdU3aCUa6Ii1ZgArieLqtB1H3
NYvSEbbdtiSapZI2W1Remc8xqwKBgQDO8W+KsiPd9L7eK6ihl+1h4XTh0IK56ahk
c743YSorXcbsbqB5vXwHjwNOJndoVcni1xRq1OeAKQhi/hej9YwtEDeOENB4I9nX
kUHYzIlC1D/kstXeENngCwcEOrpsXsMaKVS1gwfbl0lbR/l6lRCeRhSm5MVy6toq
JL27CpJVKQKBgCosQmtsfilXYKUpuGP6EgspgOBa2hYVSqep1epI0ZPG9s6EBYKD
q26yf9Pfc/Pt1ijXX4brBkhxuUsmlixJo4YHsn0fS88rTUoLp1NKzClm/8h2LeX/
zOMBybb2gLIo3fyGkVffFzNzhSd4o1SML2j50nV4PpPrPmF3FiNE6bwtAoGBALNG
l7HY55eWOm/v9JOhMWXVUlN6NnXmxRnY51XEmCqfgAA9SkqM69EEhQGD83fwsggQ
+cAfFzqA0aIoq8Q/qaM0ZFxvlpotvL+yOBAgCV1a0MtIXlyVzpn4E6kHU48kfPLC
EX95tyn2IvewH6GhV5c18RgwIhmO+Vb/I1rRKroZAoGAdLzCfkbvUdh0uPykgA7w
dWZSOZ7O1ysYAENfiHMIqlcmLCa2V6J6vewFOAW8SgxU0U+3rBMRR98SpE4SlvpY
jwx8/iywYqu/7gU2YYso2FiZDLOhvXQeCTy2DlScefBc0BdZed4tYaOfdj6Rfvyd
LPNhVG/HRJ4+2ryS5GAl9Ok=
-----END PRIVATE KEY-----
"""


def b64(value: bytes) -> str:
    return base64.urlsafe_b64encode(value).decode().rstrip("=")


def integer_b64(value: int) -> str:
    return b64(value.to_bytes((value.bit_length() + 7) // 8, "big"))


def sign(key_path: Path, claims: dict) -> str:
    header = b64(json.dumps({"alg": "RS256", "kid": KID, "typ": "JWT"}, separators=(",", ":")).encode())
    payload = b64(json.dumps(claims, separators=(",", ":")).encode())
    signing_input = f"{header}.{payload}".encode()
    signature = subprocess.run(
        ["openssl", "dgst", "-sha256", "-sign", str(key_path)],
        input=signing_input,
        check=True,
        capture_output=True,
    ).stdout
    return f"{header}.{payload}.{b64(signature)}"


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def json(self, status: int, value: dict):
        data = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        parsed = urllib.parse.urlsplit(self.path)
        if parsed.path == "/jwks":
            return self.json(200, {"keys": [{"kid": KID, "alg": "RS256", "kty": "RSA", "use": "sig", "n": integer_b64(MODULUS), "e": "AQAB"}]})
        if parsed.path != "/authorize":
            return self.json(404, {"error": "not_found"})
        query = urllib.parse.parse_qs(parsed.query)
        redirect = query.get("redirect_uri", [""])[0]
        state = query.get("state", [""])[0]
        if not redirect.startswith("http://127.0.0.1:") or not state:
            return self.json(400, {"error": "invalid_request"})
        code = f"oidc-{len(self.server.codes) + 1}"
        self.server.codes[code] = query.get("client_id", [""])[0]
        self.send_response(302)
        self.send_header("location", redirect + "?" + urllib.parse.urlencode({"code": code, "state": state}))
        self.end_headers()

    def do_POST(self):
        if self.path != "/token":
            return self.json(404, {"error": "not_found"})
        form = urllib.parse.parse_qs(self.rfile.read(int(self.headers.get("content-length", "0"))).decode())
        code = form.get("code", [""])[0]
        audience = self.server.codes.pop(code, None)
        if not audience or form.get("client_secret", [""])[0] != "e2e-secret":
            return self.json(400, {"error": "invalid_grant"})
        now = int(time.time())
        token = sign(self.server.key_path, {"iss": self.server.issuer, "aud": audience, "sub": "axon-e2e-user", "email": "e2e@example.invalid", "email_verified": True, "iat": now, "exp": now + 300})
        self.json(200, {"access_token": "upstream-access", "refresh_token": "upstream-refresh", "expires_in": 300, "id_token": token})


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, required=True)
    args = parser.parse_args()
    temp = tempfile.TemporaryDirectory(prefix="axon-e2e-oidc-")
    key_path = Path(temp.name) / "key.pem"
    key_path.write_text(PRIVATE_KEY)
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    server.issuer = f"http://127.0.0.1:{args.port}"
    server.key_path = key_path
    server.codes = {}
    server.serve_forever()


if __name__ == "__main__":
    main()
