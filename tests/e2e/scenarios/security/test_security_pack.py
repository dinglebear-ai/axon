import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SPEC=importlib.util.spec_from_file_location("security",Path(__file__).with_name("security_pack.py"))
security=importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(security)


class SecurityPackTests(unittest.TestCase):
    def test_exhaustive_ssrf_alternate_forms_are_rejected_without_connections(self):
        dns={"rebind.axon-e2e.invalid":["198.51.100.2","127.0.0.1"],
             "redirect.axon-e2e.invalid":["127.0.0.1"]}
        self.assertGreaterEqual(len(security.SSRF_CASES),17)
        for url in security.SSRF_CASES:
            classification=security.forbidden_destination(url,dns)
            self.assertNotEqual("allowed",classification,url)
            security.assert_zero_connections(0,0,classification)
            with self.assertRaises(security.SecurityError):
                security.assert_zero_connections(0,1,classification)

    def test_provider_boundary_rejects_nonowned_admin_enumeration_and_active(self):
        run="axon_e2e_run"
        cases=[
            ("qdrant","prod","collection.delete",None,False,"provider.not_owned"),
            ("qdrant","axon_e2e_c","collection.list",run+":m",False,"provider.operation_forbidden"),
            ("qdrant","axon_e2e_c","admin.delete",run+":m",False,"provider.operation_forbidden"),
            ("chrome","operator","profile.delete",None,False,"provider.not_owned"),
            ("chrome","axon_e2e_p","session.close",run+":m",True,"provider.resource_active"),
        ]
        for resource,identity,operation,marker,active,expected in cases:
            self.assertEqual(expected,security.provider_boundary(resource,identity,operation,run,marker,active))

    def test_auth_matrix_covers_all_locked_surfaces(self):
        fixture=json.loads((Path(__file__).parents[2]/"fixtures/security/auth-policy.json").read_text())
        surfaces={row[0] for row in security.AUTH_MATRIX}
        self.assertEqual(set(fixture["required_surfaces"]),surfaces)
        for surface,route,*_ in security.AUTH_MATRIX:
            security.validate_auth_observation({"surface":surface,"route":route,"credential":"invalid",
                                                "status":401,"error_code":"auth.invalid"})
        with self.assertRaises(security.SecurityError):
            security.validate_auth_observation({"surface":"rest","route":"/v1/status","credential":"missing",
                                                "status":200,"error_code":"","nonloopback":True})
        oauth={"surface":"oauth","route":"/authorize","credential":"valid","status":302,
               "error_code":"none","oauth":True,"state_verified":True,"pkce_verified":True,
               "redirect_verified":True,"claims_verified":False}
        with self.assertRaisesRegex(security.SecurityError,"OAuth"): security.validate_auth_observation(oauth)

    def test_canary_scanner_detects_every_transformation_without_leaking_secret(self):
        secret=json.loads((Path(__file__).parents[2]/"fixtures/security/hostile.json").read_text())["canary"]
        for encoding,value in security.transformations(secret).items():
            findings=security.scan_artifact(("prefix "+value+" suffix").encode(),[secret])
            self.assertIn(encoding,{item["encoding"] for item in findings})
            self.assertNotIn(secret,json.dumps(findings))
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); (root/"report.json").write_text('{"token":"[REDACTED]"}')
            security.scan_tree(root,[secret])
            (root/"bad.txt").write_text(security.transformations(secret)["hex"])
            with self.assertRaisesRegex(security.SecurityError,"canary detected"): security.scan_tree(root,[secret])

    def test_hostile_fixture_covers_path_ids_upload_and_prompt_injection(self):
        fixture=json.loads((Path(__file__).parents[2]/"fixtures/security/hostile.json").read_text())
        self.assertGreaterEqual(len(fixture["path_traversal"]),4)
        self.assertGreaterEqual(len(fixture["malformed_ids"]),5)
        self.assertGreater(fixture["oversized_upload_bytes"],1_048_576)
        self.assertIn("Ignore ownership",fixture["hostile_content"])

    def test_domain_catalog_accounts_for_every_locked_security_family(self):
        catalog=json.loads((Path(__file__).with_name("scenarios.json")).read_text())
        ids={item["id"] for item in catalog["scenarios"]}
        self.assertTrue({"security.auth.matrix","security.ssrf.alternate_forms",
                         "security.path.traversal","security.upload.oversized",
                         "security.identifier.malformed","security.hostile.content",
                         "security.provider.qdrant","security.provider.chrome",
                         "security.artifact.canary"} <= ids)

    def test_composed_entry_exists_so_security_is_not_unit_only(self):
        entry=Path(__file__).with_name("hermetic_entry.py")
        self.assertTrue(entry.is_file())
        text=entry.read_text()
        for contract in ("http.run_probes", "mcp_auth.matrix", "post_source", "spawn_owned_process",
                         "teardown.py", "http.inventory"):
            self.assertIn(contract,text)

if __name__=="__main__": unittest.main()
