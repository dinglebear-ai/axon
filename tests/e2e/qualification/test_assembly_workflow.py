import re,unittest
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3]
WORKFLOW=ROOT/".github/workflows/e2e-qualification-assemble.yml"

class AssemblyWorkflowTests(unittest.TestCase):
 def test_exact_sha_producers_are_verified_and_qualifier_is_invoked(self):
  text=WORKFLOW.read_text()
  self.assertIn('value.get("head_sha") != os.environ["EXPECTED_SHA"]',text)
  self.assertIn('value.get("conclusion") != "success"',text)
  self.assertIn("ref: ${{ github.workflow_sha }}",text)
  self.assertNotIn("ref: ${{ inputs.tested_sha }}",text)
  self.assertEqual(4,text.count("actions/download-artifact@"))
  self.assertIn("name: e2e-qualification-input",text)
  self.assertIn("uses: ./.github/workflows/e2e-qualification.yml",text)
  self.assertIn("evidence_run_id: ${{ needs.assemble.outputs.evidence_run_id }}",text)
  self.assertIn("assembled_in_current_run: true",text)
  for action in re.findall(r"uses:\s*([^\s'{]+@[0-9a-f]+)",text):self.assertRegex(action,r"^[^@]+@[0-9a-f]{40}$")
 def test_bundle_is_assembled_only_from_named_lane_outputs(self):
  text=WORKFLOW.read_text()
  for family in ("hermetic","live","platform","performance"):self.assertIn(f'"family":"{family}"',text)
  self.assertIn("assemble-qualification-input.py",text)

if __name__=="__main__":unittest.main()
