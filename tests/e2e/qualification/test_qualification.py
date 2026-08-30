import copy, hashlib, importlib.util, json, sys, tempfile, unittest
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3]
spec=importlib.util.spec_from_file_location("qualification_under_test",ROOT/"scripts/e2e/lib/qualification.py")
q=importlib.util.module_from_spec(spec);sys.modules[spec.name]=q;spec.loader.exec_module(q)
rspec=importlib.util.spec_from_file_location("reporting_for_fixture",ROOT/"scripts/e2e/lib/reporting.py")
reporting=importlib.util.module_from_spec(rspec);sys.modules[rspec.name]=reporting;rspec.loader.exec_module(reporting)
SHA="a"*40

def sha(data):return hashlib.sha256(data).hexdigest()

class QualificationTests(unittest.TestCase):
 def setUp(self):
  self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name)
  catalog={"schema_version":1,"scenarios":[]};corpus={"corpus_version":"1.0.0"}
  (self.root/"catalog.json").write_text(json.dumps(catalog));(self.root/"corpus.json").write_text(json.dumps(corpus))
  self.policy=json.loads((ROOT/"config/e2e/qualification-policy.json").read_text())
  self.policy["profiles"]["test"]={"families":{family:"optional" for family in q.FAMILIES}}
  self.policy["profiles"]["test"]["families"]["hermetic"]="required"
  scenario=reporting.Scenario("source.cli","micro","source","cli");scenario.attempt("passed",1);scenario.cleanup={"success":True,"residual":[],"refused":[],"manifest_digest":"b"*64}
  report=reporting.suite_report([scenario],tested_sha=SHA,provider_versions={"qdrant":"1.18.2"},policy={})
  raw=json.dumps(report).encode();(self.root/"hermetic.json").write_bytes(raw)
  cbytes=(self.root/"catalog.json").read_bytes();xbytes=(self.root/"corpus.json").read_bytes()
  self.index={"schema":1,"profile":"test","policy_version":"1.0.0","tested_sha":SHA,"as_of":"2026-08-30T12:00:00Z",
   "subject":{"tested_sha":SHA,"product_version":"7.2.23","catalog_version":1,"catalog_sha256":sha(cbytes),"corpus_version":"1.0.0","corpus_sha256":sha(xbytes),
    "sources":{"catalog":{"path":"catalog.json","sha256":sha(cbytes)},"corpus":{"path":"corpus.json","sha256":sha(xbytes)}}},
   "coverage":{"capabilities":["source"],"surfaces":["cli"],"catalog_covered":1,"catalog_total":1},"not_applicable":{},
   "artifacts":[{"id":"hermetic","family":"hermetic","path":"hermetic.json","sha256":sha(raw),"bytes":len(raw),"redaction_class":"sanitized","format":"canonical-report","retention":{"location":"github-artifact","days":30},
    "producer":{"repository":"dinglebear-ai/axon","workflow":"e2e-hermetic","ref":"refs/heads/main","run_id":"123","run_attempt":1,"tested_sha":SHA,"completed_at":"2026-08-30T11:00:00Z"}}]}
 def tearDown(self):self.temp.cleanup()
 def build(self,index=None):return q.build(index or self.index,self.policy,self.root)
 def test_pass_is_deterministic_but_unsigned(self):
  one,d1=self.build();two,d2=self.build();self.assertEqual(one,two);self.assertEqual(d1,d2);self.assertEqual("passed",one["qualification"]["outcome"]);self.assertFalse(one["qualification"]["release_eligible"])
  hermetic=next(x for x in one["families"] if x["family"]=="hermetic");self.assertEqual({"qdrant":"1.18.2"},hermetic["evidence"][0]["projection"]["provider_versions"]);self.assertTrue(hermetic["evidence"][0]["projection"]["teardown"]["success"])
 def test_missing_required_is_incomplete(self):
  index=copy.deepcopy(self.index);index["artifacts"]=[];manifest,_=self.build(index);self.assertEqual("incomplete",manifest["qualification"]["outcome"]);self.assertEqual("not_run",next(x for x in manifest["families"] if x["family"]=="hermetic")["state"])
 def test_digest_sha_staleness_and_future_fail_closed(self):
  for mutate,message in [
   (lambda x:x["artifacts"][0].update(sha256="0"*64),"digest mismatch"),
   (lambda x:x["artifacts"][0]["producer"].update(tested_sha="b"*40),"producer tested SHA mismatch"),
   (lambda x:x["artifacts"][0]["producer"].update(completed_at="2020-01-01T00:00:00Z"),"stale"),
   (lambda x:x["artifacts"][0]["producer"].update(completed_at="2026-08-31T00:00:00Z"),"future")]:
   index=copy.deepcopy(self.index);mutate(index)
   with self.assertRaisesRegex(q.QualificationError,message):self.build(index)
 def test_corpus_and_catalog_are_content_bound(self):
  index=copy.deepcopy(self.index);index["subject"]["catalog_version"]=2
  with self.assertRaisesRegex(q.QualificationError,"catalog version mismatch"):self.build(index)
 def test_unapproved_exception_and_unsafe_metadata_reject(self):
  index=copy.deepcopy(self.index);index["outage_exception"]={"id":"incident-1"}
  with self.assertRaisesRegex(q.QualificationError,"not policy-approved"):self.build(index)
  value=json.loads((self.root/"hermetic.json").read_text());value["token"]="redacted";raw=json.dumps(value).encode();(self.root/"hermetic.json").write_bytes(raw)
  index=copy.deepcopy(self.index);index["artifacts"][0].update(sha256=sha(raw),bytes=len(raw))
  with self.assertRaises(q.QualificationError):self.build(index)
 def test_required_not_applicable_needs_policy_and_rationale(self):
  self.policy["profiles"]["test"]["families"]["hermetic"]="not_applicable";index=copy.deepcopy(self.index);index["artifacts"]=[]
  with self.assertRaisesRegex(q.QualificationError,"rationale missing"):self.build(index)
  index["not_applicable"]["hermetic"]="Hermetic execution is outside this deployed-only profile."
  manifest,_=self.build(index);self.assertEqual("passed",manifest["qualification"]["outcome"])

if __name__=="__main__":unittest.main()
