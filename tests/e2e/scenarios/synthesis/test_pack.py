import importlib.util
import unittest
from pathlib import Path
spec = importlib.util.spec_from_file_location("synthesis_pack", Path(__file__).with_name("pack.py"))
pack = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pack)


class SynthesisPackTests(unittest.TestCase):
    def response(self, method, path, body, authenticated):
        if not authenticated:
            return 401, {"ok": False, "error": {"code": "unauthorized"}}
        if method == "GET" or body.get("switch_account"):
            return 503, {"ok": False, "error": {"code": "synthesis_account_unsupported"}}
        if "login_id" in body:
            return 404, {"ok": False, "error": {"code": "synthesis_account_login_unknown"}}
        return 400, {"ok": False, "error": {"code": "synthesis_account_confirmation_required"}}

    def test_all_account_paths_have_real_failure_and_admin_assertions(self):
        seen = []
        def call(*args):
            seen.append(args)
            return self.response(*args)
        result = pack.run(call)
        self.assertEqual(8, len(seen))
        self.assertEqual(8, result["requests"])
        self.assertEqual("surface:synthesis", result["operation_id"])

    def test_auth_bypass_is_rejected(self):
        def bypass(method, path, body, authenticated):
            return self.response(method, path, body, True)
        with self.assertRaisesRegex(RuntimeError, "admin boundary"):
            pack.run(bypass)

    def test_successful_unconfigured_mutation_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "rejection mismatch"):
            pack.run(lambda *_: (200, {"ok": True}))
