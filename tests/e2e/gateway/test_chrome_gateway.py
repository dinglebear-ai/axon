"""Browser isolation policy; real Chromium lifecycle runs in the gateway image."""
import importlib.util
import json
from pathlib import Path
import socket
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'scripts/e2e/gateway'))
from chrome import ChromeAdapter
from egress import EgressDenied, connect_public, resolve_public
from providers import ProviderFailure


class ChromePolicyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        proxy = patch('chrome.EgressProxy')
        enforcement = patch.object(ChromeAdapter, '_enforce_egress')
        self.real_enforce_egress = ChromeAdapter._enforce_egress
        self.proxy_type = proxy.start()
        enforcement.start()
        self.addCleanup(enforcement.stop)
        self.addCleanup(proxy.stop)
        self.adapter = ChromeAdapter(self.temp.name, 'test.example.ts.net')
        self.lease = {'lease_id': 'axon_e2e_123_1_abc_chrome_123'}

    def test_path_escape_is_rejected(self):
        for value in ('../other', 'axon_e2e_../other', 'axon_e2e_a/b', 'axon_e2e_a%2fb'):
            with self.subTest(value=value), self.assertRaises(ProviderFailure):
                self.adapter._directory({'lease_id': value})

    def test_only_browser_discovery_http_is_available(self):
        for method, path in [('POST', '/json/new'), ('GET', '/json/list'), ('GET', '/json/version?url=x')]:
            with self.subTest(path=path), self.assertRaises(ProviderFailure):
                self.adapter.proxy(method, path, {}, b'', self.lease)

    def test_discovery_rewrites_only_to_configured_secure_gateway(self):
        value = {'Browser': 'Chrome/test', 'webSocketDebuggerUrl': 'ws://127.0.0.1:9000/devtools/browser/a'}
        with patch.object(self.adapter, '_ensure', return_value={}), patch.object(
                self.adapter, '_discovery', return_value=(value, '/devtools/browser/a')):
            status, content_type, body = self.adapter.proxy('GET', '/json/version', {}, b'', self.lease)
        self.assertEqual(status, 200)
        self.assertEqual(json.loads(body)['webSocketDebuggerUrl'], 'wss://test.example.ts.net/devtools/browser/a')

    def test_pid_reuse_refuses_cleanup_and_preserves_evidence(self):
        directory = self.adapter._directory(self.lease)
        directory.mkdir()
        self.adapter._write(directory, {'pid': 123, 'start': '100', 'port': 9000})
        with patch.object(self.adapter, '_start_identity', return_value='200'), self.assertRaises(ProviderFailure):
            self.adapter.cleanup(self.lease)
        self.assertTrue((directory / 'process.json').exists())

    def test_absent_browser_cleanup_is_idempotent(self):
        self.assertEqual(self.adapter.cleanup(self.lease), [])
        self.assertEqual(self.adapter.cleanup(self.lease), [])

    def test_durable_receipt_survives_adapter_restart(self):
        directory = self.adapter._directory(self.lease)
        directory.mkdir()
        state = {'pid': 123, 'start': '100', 'port': 9000}
        self.adapter._write(directory, state)
        restarted = ChromeAdapter(self.temp.name, 'test.example.ts.net')
        self.assertEqual(restarted._state(self.lease), state)

    def test_cleanup_removes_profile_only_after_group_audit(self):
        directory = self.adapter._directory(self.lease)
        directory.mkdir()
        (directory / 'profile').mkdir()
        self.adapter._write(directory, {'pid': 123, 'start': '100', 'port': 9000})
        with patch.object(self.adapter, '_start_identity', return_value=None), patch.object(
                self.adapter, '_group_alive', return_value=False):
            self.assertEqual(self.adapter.cleanup(self.lease), [])
        self.assertFalse(directory.exists())

    def test_chromium_is_forced_through_filtered_proxy(self):
        process = unittest.mock.Mock()
        process.pid = 123
        process.poll.return_value = None
        with patch('chrome.subprocess.Popen', return_value=process) as popen, \
                patch('chrome.os.chown'), \
                patch.object(self.adapter, '_start_identity', return_value='100'), \
                patch.object(self.adapter, '_discovery', return_value=({}, '/devtools/browser/a')):
            self.adapter._ensure(self.lease)
        launcher = popen.call_args.args[0]
        self.assertIn('--proxy-server=http://127.0.0.1:18888', launcher)
        self.assertIn('--proxy-bypass-list=<-loopback>', launcher)
        self.assertIn('--disable-quic', launcher)
        self.assertIn('--force-webrtc-ip-handling-policy=disable_non_proxied_udp', launcher)
        self.assertIn('--no-sandbox', launcher)

        self.assertTrue(any('prctl(38,1,0,0,0)' in argument for argument in launcher))

    def test_kernel_egress_rules_are_checked_then_installed_in_order(self):
        missing = unittest.mock.Mock(returncode=1)
        installed = unittest.mock.Mock(returncode=0)
        with patch('chrome.subprocess.run', side_effect=[missing, installed] * 5) as run:
            self.real_enforce_egress(self.adapter)
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(len(commands), 10)
        self.assertEqual(commands[0][:5], ['iptables', '--wait', '-C', 'OUTPUT', '-m'])
        self.assertEqual(commands[1][:5], ['iptables', '--wait', '-A', 'OUTPUT', '-m'])
        self.assertIn('ESTABLISHED,RELATED', commands[0])
        self.assertIn('127.0.0.1', commands[2])
        self.assertIn('18888', commands[2])
        self.assertEqual(commands[4][-1], 'REJECT')
        self.assertEqual(commands[6][0], 'ip6tables')
        self.assertEqual(commands[8][-1], 'REJECT')

    def test_kernel_egress_rule_check_fails_closed(self):
        failed = unittest.mock.Mock(returncode=2)
        with patch('chrome.subprocess.run', return_value=failed), self.assertRaises(ProviderFailure):
            self.real_enforce_egress(self.adapter)


