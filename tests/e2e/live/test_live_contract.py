from __future__ import annotations
import importlib.util,json,os,re,sys,tempfile,unittest,urllib.error
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
 def test_live_lease_data_token_requires_exact_scoped_token_format(self):
  valid="axe1_"+"a"*64
  for value in (None,"","x"*64,"axe1_"+"a"*63,"axe1_"+"A"*64,"axe1_"+"g"*64,valid+"0"):
   with self.assertRaises(RuntimeError):runner.take_data_token({"data_token":value})
  lease={"data_token":valid};self.assertEqual(valid,runner.take_data_token(lease));self.assertNotIn("data_token",lease)
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
  self.assertIn("environment: axon-live-e2e-manual",text)
  self.assertIn("if: github.event_name == 'workflow_dispatch'",text)
  self.assertIn("needs: [admission, manual-approval]",text)
  self.assertIn("needs.manual-approval.result == 'success'",text)
  self.assertIn("tailscale/github-action@780049a30b6ff5c378a9e7b389d15ece7a204888",text);self.assertIn("version: 1.94.0",text)
  self.assertIn("tags: tag:axon-ci-e2e",text);self.assertIn("cancel-in-progress: false",text);self.assertIn("ref: ${{ github.sha }}",text)
  self.assertIn("E2E Live admission (no private access)",text);self.assertGreaterEqual(text.count("validate-live-invocation.py"),2)
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
 def test_preflight_rejects_reused_management_tokens_without_leaking_values(self):
  config=preflight.load_config();env={item["auth_env"]:f"token-{item['name']}" for item in config["providers"]}
  preflight.validate_management_tokens(config,env)
  marker="shared-management-secret"
  env[config["providers"][0]["auth_env"]]=marker;env[config["providers"][1]["auth_env"]]=marker
  with self.assertRaises(preflight.PreflightError) as caught:preflight.validate_management_tokens(config,env)
  self.assertNotIn(marker,str(caught.exception))
 def test_live_runner_always_deletes_every_acquired_lease_and_audits_residuals(self):
  config=json.loads((ROOT/"config/e2e/live-services.json").read_text());env={"GITHUB_RUN_ID":"123","GITHUB_RUN_ATTEMPT":"2","GITHUB_SHA":"a"*40,"GITHUB_ACTIONS":"true"}
  admin_tokens={};data_tokens={item["name"]:"axe1_"+format(index,"064x") for index,item in enumerate(config["providers"],1)}
  for item in config["providers"]:
   admin_tokens[item["name"]]=f"admin-{item['name']}";env.update({item["url_env"]:f"https://{item['name']}.example.ts.net",item["auth_env"]:admin_tokens[item["name"]]})
  calls=[];deleted=set();leases_by_name={};command_envs=[]
  def api(url,token,method,payload):
   name=next(item["name"] for item in config["providers"] if item["url_env"].split("_")[2].lower() in url)
   self.assertEqual(admin_tokens[name],token,"lease management must retain the static root token")
   calls.append((url,method,payload))
   if url.endswith("/reap"):return {"status":"passed","residuals":[]}
   if url.endswith("/heartbeat"):return {"status":"renewed","heartbeat_at":"2026-08-30T00:00:00Z","expires_at":"2099-01-01T00:00:00Z","namespace":payload["namespace"],"owner":"dinglebear-ai/axon","run_id":"123","run_attempt":"2"}
   if method=="POST":
    lease={"lease_id":payload["lease_id"],"namespace":payload["namespace"],"expires_at":payload["expires_at"],"provider":name,"owner":"dinglebear-ai/axon","run_id":"123","run_attempt":"2","heartbeat_at":payload["heartbeat_at"],"data_token":data_tokens[name]};leases_by_name[payload["lease_id"]]=lease;return lease
   name=url.rsplit("/",1)[-1]
   if method=="DELETE":deleted.add(name);return {"status":"deleted","residuals":[]}
   return None if name in deleted else leases_by_name.get(name)
  adapter_deleted=set()
  def adapter_api(_self,resource,method="GET",payload=None):
   name=resource.metadata["lease_id"]
   self.assertEqual(admin_tokens[resource.metadata["provider"]],os.environ[resource.metadata["token_env"]])
   if method=="DELETE":adapter_deleted.add(name);return {"status":"deleted","residuals":[]}
   lease=None if name in adapter_deleted else leases_by_name.get(name)
   return None if lease is None else {key:value for key,value in lease.items() if key!="data_token"}
  with tempfile.TemporaryDirectory() as directory:
   path=Path(directory);owned=path/"owned";pf=path/"preflight.json";report=path/"report.json";pf.write_text(json.dumps({"status":"passed"}));env["AXON_E2E_OWNED_ROOT"]=str(owned)
   argv=["run-live.py","--preflight",str(pf),"--report",str(report)]
   failed=mock.Mock(returncode=7,stdout=b"",stderr=b"")
   def process(argv,**_kwargs):
    if argv[:2]==["git","rev-parse"]:return mock.Mock(returncode=0,stdout="a"*40+"\n",stderr="")
    return failed
   def run_owned(_manifest,_run_root,_scenario,command_env,_heartbeats):self.assertEqual(4,printed.call_count,"data tokens must be masked before commands run");command_envs.append(command_env);return failed
   with mock.patch.dict(os.environ,env,clear=True),mock.patch.object(sys,"argv",argv),mock.patch.object(runner,"call",side_effect=api),mock.patch.object(runner.subprocess,"run",side_effect=process),mock.patch.object(runner,"run_owned",side_effect=run_owned),mock.patch.object(runner.teardown.provider_api.GatewayLeaseAdapter,"_request",adapter_api),mock.patch("builtins.print") as printed:
    self.assertEqual(2,runner.main())
   self.assertEqual([],list(owned.rglob("*")),"successful canonical teardown must remove every run-owned path and manifest")
   body=json.loads(report.read_text());self.assertEqual("product",body["classification"],body);self.assertEqual([{"provider":"canonical-teardown","passed":True}],body["cleanup"],body);self.assertTrue(body["teardown"]["success"]);self.assertTrue(body["manifest_digest"])
   self.assertEqual(4,len(adapter_deleted));self.assertEqual(4,sum(url.endswith("/reap") for url,_method,_payload in calls))
   self.assertEqual(["provider-doctor"],[item["id"] for item in body["scenarios"]],"circuit breaker must prevent later destructive work")
   self.assertGreaterEqual(sum(url.endswith("/heartbeat") for url,_method,_payload in calls),4)
   command_env=command_envs[0];self.assertEqual(data_tokens["qdrant"],command_env["QDRANT_API_KEY"]);self.assertEqual(data_tokens["tei"],command_env["AXON_TEI_BEARER_TOKEN"]);self.assertEqual(data_tokens["chrome"],command_env["AXON_CHROME_BEARER_TOKEN"]);self.assertEqual(data_tokens["llm"],command_env["AXON_OPENAI_API_KEY"])
   self.assertEqual(body["namespace"],command_env["AXON_COLLECTION"]);self.assertFalse(set(admin_tokens.values())&set(command_env.values()),"root management tokens must not enter command environment")
   self.assertEqual([mock.call(f"::add-mask::{data_tokens[item['name']]}",flush=True) for item in config["providers"]],printed.call_args_list)
   encoded=json.dumps(body);self.assertTrue(all(token not in encoded for token in data_tokens.values()));self.assertTrue(all(token not in encoded for token in admin_tokens.values()))
 def test_live_runner_cleans_intent_when_first_provider_lease_is_absent(self):
  config=json.loads((ROOT/"config/e2e/live-services.json").read_text());env={"GITHUB_RUN_ID":"123","GITHUB_RUN_ATTEMPT":"2","GITHUB_SHA":"a"*40}
  for item in config["providers"]:env.update({item["url_env"]:f"https://{item['name']}.example.ts.net",item["auth_env"]:"token"})
  def api(url,_token,method,_payload):
   if url.endswith("/reap"):return {"status":"passed","residuals":[]}
   if method=="POST":raise urllib.error.URLError(TimeoutError("gateway unavailable"))
   raise AssertionError(f"unexpected direct gateway request: {method} {url}")
  with tempfile.TemporaryDirectory() as directory:
   path=Path(directory);owned=path/"owned";pf=path/"preflight.json";report=path/"report.json";pf.write_text(json.dumps({"status":"passed"}));env["AXON_E2E_OWNED_ROOT"]=str(owned)
   argv=["run-live.py","--preflight",str(pf),"--report",str(report)]
   with mock.patch.dict(os.environ,env,clear=True),mock.patch.object(sys,"argv",argv),mock.patch.object(runner,"call",side_effect=api),mock.patch.object(runner.teardown.provider_api.GatewayLeaseAdapter,"_request",return_value=None):
    self.assertEqual(2,runner.main())
   body=json.loads(report.read_text());self.assertEqual("network",body["classification"],body);self.assertEqual("TimeoutError",body["failure_detail"],body)
   self.assertFalse(body["success"]);self.assertEqual([],body["scenarios"]);self.assertEqual([],body["invariants"])
   self.assertTrue(body["cleanup"][0]["passed"]);self.assertTrue(body["teardown"]["success"])
   self.assertEqual([],list(owned.rglob("*")),"authoritative absence must retire all run-owned authority")
 def test_lost_lease_create_response_is_reconciled_from_signed_intent(self):
  config=json.loads((ROOT/"config/e2e/live-services.json").read_text());env={"GITHUB_RUN_ID":"123","GITHUB_RUN_ATTEMPT":"2","GITHUB_SHA":"a"*40}
  for item in config["providers"]:env.update({item["url_env"]:f"https://{item['name']}.example.ts.net",item["auth_env"]:"token"})
  created={};deleted=set()
  def api(url,_token,method,payload):
   if url.endswith("/reap"):return {"status":"passed","residuals":[]}
   if method=="POST":
    provider=config["providers"][0]["name"]
    created[payload["lease_id"]]={"lease_id":payload["lease_id"],"namespace":payload["namespace"],"provider":provider,"owner":payload["owner"],"run_id":payload["run_id"],"run_attempt":payload["run_attempt"],"expires_at":payload["expires_at"],"heartbeat_at":payload["heartbeat_at"]}
    raise urllib.error.URLError(TimeoutError("response lost after durable create"))
   raise AssertionError(f"unexpected direct gateway request: {method} {url}")
  def adapter_api(_self,resource,method="GET",payload=None):
   lease_id=resource.metadata["lease_id"]
   if method=="DELETE":
    self.assertEqual({"namespace":resource.metadata["namespace"],"owner":"dinglebear-ai/axon","residual_audit":True},payload);deleted.add(lease_id);return {"status":"deleted","residuals":[]}
   return None if lease_id in deleted else created.get(lease_id)
  with tempfile.TemporaryDirectory() as directory:
   path=Path(directory);owned=path/"owned";pf=path/"preflight.json";report=path/"report.json";pf.write_text(json.dumps({"status":"passed"}));env["AXON_E2E_OWNED_ROOT"]=str(owned)
   argv=["run-live.py","--preflight",str(pf),"--report",str(report)]
   with mock.patch.dict(os.environ,env,clear=True),mock.patch.object(sys,"argv",argv),mock.patch.object(runner,"call",side_effect=api),mock.patch.object(runner.teardown.provider_api.GatewayLeaseAdapter,"_request",adapter_api):
    self.assertEqual(2,runner.main())
   body=json.loads(report.read_text());self.assertEqual("network",body["classification"],body);self.assertTrue(body["cleanup"][0]["passed"],body);self.assertTrue(body["teardown"]["success"],body)
   self.assertEqual(1,len(created));self.assertEqual(set(created),deleted);lease_id=next(iter(created));self.assertEqual(f"{body['namespace']}_qdrant",lease_id)
   self.assertEqual([],list(owned.rglob("*")),"reconciled cleanup must retire all run-owned authority")
 def test_docs_keep_raw_shared_controls_and_external_mutation_forbidden(self):
  text=(ROOT/"docs/guides/e2e-live-homelab.md").read_text().lower();self.assertIn("never point ci at raw",text);self.assertIn("application bearer",text)
  self.assertIn("stale-lease",text);self.assertIn("non-required",text);self.assertIn("separate read-only job",text);self.assertIn("same evaluation session",text);self.assertIn("tailscale token exchange",text)
 def test_actual_provider_clients_receive_application_credentials_and_canonical_teardown(self):
  runner_text=(ROOT/"scripts/e2e/run-live.py").read_text();tei=(ROOT/"crates/axon-embedding/src/tei/client.rs").read_text();chrome=(ROOT/"crates/axon-adapters/src/web_engine/engine/runtime.rs").read_text()
  self.assertNotIn("?api_key=",runner_text);self.assertIn('QDRANT_API_KEY=data_tokens["qdrant"]',runner_text);self.assertIn("AXON_TEI_BEARER_TOKEN",runner_text+tei);self.assertIn("bearer_auth(token)",tei)
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
