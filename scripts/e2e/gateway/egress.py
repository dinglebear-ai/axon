"""Loopback HTTP proxy that limits leased browsers to public web destinations."""
from __future__ import annotations

import argparse
import ipaddress
import select
import socket
import socketserver
import threading
import urllib.parse


class EgressDenied(Exception):
    """The requested destination or proxy request is outside the web policy."""


_TOKEN = frozenset(
    b"!#$%&'*+-.^_`|~0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
)
_HOP_BY_HOP = {
    'connection', 'keep-alive', 'proxy-authenticate', 'proxy-authorization',
    'proxy-connection', 'te', 'trailer', 'transfer-encoding', 'upgrade',
}
_MAX_HEADER_BYTES = 64 * 1024
_MAX_BODY_BYTES = 16 * 1024 * 1024


def _public_address(value):
    try:
        address = ipaddress.ip_address(value)
    except ValueError as error:
        raise EgressDenied('resolver returned an invalid address') from error
    if not address.is_global:
        raise EgressDenied('destination is not globally routable')
    return address


def resolve_public(host, port, resolver=socket.getaddrinfo):
    """Resolve once, reject mixed answers, and return pinned numeric addresses."""
    if not host or len(host) > 253 or '\x00' in host:
        raise EgressDenied('destination host is invalid')
    try:
        ascii_host = host.encode('idna').decode('ascii')
    except UnicodeError as error:
        raise EgressDenied('destination host is invalid') from error
    try:
        records = resolver(ascii_host, port, type=socket.SOCK_STREAM, proto=socket.IPPROTO_TCP)
    except socket.gaierror as error:
        raise EgressDenied('destination did not resolve') from error
    pinned = []
    for family, socktype, protocol, _canonical, sockaddr in records:
        if family not in (socket.AF_INET, socket.AF_INET6):
            continue
        _public_address(sockaddr[0])
        candidate = (family, socktype, protocol, sockaddr)
        if candidate not in pinned:
            pinned.append(candidate)
    if not pinned:
        raise EgressDenied('destination has no public address')
    return pinned


def connect_public(host, port, *, timeout=10, resolver=socket.getaddrinfo,
                   socket_factory=socket.socket):
    """Connect to a numeric address from the single validated DNS answer set."""
    records = resolve_public(host, port, resolver)
    last_error = None
    for family, socktype, protocol, sockaddr in records:
        connection = socket_factory(family, socktype, protocol)
        try:
            connection.settimeout(timeout)
            connection.connect(sockaddr)
            return connection
        except OSError as error:
            last_error = error
            connection.close()
    raise EgressDenied('public destination is unreachable') from last_error


def _authority(value, default_port):
    try:
        parsed = urllib.parse.urlsplit('//' + value)
        port = parsed.port or default_port
    except ValueError as error:
        raise EgressDenied('destination authority is invalid') from error
    if parsed.username is not None or parsed.password is not None or not parsed.hostname:
        raise EgressDenied('destination authority is invalid')
    if parsed.path or parsed.query or parsed.fragment or port not in (80, 443):
        raise EgressDenied('destination authority is forbidden')
    return parsed.hostname, port


def _read_headers(stream):
    headers = []
    consumed = 0
    while True:
        line = stream.readline(_MAX_HEADER_BYTES + 1)
        consumed += len(line)
        if not line or consumed > _MAX_HEADER_BYTES:
            raise EgressDenied('proxy headers are invalid')
        if line in (b'\r\n', b'\n'):
            return headers
        if line[:1] in (b' ', b'\t') or b':' not in line:
            raise EgressDenied('proxy headers are invalid')
        raw_name, raw_value = line.rstrip(b'\r\n').split(b':', 1)
        if not raw_name or any(byte not in _TOKEN for byte in raw_name):
            raise EgressDenied('proxy header name is invalid')
        if any(byte < 32 and byte != 9 or byte == 127 for byte in raw_value):
            raise EgressDenied('proxy header value is invalid')
        headers.append((raw_name.decode('ascii'), raw_value.strip().decode('iso-8859-1')))


def _relay(left, right):
    while True:
        readable, _, _ = select.select([left, right], [], [], 30)
        if not readable:
            return
        for source in readable:
            try:
                payload = source.recv(65536)
            except ConnectionResetError:
                payload = b''
            if not payload:
                return
            target = right if source is left else left
            target.sendall(payload)


