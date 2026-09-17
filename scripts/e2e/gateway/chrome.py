"""A private Chromium process and profile for the gateway's sole active lease."""
from __future__ import annotations

import http.client
import json
import os
from pathlib import Path
import re
import select
import shutil
import signal
import socket
import subprocess
import sys
import threading
import time

from providers import ProviderFailure
from egress import EgressProxy


class ChromeAdapter:
    EGRESS_PORT = 18888

    def __init__(self, state_dir, peer, binary='/usr/bin/chromium', browser_uid=10002):
        self.root = Path(state_dir) / 'browsers'
        self.root.mkdir(parents=True, exist_ok=True, mode=0o711)
        self.peer, self.binary, self.uid = peer, binary, browser_uid
        self.lock = threading.RLock()
        self.processes = {}
        self.lease_active = lambda _lease: True
        try:
            self.egress_proxy = EgressProxy(self.EGRESS_PORT)
            self._enforce_egress()
        except (OSError, ProviderFailure):
            if hasattr(self, 'egress_proxy'):
                self.egress_proxy.close()
            raise

    def _enforce_egress(self):
        ipv4_rules = [
            ['-m', 'owner', '--uid-owner', str(self.uid), '-m', 'conntrack',
             '--ctstate', 'ESTABLISHED,RELATED', '-j', 'ACCEPT'],
            ['-m', 'owner', '--uid-owner', str(self.uid), '-p', 'tcp', '-d', '127.0.0.1',
             '--dport', str(self.EGRESS_PORT), '-m', 'conntrack', '--ctstate', 'NEW',
             '-j', 'ACCEPT'],
            ['-m', 'owner', '--uid-owner', str(self.uid), '-j', 'REJECT'],
        ]
        ipv6_rules = [
            ['-m', 'owner', '--uid-owner', str(self.uid), '-m', 'conntrack',
             '--ctstate', 'ESTABLISHED,RELATED', '-j', 'ACCEPT'],
            ['-m', 'owner', '--uid-owner', str(self.uid), '-j', 'REJECT'],
        ]
        try:
            for binary, rules in (('iptables', ipv4_rules), ('ip6tables', ipv6_rules)):
                for rule in rules:
                    check = subprocess.run(
                        [binary, '--wait', '-C', 'OUTPUT', *rule], check=False,
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                    )
                    if check.returncode == 0:
                        continue
                    if check.returncode != 1:
                        raise ProviderFailure('browser egress enforcement check failed')
                    subprocess.run(
                        [binary, '--wait', '-A', 'OUTPUT', *rule], check=True,
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                    )
        except (OSError, subprocess.CalledProcessError) as error:
            raise ProviderFailure('browser egress enforcement is unavailable') from error

    def _directory(self, lease):
        identity = lease['lease_id']
        if not re.fullmatch(r'axon_e2e_[a-zA-Z0-9_]{1,220}', identity):
            raise ProviderFailure('invalid browser lease identity')
        return self.root / identity

    @staticmethod
    def _start_identity(pid):
        try:
            # comm can contain spaces/parentheses; fields after its last ')' start at 3.
            return Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[19]
        except FileNotFoundError:
            return None

    def _write(self, directory, state):
        temporary = directory / 'process.json.tmp'
        with temporary.open('w') as stream:
            os.chmod(temporary, 0o600)
            json.dump(state, stream)
            stream.flush()
            os.fsync(stream.fileno())
        temporary.replace(directory / 'process.json')
        descriptor = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)

    def _state(self, lease):
        directory = self._directory(lease)
        try:
            return json.loads((directory / 'process.json').read_text())
        except FileNotFoundError:
            return None

    def _discovery(self, state):
        connection = http.client.HTTPConnection('127.0.0.1', state['port'], timeout=3)
        try:
            connection.request('GET', '/json/version')
            response = connection.getresponse()
            if response.status != 200:
                raise ProviderFailure('browser discovery failed')
            value = json.loads(response.read(1024 * 1024))
        finally:
            connection.close()
        from urllib.parse import urlsplit
        parsed = urlsplit(value.get('webSocketDebuggerUrl', ''))
        if parsed.scheme != 'ws' or parsed.hostname not in ('127.0.0.1', 'localhost') or parsed.port != state['port']:
            raise ProviderFailure('browser returned an unexpected loopback endpoint')
        if not re.fullmatch(r'/devtools/browser/[a-zA-Z0-9-]+', parsed.path):
            raise ProviderFailure('browser returned an invalid endpoint')
        return value, parsed.path

    def _ensure(self, lease):
        with self.lock:
            state = self._state(lease)
            if state:
                if self._start_identity(state['pid']) != state['start']:
                    raise ProviderFailure('browser process ownership changed')
                return state
            directory = self._directory(lease)
            directory.mkdir(mode=0o711, exist_ok=False)
            profile = directory / 'profile'
            profile.mkdir(mode=0o700)
            os.chown(profile, self.uid, self.uid)
            # The owned directory precedes allocation; an interrupted startup is
            # refused and retained for cleanup, never silently reused.
            with socket.socket() as listener:
                listener.bind(('127.0.0.1', 0))
                port = listener.getsockname()[1]
            # Container no-new-privileges prevents Debian's setuid Chromium
            # sandbox. The browser still runs as the dedicated unprivileged UID
            # whose kernel egress policy is installed before this process exists.
            argv = [self.binary, '--headless=new', '--no-sandbox', '--disable-dev-shm-usage',
                    '--no-first-run', '--disable-background-networking', '--disable-extensions',
                    '--disable-quic', '--force-webrtc-ip-handling-policy=disable_non_proxied_udp',
                    f'--proxy-server=http://127.0.0.1:{self.EGRESS_PORT}',
                    '--proxy-bypass-list=<-loopback>',
                    '--remote-debugging-address=127.0.0.1', f'--remote-debugging-port={port}',
                    f'--user-data-dir={profile}', 'about:blank']
            launcher = [
                sys.executable, '-c',
                'import ctypes,os,sys; gate=os.read(0,1); '
                'ok=ctypes.CDLL(None,use_errno=True).prctl(38,1,0,0,0)==0; '
                'sys.exit(0) if gate != b"G" or not ok else os.execv(sys.argv[1],sys.argv[1:])',
                *argv,
            ]
            process = subprocess.Popen(launcher, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                                       stderr=subprocess.DEVNULL, start_new_session=True,
                                       user=self.uid, group=self.uid, extra_groups=[],
                                       env={'PATH': '/usr/bin:/bin', 'HOME': str(profile), 'LANG': 'C.UTF-8'})
            self.processes[lease['lease_id']] = process
            state = {'pid': process.pid, 'start': self._start_identity(process.pid), 'port': port}
            self._write(directory, state)
            process.stdin.write(b'G')
            process.stdin.flush()
            process.stdin.close()
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline and process.poll() is None:
                try:
                    self._discovery(state)
                    return state
                except (OSError, ValueError, ProviderFailure):
                    time.sleep(.1)
            raise ProviderFailure('isolated browser did not become ready')

    def proxy(self, method, incoming, headers, body, lease):
        if method != 'GET' or incoming != '/json/version' or body:
            raise ProviderFailure('browser route is forbidden')
        state = self._ensure(lease)
        value, path = self._discovery(state)
        value['webSocketDebuggerUrl'] = f'wss://{self.peer}{path}'
        return 200, 'application/json', json.dumps(value).encode()

    def websocket(self, handler, incoming, headers, lease):
        state = self._ensure(lease)
        _, path = self._discovery(state)
        if incoming != path or headers.get('Sec-WebSocket-Version') != '13':
            raise ProviderFailure('browser websocket route is forbidden')
        key = headers.get('Sec-WebSocket-Key', '')
        import base64
        try:
            if len(base64.b64decode(key, validate=True)) != 16:
                raise ValueError('key length')
        except ValueError as error:
            raise ProviderFailure('invalid websocket handshake') from error
        upstream = socket.create_connection(('127.0.0.1', state['port']), timeout=5)
        try:
            request = (f'GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{state["port"]}\r\n'
                       f'Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\n'
                       'Sec-WebSocket-Version: 13\r\n\r\n')
            upstream.sendall(request.encode('ascii'))
            response = http.client.HTTPResponse(upstream)
            response.begin()
            if response.status != 101:
                raise ProviderFailure('isolated browser websocket refused')
            handler.send_response(101)
            for name in ('Upgrade', 'Connection', 'Sec-WebSocket-Accept'):
                value = response.getheader(name)
                if value:
                    handler.send_header(name, value)
            handler.end_headers()
            handler.wfile.flush()
            handler.close_connection = True
            # No credentials, provider routes, or client-selected destinations
            # cross this bridge. Each connection terminates with its lease.
            client = handler.connection
            upstream.settimeout(5)
            client.settimeout(5)
            while self.lease_active(lease):
                ready, _, _ = select.select([client, upstream], [], [], 1)
                for source in ready:
                    payload = source.recv(65536)
                    if not payload:
                        return
                    (upstream if source is client else client).sendall(payload)
        finally:
            upstream.close()

    @staticmethod
    def _group_alive(group):
        for entry in Path('/proc').iterdir():
            if not entry.name.isdigit():
                continue
            try:
                fields = (entry / 'stat').read_text().rsplit(')', 1)[1].split()
                if int(fields[2]) == group and fields[0] != 'Z':
                    return True
            except FileNotFoundError:
                continue
        return False

    def cleanup(self, lease):
        with self.lock:
            directory = self._directory(lease)
            if not directory.exists():
                return []
            state = self._state(lease)
            if state is None:
                # The launcher cannot exec Chromium before its receipt is
                # durable. Parent death closes the pipe and exits the launcher.
                shutil.rmtree(directory)
                return []
            observed = self._start_identity(state['pid'])
            if observed is not None and observed != state['start']:
                raise ProviderFailure('browser PID ownership changed')
            # Kill the entire owned session, including surviving renderer
            # children when the original browser leader has already exited.
            process = self.processes.pop(lease['lease_id'], None)
            for sig in (signal.SIGTERM, signal.SIGKILL):
                if not self._group_alive(state['pid']):
                    break
                os.killpg(state['pid'], sig)
                deadline = time.monotonic() + 5
                while self._group_alive(state['pid']) and time.monotonic() < deadline:
                    if process:
                        process.poll()
                    time.sleep(.05)
            if process:
                process.wait(timeout=5)
            if self._group_alive(state['pid']):
                raise ProviderFailure('browser process group remains after teardown')
            shutil.rmtree(directory)
            if directory.exists():
                raise ProviderFailure('browser profile remains after teardown')
            return []