class EgressPolicyTests(unittest.TestCase):
    @staticmethod
    def record(address):
        family = socket.AF_INET6 if ':' in address else socket.AF_INET
        sockaddr = (address, 443, 0, 0) if family == socket.AF_INET6 else (address, 443)
        return family, socket.SOCK_STREAM, socket.IPPROTO_TCP, '', sockaddr

    def test_private_and_special_destinations_are_rejected(self):
        for address in ('127.0.0.1', '10.0.0.1', '172.16.0.1', '192.168.0.1',
                        '100.64.0.1', '169.254.1.1', '::1', 'fc00::1', 'fe80::1'):
            with self.subTest(address=address), self.assertRaises(EgressDenied):
                resolve_public('target.example', 443, lambda *args, value=address, **kwargs: [self.record(value)])

    def test_mixed_public_and_private_dns_answer_is_rejected(self):
        answers = [self.record('93.184.216.34'), self.record('10.0.0.1')]
        with self.assertRaises(EgressDenied):
            resolve_public('target.example', 443, lambda *args, **kwargs: answers)

    def test_public_ipv4_and_ipv6_destinations_are_allowed(self):
        for address in ('93.184.216.34', '2606:2800:220:1:248:1893:25c8:1946'):
            with self.subTest(address=address):
                self.assertEqual(resolve_public(
                    'target.example', 443,
                    lambda *args, value=address, **kwargs: [self.record(value)],
                )[0][3][0], address)

    def test_connection_uses_pinned_numeric_answer_without_second_dns_lookup(self):
        fake = unittest.mock.Mock()
        resolver = unittest.mock.Mock(return_value=[self.record('93.184.216.34')])

        def socket_factory(*args):
            return fake

        self.assertIs(connect_public('target.example', 443, resolver=resolver,
                                     socket_factory=socket_factory), fake)
        resolver.assert_called_once()
        fake.connect.assert_called_once_with(('93.184.216.34', 443))


if __name__ == '__main__':
    unittest.main()
