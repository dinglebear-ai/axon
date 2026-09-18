from pathlib import Path
import unittest

PLUGIN = Path(__file__).resolve().parents[1]
ROOT = PLUGIN.parents[1]
SKILL = PLUGIN / "skills" / "install-axon" / "SKILL.md"
REF = PLUGIN / "skills" / "install-axon" / "references" / "setup.md"
OPENAI = PLUGIN / "skills" / "install-axon" / "agents" / "openai.yaml"

class InstallAxonContractTest(unittest.TestCase):
    def test_security_and_deployment_contract(self):
        text = SKILL.read_text() + REF.read_text()
        required = ["AXON_INSTALL_METHOD=build ./install.sh", "axon setup init", "AXON_HTTP_TOKEN", "AXON_GOOGLE_CLIENT_SECRET", "0600", "auth/google/callback", "--auth-admin-email", "AXON_LLM_BACKEND=codex-app-server", "AXON_INCUS_RUN_SERVER=true", "explicit approval"]
        for value in required:
            self.assertIn(value, text)

    def test_skill_is_explicit(self):
        self.assertIn("allow_implicit_invocation: false", OPENAI.read_text())

    def test_setup_cli_does_not_expose_google_secret(self):
        args = (ROOT / "crates" / "axon-core" / "src" / "config" / "cli" / "setup_args.rs").read_text()
        dispatch = (ROOT / "crates" / "axon-core" / "src" / "config" / "parse" / "build_config" / "command_dispatch.rs").read_text()
        runtime = (ROOT / "crates" / "axon-cli" / "src" / "commands" / "setup.rs").read_text()
        for text in [args, dispatch, runtime]:
            self.assertNotIn("google-client-secret", text)

if __name__ == "__main__":
    unittest.main()