class _ProxyHandler(socketserver.StreamRequestHandler):
    timeout = 30

    def handle(self):
        try:
            line = self.rfile.readline(8193)
            if not line or len(line) > 8192:
                raise EgressDenied('proxy request line is invalid')
            parts = line.rstrip(b'\r\n').split(b' ')
            if len(parts) != 3 or parts[2] != b'HTTP/1.1':
                raise EgressDenied('proxy request line is invalid')
            if any(byte < 33 or byte > 126 for byte in parts[1]):
                raise EgressDenied('proxy target is invalid')
            method = parts[0].decode('ascii')
            target = parts[1].decode('ascii')
            if not method or any(byte not in _TOKEN for byte in parts[0]):
                raise EgressDenied('proxy method is invalid')
            headers = _read_headers(self.rfile)
            if method == 'CONNECT':
                self._connect(target, headers)
            else:
                self._http(method, target, headers)
        except (EgressDenied, OSError, UnicodeError, ValueError):
            try:
                self.wfile.write(b'HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n')
            except OSError:
                pass

    def _connect(self, target, headers):
        host, port = _authority(target, 443)
        if any(name.lower() in ('content-length', 'transfer-encoding') for name, _ in headers):
            raise EgressDenied('CONNECT body is forbidden')
        upstream = connect_public(host, port)
        try:
            self.wfile.write(b'HTTP/1.1 200 Connection Established\r\n\r\n')
            self.wfile.flush()
            _relay(self.connection, upstream)
        finally:
            upstream.close()

    def _http(self, method, target, headers):
        if method not in ('GET', 'HEAD', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS'):
            raise EgressDenied('HTTP method is forbidden')
        try:
            parsed = urllib.parse.urlsplit(target)
            port = parsed.port or 80
        except ValueError as error:
            raise EgressDenied('proxy target is invalid') from error
        if (parsed.scheme != 'http' or parsed.username is not None or parsed.password is not None
                or not parsed.hostname or parsed.fragment or port != 80):
            raise EgressDenied('proxy target is forbidden')
        host_headers = [value for name, value in headers if name.lower() == 'host']
        if len(host_headers) != 1:
            raise EgressDenied('exactly one Host header is required')
        expected_host, expected_port = _authority(host_headers[0], 80)
        if expected_host.casefold() != parsed.hostname.casefold() or expected_port != port:
            raise EgressDenied('Host header does not match destination')
        if any(name.lower() == 'transfer-encoding' for name, _ in headers):
            raise EgressDenied('transfer encoding is forbidden')
        lengths = [value for name, value in headers if name.lower() == 'content-length']
        if len(lengths) > 1:
            raise EgressDenied('ambiguous content length')
        if lengths and (not lengths[0] or not lengths[0].isascii() or not lengths[0].isdigit()):
            raise EgressDenied('content length is invalid')
        length = int(lengths[0]) if lengths else 0
        if length < 0 or length > _MAX_BODY_BYTES:
            raise EgressDenied('request body is too large')
        upstream = connect_public(parsed.hostname, port)
        try:
            path = urllib.parse.urlunsplit(('', '', parsed.path or '/', parsed.query, ''))
            upstream.sendall(f'{method} {path} HTTP/1.1\r\n'.encode('ascii'))
            for name, value in headers:
                if name.lower() not in _HOP_BY_HOP:
                    upstream.sendall(f'{name}: {value}\r\n'.encode('iso-8859-1'))
            upstream.sendall(b'Connection: close\r\n\r\n')
            remaining = length
            while remaining:
                payload = self.rfile.read(min(remaining, 65536))
                if not payload:
                    raise EgressDenied('request body was truncated')
                upstream.sendall(payload)
                remaining -= len(payload)
            while True:
                payload = upstream.recv(65536)
                if not payload:
                    return
                self.wfile.write(payload)
        finally:
            upstream.close()


class _ProxyServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True

    def handle_error(self, request, client_address):
        # Destination details and browser traffic never belong in gateway logs.
        pass


class EgressProxy:
    """Own one loopback public-web proxy and its serving thread."""

    def __init__(self, port=0, bind='127.0.0.1'):
        self.server = _ProxyServer((bind, port), _ProxyHandler)
        self.port = self.server.server_address[1]
        self.thread = threading.Thread(
            target=self.server.serve_forever, name=f'chrome-egress-{self.port}', daemon=True,
        )
        self.thread.start()

    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--bind', default='0.0.0.0')
    parser.add_argument('--port', type=int, default=8080)
    args = parser.parse_args()
    server = _ProxyServer((args.bind, args.port), _ProxyHandler)
    try:
        server.serve_forever()
    finally:
        server.server_close()


if __name__ == '__main__':
    main()
