from __future__ import annotations
import importlib.util,json,os,re,sys,tempfile,unittest
from pathlib import Path
from unittest import mock
ROOT=Path(__file__).resolve().parents[3]
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec);sys.modules[name]=module;spec.loader.exec_module(module);return module
preflight=load("axon_live_preflight_test",ROOT/"scripts/e2e/preflight-live.py")
runner=load("axon_live_runner_test",ROOT/"scripts/e2e/run-live.py")
wif=load("axon_wif_claim_test",ROOT/"scripts/e2e/validate-wif-claims.py")
def policy():
 text=(ROOT/"config/tailscale/axon-ci-live-policy.hujson").read_text();return json.loads(re.sub(r"//.*","",text))
class LiveContractTests(unittest.TestCase):
 def test_heartbeat_failure_retains_sanitized_provider_operation_and_cause(self):
  item={"name":"qdrant","url_env":"GATEWAY_URL","auth_env":"GATEWAY_TOKEN"};lease={"lease_id":"opaque"}
  beats=runner.Heartbeats([(item,lease)],"namespace","123","2",1)
  with mock.patch.dict(os.environ,{"GATEWAY_URL":"https://private.example","GATEWAY_TOKEN":"super-secret"},clear=True),mock.patch.object(runner,"call",side_effect=TimeoutError("secret detail")):
   with self.assertRaises(runner.HeartbeatFailure) as caught:beats._beat()
  self.assertEqual({"provider":"qdrant","operation":"lease-heartbeat","cause":"TimeoutError"},caught.exception.evidence())
  self.assertNotIn("secret",json.dumps(caught.exception.evidence()))
 def test_invalid_invariants_and_wrong_binary_are_typed_harness_errors(self):
  valid={"commands":[{"argv":["target/debug/axon"]}],"invariants":["job-terminal"]}
  with self.assertRaises(runner.HarnessError):runner.validate_plan({**valid,"invariants":["invented"]},"a"*40,"a"*40)
  with self.assertRaises(runner.HarnessError):runner.validate_plan({**valid,"commands":[{"argv":["/tmp/not-axon"]}]},"a"*40,"a"*40)
  self.assertEqual(["job-terminal"],runner.validate_plan(valid,"a"*40,"a"*40))
 def test_repeated_termination_is_deferred_after_live_cleanup_begins(self):
  shield=runner.CancellationShield()
  with self.assertRaises(InterruptedError):shield._handle(15,None)
  shield.begin_cleanup();shield._handle(15,None);shield._handle(15,None)
  self.assertTrue(shield.interrupted);self.assertTrue(shield.cleanup)
 def test_workflow_is_trusted_wif_only_and_pinned(self):
  text=(ROOT/".github/workflows/e2e-live.yml").read_text()
  self.assertIn("branches: [main]",text);self.assertIn("schedule:",text);self.assertIn("workflow_dispatch:",text)
  for forbidden in ("pull_request:","pull_request_target:","workflow_run:","oauth-secret:","TS_OAUTH_SECRET","authkey:"):self.assertNotIn(forbidden,text)
  self.assertIn("contents: read\n  id-token: write",text);self.assertIn("environment: axon-live-e2e",text)
  self.assertIn("tailscale/github-action@780049a30b6ff5c378a9e7b389d15ece7a204888",text);self.assertIn("version: 1.94.0",text)
  self.assertIn("tags: tag:axon-ci-e2e",text);self.assertIn("cancel-in-progress: false",text);self.assertIn("ref: ${{ github.sha }}",text)
  self.assertIn("E2E Live admission (no private access)",text);self.assertIn("needs: admission",text);self.assertGreaterEqual(text.count("validate-live-invocation.py"),2)
  for action in re.findall(r"uses:\s*([^\s]+)",text):self.assertRegex(action,r"^[^@]+@[0-9a-f]{40}$")
 def test_wif_claims_are_exact_and_scope_only_ephemeral_tag_creation(self):
  body=json.loads((ROOT/"config/tailscale/axon-ci-wif.json").read_text())
  self.assertEqual("repo:dinglebear-ai/axon:environment:axon-live-e2e",body["subject"]);self.assertEqual(["refs/heads/main"],body["refs"])
  self.assertEqual(["push","schedule","workflow_dispatch"],body["events"]);self.assertEqual({"auth_keys":"write","tags":["tag:axon-ci-e2e"],"ephemeral_only":True},body["scope"])
  self.assertIn("policy",body["denied_capabilities"]);self.assertIn("other_tags",body["denied_capabilities"])
 def test_wif_evaluator_rejects_mutable_claims_and_same_session_replay(self):
  policy=json.loads((ROOT/"config/tailscale/axon-ci-wif.json").read_text());now=2_000_000_000;audience="api.tailscale.com/client-123"
  claims={"iss":policy["issuer"],"aud":audience,"repository_owner":policy["repository_owner"],"repository_owner_id":policy["repository_owner_id"],"repository":policy["repository"],"repository_id":policy["repository_id"],"job_workflow_ref":policy["job_workflow_ref"],"ref":"refs/heads/main","environment":policy["environment"],"sub":policy["subject"],"event_name":"push","iat":now-10,"nbf":now-10,"exp":now+300,"jti":"unique-token-identity-123"}
  seen=set();self.assertTrue(wif.validate(claims,policy,audience,"tag:axon-ci-e2e",seen,now))
  with self.assertRaises(wif.ClaimError):wif.validate(claims,policy,audience,"tag:axon-ci-e2e",seen,now)
  for key,value in (("aud","wrong"),("repository_owner_id","9"),("repository_id","9"),("job_workflow_ref","evil/reusable.yml@refs/heads/main"),("ref","refs/pull/7/merge"),("environment","other"),("event_name","pull_request"),("exp",now-100)):
   bad=dict(claims);bad["jti"]="another-unique-token-456";bad[key]=value
   with self.assertRaises(wif.ClaimError):wif.validate(bad,policy,audience,"tag:axon-ci-e2e",set(),now)
  with self.assertRaises(wif.ClaimError):wif.validate(dict(claims,jti="third-unique-token-789"),policy,audience,"tag:other",set(),now)
 def test_policy_grants_exact_gateway_tags_and_tcp_443_only(self):
  body=policy();self.assertEqual([],body["ssh"]);self.assertNotIn("acls",body)
  self.assertEqual(4,len(body["grants"]));expected={"tag:axon-e2e-qdrant-gateway","tag:axon-e2e-tei-gateway","tag:axon-e2e-chrome-gateway","tag:axon-e2e-llm-gateway"}
  self.assertEqual(expected,{item["dst"][0] for item in body["grants"]})
  for grant in body["grants"]:
   self.assertEqual(["tag:axon-ci-e2e"],grant["src"]);self.assertEqual(["tcp:443"],grant["ip"]);self.assertNotIn("*",json.dumps(grant))
 def test_preflight_requires_exact_peer_https_bearer_and_enforcing_proxy(self):
  item=preflight.load_config()["providers"][0];env={item["url_env"]:"https://qdrant-gateway.example.ts.net",item["peer_env"]:"qdrant-gateway.example.ts.net",item["auth_env"]:"masked"}
  identity={"schema":1,"service":"qdrant","peer":env[item["peer_env"]],"tag":item["tag"],"enforcement":"disposable-tenant-proxy","lease_api":True,"application_auth":"bearer-required","version":"1"}
  peers=[{"DNSName":env[item["peer_env"]]+".","TailscaleIPs":["100.64.1.2"],"Tags":[item["tag"]],"Online":True}]
  evidence=preflight.validate_provider(item,env,lambda _:identity,lambda _:None,peers);self.assertEqual("qdrant",evidence["service"]);self.assertRegex(evidence["identity_sha256"],r"^[0-9a-f]{64}$");self.assertNotIn("peer",evidence);self.assertNotIn("tag",evidence)
  for mutate in (lambda e:e.update({item["url_env"]:"http://qdrant-gateway.example.ts.net"}),lambda e:e.update({item["peer_env"]:"other.example.ts.net"}),lambda e:e.update({item["auth_env"]:""})):
   bad=dict(env);mutate(bad)
   with self.assertRaises(preflight.PreflightError):preflight.validate_provider(item,bad,lambda _:identity,lambda _:None,peers)
  raw=dict(identity);raw["enforcement"]="raw-shared-qdrant"
  with self.assertRaises(preflight.PreflightError):preflight.validate_provider(item,env,lambda _:raw,lambda _:None,peers)
  localhost=dict(env);localhost[item["url_env"]]="https://localhost";localhost[item["peer_env"]]="localhost"
  with self.assertRaises(preflight.PreflightError):preflight.validate_provider(item,localhost,lambda _:identity,lambda _:None,[])
 def test_live_runner_always_deletes_every_acquired_lease_and_audits_residuals(self):
  config=json.loads((ROOT/"config/e2e/live-services.json").read_text());env={"GITHUB_RUN_ID":"123","GITHUB_RUN_ATTEMPT":"2","GITHUB_SHA":"a"*40}
  for item in config["providers"]:env.update({item["url_env"]:f"https://{item['name']}.example.ts.net",item["auth_env"]:"token"})
  calls=[];deleted=set();leases_by_name={}
  def api(url,_token,method,payload):
   calls.append((url,method,payload))
   if url.endswith("/reap"):return {"status":"passed","residuals":[]}
   if url.endswith("/heartbeat"):return {"status":"renewed","heartbeat_at":"2026-08-30T00:00:00Z","expires_at":"2099-01-01T00:00:00Z","namespace":payload["namespace"],"owner":"dinglebear-ai/axon","run_id":"123","run_attempt":"2"}
   if method=="POST":
    name=next(item["name"] for item in config["providers"] if item["url_env"].split("_")[2].lower() in url);lease={"lease_id":payload["lease_id"],"namespace":payload["namespace"],"expires_at":payload["expires_at"],"provider":name,"owner":"dinglebear-ai/axon","run_id":"123","run_attempt":"2","heartbeat_at":payload["heartbeat_at"]};leases_by_name[payload["lease_id"]]=lease;return lease
   name=url.rsplit("/",1)[-1]
   if method=="DELETE":deleted.add(name);return {"status":"deleted","residuals":[]}
   return None if name in deleted else leases_by_name.get(name)
  adapter_deleted=set()
  def adapter_api(_self,resource,method="GET",payload=None):
   name=resource.metadata["lease_id"]
   if method=="DELETE":adapter_deleted.add(name);return {"status":"deleted","residuals":[]}
   return None if name in adapter_deleted else leases_by_name.get(name)
  with tempfile.TemporaryDirectory() as directory:
   path=Path(directory);owned=path/"owned";pf=path/"preflight.json";report=path/"report.json";pf.write_text(json.dumps({"status":"passed"}));env["AXON_E2E_OWNED_ROOT"]=str(owned)
   argv=["run-live.py","--preflight",str(pf),"--report",str(report)]
   failed=mock.Mock(returncode=7,stdout=b"",stderr=b"")
   def process(argv,**_kwargs):
    if argv[:2]==["git","rev-parse"]:return mock.Mock(returncode=0,stdout="a"*40+"\n",stderr="")
    return failed
   with mock.patch.dict(os.environ,env,clear=True),mock.patch.object(sys,"argv",argv),mock.patch.object(runner,"call",side_effect=api),mock.patch.object(runner.subprocess,"run",side_effect=process),mock.patch.object(runner,"run_owned",return_value=failed),mock.patch.object(runner.teardown.provider_api.GatewayLeaseAdapter,"_request",adapter_api):
    self.assertEqual(2,runner.main())
   self.assertEqual([],list(owned.rglob("*")),"successful canonical teardown must remove every run-owned path and manifest")
   body=json.loads(report.read_text());self.assertEqual("product",body["classification"],body);self.assertEqual([{"provider":"canonical-teardown","passed":True}],body["cleanup"],body);self.assertTrue(body["teardown"]["success"]);self.assertTrue(body["manifest_digest"])
   self.assertEqual(4,len(adapter_deleted));self.assertEqual(4,sum(url.endswith("/reap") for url,_method,_payload in calls))
   self.assertEqual(["provider-doctor"],[item["id"] for item in body["scenarios"]],"circuit breaker must prevent later destructive work")
   self.assertGreaterEqual(sum(url.endswith("/heartbeat") for url,_method,_payload in calls),4)
 def test_docs_keep_raw_shared_controls_and_external_mutation_forbidden(self):
  text=(ROOT/"docs/guides/e2e-live-homelab.md").read_text().lower();self.assertIn("never point ci at raw",text);self.assertIn("application bearer",text)
  self.assertIn("stale-lease",text);self.assertIn("non-required",text);self.assertIn("separate read-only job",text);self.assertIn("same evaluation session",text);self.assertIn("tailscale token exchange",text)
 def test_actual_provider_clients_receive_application_credentials_and_canonical_teardown(self):
  runner_text=(ROOT/"scripts/e2e/run-live.py").read_text();tei=(ROOT/"crates/axon-embedding/src/tei/client.rs").read_text();chrome=(ROOT/"crates/axon-adapters/src/web_engine/engine/runtime.rs").read_text()
  self.assertNotIn("?api_key=",runner_text);self.assertIn('QDRANT_API_KEY=os.environ["AXON_E2E_QDRANT_TOKEN"]',runner_text);self.assertIn("AXON_TEI_BEARER_TOKEN",runner_text+tei);self.assertIn("bearer_auth(token)",tei)
  self.assertIn("AXON_CHROME_BEARER_TOKEN",runner_text+chrome);self.assertIn("bearer_auth(token)",chrome)
  self.assertIn("write_setup_intent",runner_text);self.assertIn("write_provider_ledger",runner_text);self.assertIn("teardown.Engine",runner_text)
  self.assertIn("GatewayLeaseAdapter",(ROOT/"scripts/e2e/lib/axon_e2e_provider_state.py").read_text())
 def test_preflight_failure_still_emits_sanitized_classified_evidence(self):
  with tempfile.TemporaryDirectory() as directory:
   path=Path(directory);pf=path/"preflight.json";report=path/"report.json";pf.write_text(json.dumps({"status":"failed","classification":"auth","sanitized":True}))
   env={"GITHUB_RUN_ID":"123","GITHUB_RUN_ATTEMPT":"1","GITHUB_SHA":"a"*40};argv=["run-live.py","--preflight",str(pf),"--report",str(report)]
   with mock.patch.dict(os.environ,env,clear=True),mock.patch.object(sys,"argv",argv):self.assertEqual(2,runner.main())
   body=json.loads(report.read_text());self.assertEqual("auth",body["classification"]);self.assertFalse(body["success"]);self.assertTrue(body["sanitized"])
 def test_live_verifier_rejects_private_tailnet_identity_even_when_marked_sanitized(self):
  text=(ROOT/"scripts/e2e/verify-live-report.py").read_text();self.assertIn("ts\\.net",text);self.assertIn("redaction.scan_bytes",text)
 def test_oversize_probe_never_places_bearer_in_process_argv(self):
  text=(ROOT/"tests/e2e/scenarios/security/hermetic_entry.py").read_text();self.assertNotIn('f"Authorization: Bearer {token}"',text.split("def oversize_probe",1)[1].split("def assert_clean_capture",1)[0].replace('connection.putheader("Authorization",f"Bearer {token}")',""));self.assertIn("http.client",text)
if __name__=="__main__":unittest.main()
