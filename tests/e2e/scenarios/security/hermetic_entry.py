#!/usr/bin/env python3
"""Real composed security entry: Axon + transport clients + owned manifest."""
from __future__ import annotations
import argparse,base64,hashlib,http.client as http_client,importlib.util,json,os,subprocess,sys,time,urllib.error,urllib.parse,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4]
def load(name,path):
 spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec)
 assert spec and spec.loader;sys.modules[name]=module;spec.loader.exec_module(module);return module
isolation=load("security_isolation",ROOT/"scripts/e2e/lib/run-isolation.py")
http=load("security_http_adapter",ROOT/"scripts/e2e/adapters/http_adapter.py")
mcp_auth=load("security_mcp_auth",ROOT/"scripts/e2e/adapters/mcp_auth.py")
taskwire=load("security_taskwire",ROOT/"scripts/test-mcp-tasks-wire.py")
security=load("security_pack_live",Path(__file__).with_name("security_pack.py"))

def wait(url,process):
 for _ in range(200):
  if process.process.poll() is not None:raise RuntimeError("owned authenticated Axon exited before ready")
  try:urllib.request.urlopen(url,timeout=.1);return
  except urllib.error.HTTPError as error:
   try:
    if error.code in {401,403}:return
   finally:error.close()
  except OSError:pass
  time.sleep(.025)
 raise RuntimeError("owned authenticated Axon did not become ready")

def post_source(base,url,token):
 response=http.request(base,token,http.json_request("POST","/v1/sources",{"source":url,"scope":"page","wait":True}),8)
 try:body=json.loads(response.body)
 except json.JSONDecodeError:body={}
 code=body.get("code") or (body.get("error",{}).get("code") if isinstance(body.get("error"),dict) else None)
 if response.status not in {400,403,422} or not isinstance(code,str):
  raise RuntimeError(f"Axon did not return a structured SSRF rejection: {response.status} {body}")
 return {"status":response.status,"code":code}
def provider_request(base,method,path):
 request=urllib.request.Request(base+path,method=method,headers={"content-type":"application/json"},data=b"{}" if method in {"POST","PUT"} else None)
 try:urllib.request.urlopen(request,timeout=2)
 except urllib.error.HTTPError as error:
  try:
   body=json.loads(error.read());
   if error.code!=403 or body.get("error",{}).get("code")!="provider.not_owned":raise RuntimeError("provider double returned wrong rejection")
   return body
  finally:error.close()
 raise RuntimeError("provider boundary accepted non-owned request")
def error_code(response):
 try:value=json.loads(response.body)
 except json.JSONDecodeError:raise RuntimeError(f"HTTP {response.status} returned an unstructured error")
 code=value.get("code")
 if not isinstance(code,str) and isinstance(value.get("error"),dict):code=value["error"].get("code")
 if not isinstance(code,str) or not code:raise RuntimeError(f"HTTP {response.status} omitted an exact error code")
 return code
def oversize_probe(base,token,path,size):
 path.write_bytes(b"x"*size)
 parsed=urllib.parse.urlsplit(base);connection_type=http_client.HTTPSConnection if parsed.scheme=="https" else http_client.HTTPConnection
 connection=connection_type(parsed.hostname,parsed.port,timeout=15)
 try:
  connection.putrequest("POST",(parsed.path.rstrip("/") if parsed.path else "")+"/v1/uploads")
  connection.putheader("Authorization",f"Bearer {token}");connection.putheader("content-type","application/json");connection.putheader("content-length",str(size));connection.endheaders()
  with path.open("rb") as stream:
   try:
    while chunk:=stream.read(64*1024):connection.send(chunk)
   except (BrokenPipeError,ConnectionResetError):
    # A server may reject the declared oversized body before consuming it.
    # The response remains authoritative and must still be inspected below.
    pass
  response=connection.getresponse();body=response.read();status=response.status
 finally:connection.close();path.unlink(missing_ok=True)
 return http.HttpResponse(status,{},body)
def assert_clean_capture(label,data,secrets):
 findings=security.scan_artifact(data if isinstance(data,bytes) else data.encode(),secrets)
 if findings:raise RuntimeError(f"secret material detected in captured {label}: {findings}")
class NoRedirect(urllib.request.HTTPRedirectHandler):
 def redirect_request(self,*_):return None
def oauth_flow(base,scope):
 verifier=f"verifier-{scope}-axon-e2e";challenge=base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).decode().rstrip("=");state=f"state-{scope}"
 query=urllib.parse.urlencode({"response_type":"code","client_id":"e2e-client","redirect_uri":"http://127.0.0.1/callback",
  "state":state,"code_challenge":challenge,"code_challenge_method":"S256","scope":scope})
 try:urllib.request.build_opener(NoRedirect()).open(base+"/authorize?"+query,timeout=2)
 except urllib.error.HTTPError as error:
  try:
   if error.code!=302:raise
   location=error.headers["location"]
  finally:error.close()
 parsed=urllib.parse.parse_qs(urllib.parse.urlsplit(location).query)
 if parsed.get("state")!=[state]:raise RuntimeError("OAuth state mismatch")
 form=urllib.parse.urlencode({"grant_type":"authorization_code","code":parsed["code"][0],"client_id":"e2e-client",
  "redirect_uri":"http://127.0.0.1/callback","code_verifier":verifier}).encode()
 response=json.loads(urllib.request.urlopen(urllib.request.Request(base+"/token",form,{"content-type":"application/x-www-form-urlencoded"}),timeout=2).read())
 segment=response["access_token"].split(".")[1];claims=json.loads(base64.urlsafe_b64decode(segment+"="*(-len(segment)%4)))
 if claims.get("scope")!=scope or claims.get("email_verified") is not True:raise RuntimeError("OAuth claims/scope mismatch")
 return {"scope":scope,"state_verified":True,"pkce_verified":True}

def axon_oauth_flow(axon_base,scope):
 redirect_uri="http://127.0.0.1:65534/callback"
 registered=http.request(axon_base,None,http.json_request("POST","/register",{"redirect_uris":[redirect_uri]}),8)
 if registered.status!=200:raise RuntimeError(f"Axon OAuth registration failed: {registered.status} {registered.body}")
 client_id=json.loads(registered.body)["client_id"]
 verifier=f"axon-client-verifier-{scope}";challenge=base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).decode().rstrip("=");state=f"axon-state-{scope}"
 authorize=axon_base+"/authorize?"+urllib.parse.urlencode({"response_type":"code","client_id":client_id,"redirect_uri":redirect_uri,
  "state":state,"code_challenge":challenge,"code_challenge_method":"S256","scope":scope})
 opener=urllib.request.build_opener(NoRedirect());locations=[]
 current=authorize
 for expected_host in (urllib.parse.urlsplit(axon_base).netloc,None,urllib.parse.urlsplit(axon_base).netloc):
  try:opener.open(current,timeout=4);raise RuntimeError("OAuth redirect chain ended early")
  except urllib.error.HTTPError as error:
   try:
    if error.code not in {302,303}:raise
    current=error.headers["location"];locations.append(current)
   finally:error.close()
   if expected_host and urllib.parse.urlsplit(current).netloc==expected_host and len(locations)==1:
    raise RuntimeError("Axon authorize did not redirect through configured provider")
 callback=urllib.parse.urlsplit(current);query=urllib.parse.parse_qs(callback.query)
 if callback.netloc!="127.0.0.1:65534" or query.get("state")!=[state] or "code" not in query:
  raise RuntimeError(f"Axon OAuth callback/state failed: {current}")
 form=urllib.parse.urlencode({"grant_type":"authorization_code","code":query["code"][0],"client_id":client_id,
  "redirect_uri":redirect_uri,"code_verifier":verifier}).encode()
 token_response=http.request(axon_base,None,http.HttpRequest("POST","/token",form),8,{"content-type":"application/x-www-form-urlencoded"})
 if token_response.status!=200:raise RuntimeError(f"Axon token exchange failed: {token_response.status} {token_response.body}")
 token=json.loads(token_response.body)
 if token.get("scope")!=scope or not token.get("access_token"):raise RuntimeError("Axon issued wrong OAuth scope")
 return {"scope":scope,"token":token["access_token"],"state_verified":True,"pkce_verified":True,"redirects":locations}
def oauth_negative_probes(axon_base):
 opener=urllib.request.build_opener(NoRedirect())
 def redirect(url):
  try:opener.open(url,timeout=4);raise RuntimeError("expected OAuth redirect")
  except urllib.error.HTTPError as error:
   try:
    if error.code not in {302,303}:raise
    return error.headers["location"]
   finally:error.close()
 def begin(label):
  callback="http://127.0.0.1:65534/callback"
  registered=http.request(axon_base,None,http.json_request("POST","/register",{"redirect_uris":[callback]}),8)
  client=json.loads(registered.body)["client_id"];verifier=f"negative-verifier-{label}"
  challenge=base64.urlsafe_b64encode(hashlib.sha256(verifier.encode()).digest()).decode().rstrip("=")
  authorize=axon_base+"/authorize?"+urllib.parse.urlencode({"response_type":"code","client_id":client,"redirect_uri":callback,
    "state":f"client-state-{label}","code_challenge":challenge,"code_challenge_method":"S256","scope":"axon:read"})
  provider=redirect(authorize);axon_callback=redirect(provider)
  return client,callback,verifier,axon_callback
 client,callback,_verifier,axon_callback=begin("state")
 parsed=urllib.parse.urlsplit(axon_callback);query=urllib.parse.parse_qs(parsed.query);query["state"]=["tampered-provider-state"]
 tampered=urllib.parse.urlunsplit((parsed.scheme,parsed.netloc,parsed.path,urllib.parse.urlencode(query,doseq=True),parsed.fragment))
 state_response=http.request(axon_base,None,http.HttpRequest("GET",tampered),8)
 if state_response.status not in {400,401,403,422}:raise RuntimeError("tampered OAuth provider state was accepted")
 client,callback,verifier,axon_callback=begin("pkce");client_redirect=redirect(axon_callback)
 code=urllib.parse.parse_qs(urllib.parse.urlsplit(client_redirect).query)["code"][0]
 form=urllib.parse.urlencode({"grant_type":"authorization_code","code":code,"client_id":client,"redirect_uri":callback,
   "code_verifier":verifier+"-wrong"}).encode()
 wrong=http.request(axon_base,None,http.HttpRequest("POST","/token",form),8,{"content-type":"application/x-www-form-urlencoded"})
 if wrong.status not in {400,401}:raise RuntimeError("wrong OAuth PKCE verifier was accepted")
 return {"state_tamper_status":state_response.status,"wrong_pkce_status":wrong.status}

def main():
 parser=argparse.ArgumentParser();parser.add_argument("--launcher-descriptor",type=Path,required=True);args=parser.parse_args()
 descriptor=json.loads(args.launcher_descriptor.read_text());run_id=descriptor["run_id"];run_root=Path(descriptor["run_root"])
 owned_root=run_root/"security";data=owned_root/"data";data.mkdir(parents=True,exist_ok=True)
 manifest=isolation.Manifest.create(owned_root/"manifests",run_id,data)
 manifest.register("data_dir",str(data))
 evidence=owned_root/"evidence";evidence.mkdir()
 manifest.register("output",str(evidence))
 reservation=isolation.allocate_port(owned_root/"leases",run_id,manifest);port=reservation.port;reservation.close()
 canary=json.loads((ROOT/"tests/e2e/fixtures/security/hostile.json").read_text())["canary"]
 env={**os.environ,**descriptor["environment"],"AXON_DATA_DIR":str(data),"AXON_SQLITE_PATH":str(data/"jobs.db"),
      "AXON_HTTP_HOST":"127.0.0.1","AXON_HTTP_PORT":str(port),"AXON_HTTP_TOKEN":canary,
      "AXON_ALLOWED_ORIGINS":"https://allowed.axon-e2e.invalid"}
 sandbox=owned_root/"network.sb";sandbox.write_text('(version 1)\n(allow default)\n(deny network-outbound)\n(allow network-outbound (remote ip "localhost:*"))\n')
 manifest.register("temp_path",str(sandbox))
 axon_argv=[descriptor["binary"],"mcp","--transport","http"]
 if Path("/usr/bin/sandbox-exec").is_file():axon_argv=["/usr/bin/sandbox-exec","-f",str(sandbox),*axon_argv]
 process=isolation.spawn_owned_process(manifest,owned_root,axon_argv,env=env,capture_prefix=evidence/"axon-http")
 provider_port_res=isolation.allocate_port(owned_root/"leases",run_id,manifest);provider_guard_port=provider_port_res.port;provider_port_res.close()
 provider_guard=isolation.spawn_owned_process(manifest,owned_root,[sys.executable,str(ROOT/"tests/e2e/fixtures/security/provider_boundary_double.py"),
   "--port",str(provider_guard_port)],capture_prefix=evidence/"provider-boundary")
 oauth_port_res=isolation.allocate_port(owned_root/"leases",run_id,manifest);oauth_port=oauth_port_res.port;oauth_port_res.close()
 oauth_process=isolation.spawn_owned_process(manifest,owned_root,[sys.executable,str(ROOT/"tests/e2e/fixtures/security/google_oidc_provider.py"),"--port",str(oauth_port)],capture_prefix=evidence/"google-oidc")
 base=f"http://127.0.0.1:{port}";wait(base+"/v1/status",process)
 wait(f"http://127.0.0.1:{provider_guard_port}/stats",provider_guard)
 wait(f"http://127.0.0.1:{oauth_port}/jwks",oauth_process)
 oauth_axon_res=isolation.allocate_port(owned_root/"leases",run_id,manifest);oauth_axon_port=oauth_axon_res.port;oauth_axon_res.close()
 oauth_data=owned_root/"oauth-data";oauth_data.mkdir();manifest.register("temp_path",str(oauth_data))
 oauth_axon_base=f"http://127.0.0.1:{oauth_axon_port}"
 oidc_base=f"http://127.0.0.1:{oauth_port}"
 oauth_axon_env={**env,"AXON_DATA_DIR":str(oauth_data),"AXON_SQLITE_PATH":str(oauth_data/"jobs.db"),
  "AXON_HTTP_PORT":str(oauth_axon_port),"AXON_AUTH_MODE":"oauth","AXON_PUBLIC_URL":oauth_axon_base,
  "AXON_GOOGLE_CLIENT_ID":"e2e-client","AXON_GOOGLE_CLIENT_SECRET":"e2e-secret",
  "AXON_AUTH_ADMIN_EMAIL":"operator-admin@example.invalid","AXON_ALLOWED_REDIRECT_URIS":"http://127.0.0.1:65534/callback",
  "AXON_GOOGLE_AUTHORIZE_ENDPOINT":oidc_base+"/authorize","AXON_GOOGLE_TOKEN_ENDPOINT":oidc_base+"/token",
  "AXON_GOOGLE_JWKS_ENDPOINT":oidc_base+"/jwks","AXON_GOOGLE_ISSUER":oidc_base}
 oauth_axon=isolation.spawn_owned_process(manifest,owned_root,[descriptor["binary"],"mcp","--transport","http"],env=oauth_axon_env,capture_prefix=evidence/"axon-oauth")
 wait(oauth_axon_base+"/v1/status",oauth_axon)
 subprocess.run(["sqlite3",str(oauth_data/"auth.db"),
   "INSERT INTO allowed_users(email,added_by,created_at) VALUES('e2e@example.invalid','e2e-fixture',unixepoch())"],
   check=True,capture_output=True,text=True)
 report_path=evidence/"security-report.json";result=None
 try:
  probes=http.run_probes(base,canary,8)
  required_probe_ids={"auth.valid","auth.missing","auth.invalid","auth.query_token","auth.conflicting",
    "error.malformed_json","error.unknown_id","error.conflict","error.oversize","error.traversal","error.hostile_headers"}
  if not all(item["passed"] for item in probes if item["id"] in required_probe_ids):
   raise RuntimeError(f"real HTTP security probes failed: {probes}")
  conflict=http.request(base,None,http.HttpRequest("GET","/v1/status"),8,
    {"Authorization":f"Bearer {canary}","x-api-key":"invalid-e2e-token"})
  conflict_code=error_code(conflict)
  if (conflict.status,conflict_code)!=(400,"auth.conflicting_credentials"):
   raise RuntimeError(f"conflicting credential contract drifted: HTTP {conflict.status} {conflict_code}")
  # Forwarded host is deliberately ignored in favor of the validated Host
  # header. An unlisted Origin may receive a response, but never CORS authority.
  forwarded=http.request(base,None,http.HttpRequest("GET","/v1/status"),8,
    {"Authorization":f"Bearer {canary}","Forwarded":"host=evil.invalid;proto=https","X-Forwarded-Host":"evil.invalid"})
  origin=http.request(base,None,http.HttpRequest("GET","/v1/status"),8,
    {"Authorization":f"Bearer {canary}","Origin":"https://evil.invalid"})
  if forwarded.status != 200 or origin.headers.get("Access-Control-Allow-Origin") is not None:
   raise RuntimeError("forwarded-host/origin policy did not fail closed")
  nonloop_env={**env,"AXON_HTTP_HOST":"0.0.0.0"}
  for key in ("AXON_HTTP_TOKEN","AXON_AUTH_MODE","AXON_GOOGLE_CLIENT_ID","AXON_GOOGLE_CLIENT_SECRET"):
   nonloop_env.pop(key,None)
  nonloop_run=subprocess.run([descriptor["binary"],"serve"],env=nonloop_env,capture_output=True,text=True,timeout=8)
  assert_clean_capture("nonloop stdout/stderr",nonloop_run.stdout+nonloop_run.stderr,[canary])
  nonloop_output=(nonloop_run.stdout+nonloop_run.stderr).casefold()
  nonloop={"id":"auth.non_loopback_bind","passed":nonloop_run.returncode!=0 and
           ("auth" in nonloop_output or "token" in nonloop_output),"exit_code":nonloop_run.returncode}
  if not nonloop["passed"]:raise RuntimeError(f"non-loopback tokenless bind accepted: {nonloop}")
  mcp=mcp_auth.matrix(base+"/mcp",canary,None,"https://allowed.axon-e2e.invalid")
  if not mcp["success"]:raise RuntimeError(f"real MCP HTTP auth matrix failed: {mcp['failures']}")
  oauth_base=f"http://127.0.0.1:{oauth_port}";oauth=[]
  axon_oauth=[axon_oauth_flow(oauth_axon_base,"axon:read"),axon_oauth_flow(oauth_axon_base,"axon:write")]
  issued_tokens=[item["token"] for item in axon_oauth]
  oauth_negative=oauth_negative_probes(oauth_axon_base)
  for issued in axon_oauth:
   status=http.request(oauth_axon_base,issued["token"],http.HttpRequest("GET","/v1/status"),8)
   if status.status!=200:raise RuntimeError(f"Axon rejected its own OAuth token: {status.status}")
  insufficient=http.request(oauth_axon_base,axon_oauth[0]["token"],http.json_request("POST","/v1/prune/exec",{}),8)
  if insufficient.status!=403:raise RuntimeError(f"read token reached admin destructive route: {insufficient.status}")
  oauth_mcp=mcp_auth.matrix(oauth_axon_base+"/mcp",axon_oauth[1]["token"],axon_oauth[0]["token"],None)
  if not oauth_mcp["success"]:raise RuntimeError(f"issued OAuth token failed MCP: {oauth_mcp['failures']}")
  bad_redirect=http.request(oauth_axon_base,None,http.json_request("POST","/register",{"redirect_uris":["https://evil.invalid/cb"]}),8)
  if bad_redirect.status not in {400,422}:raise RuntimeError("Axon OAuth redirect allowlist bypass")
  # Every encoded canary crosses the real HTTP boundary as hostile metadata;
  # none may be reflected into the response or retained evidence.
  transformed_auth=[]
  for encoding,value in security.transformations(canary).items():
   response=http.request(base,canary,http.HttpRequest("GET","/v1/status"),8,{"X-Axon-E2E-Canary":value})
   if response.status != 200 or value.encode() in response.body:raise RuntimeError("transformed canary was rejected or reflected")
   transformed_auth.append({"encoding":encoding,"status":response.status})
  # Exercise auth on upload, artifact and destructive routes, plus production
  # validation for malformed IDs, oversized uploads and hostile content.
  route_auth=[]
  for path,method,body in (("/v1/uploads","POST",b"{}"),("/v1/artifacts/art_missing","GET",None),
      ("/v1/prune/exec","POST",b"{}"),("/v1/reset/exec","POST",b"{}")):
   for profile,headers in (("missing",{}),("invalid",{"Authorization":"Bearer invalid"})):
    response=http.request(base,None,http.HttpRequest(method,path,body),8,headers)
    if response.status not in {401,403}:raise RuntimeError(f"{path} {profile} auth accepted")
    route_auth.append({"path":path,"profile":profile,"status":response.status})
  hostile=json.loads((ROOT/"tests/e2e/fixtures/security/hostile.json").read_text())
  validation=[]
  for kind,identifiers in (("malformed_id",hostile["malformed_ids"]),("path_traversal",hostile["path_traversal"])):
   for identifier in identifiers:
    quoted=urllib.parse.quote(identifier,safe="")
    response=http.request(base,canary,http.HttpRequest("GET",f"/v1/artifacts/{quoted}/content"),8)
    code=error_code(response)
    if (response.status,code)!=(400,"artifact.invalid_id"):
     raise RuntimeError(f"{kind} contract drifted: HTTP {response.status} {code}")
    validation.append({"kind":kind,"status":response.status,"code":code})
  oversized_path=owned_root/"oversize-request.bin";manifest.register("temp_path",str(oversized_path))
  oversized=oversize_probe(base,canary,oversized_path,hostile["oversized_upload_bytes"])
  oversized_code=error_code(oversized)
  if (oversized.status,oversized_code)!=(413,"route.validation.invalid_body"):
   raise RuntimeError(f"oversized upload contract drifted: HTTP {oversized.status} {oversized_code}")
  hostile_response=http.request(base,canary,http.json_request("POST","/v1/query",
    {"query":hostile["hostile_content"],"collection":run_id,"limit":1}),8)
  if hostile_response.status!=200 or hostile["hostile_content"].encode() in hostile_response.body:
   raise RuntimeError("hostile content escaped contract or disclosed canary")
  artifact_payload=f"owned artifact retrieval proof for {run_id}\n".encode();artifact_sha=hashlib.sha256(artifact_payload).hexdigest()
  create=http.request(base,canary,http.json_request("POST","/v1/uploads",{"filename":f"{run_id}.txt","content_type":"text/plain",
    "size_bytes":len(artifact_payload),"purpose":"source_artifact","sha256":artifact_sha,"metadata":{"run_id":run_id}}),8)
  if create.status!=200:raise RuntimeError(f"owned upload create failed: {create.status} {create.body}")
  created=json.loads(create.body);upload_id=created["upload_id"]
  put=http.request(base,canary,http.HttpRequest("PUT",f"/v1/uploads/{upload_id}/content",artifact_payload),8,
    {"content-type":"text/plain","x-content-sha256":artifact_sha})
  if put.status!=200:raise RuntimeError(f"owned upload content failed: {put.status} {put.body}")
  complete=http.request(base,canary,http.json_request("POST",f"/v1/uploads/{upload_id}/complete",{"sha256":artifact_sha}),8)
  if complete.status!=200:raise RuntimeError(f"owned upload complete failed: {complete.status} {complete.body}")
  artifact_id=json.loads(complete.body)["artifact_id"]
  content=http.request(base,canary,http.HttpRequest("GET",f"/v1/artifacts/{artifact_id}/content"),8)
  if content.status!=200 or content.body!=artifact_payload or hashlib.sha256(content.body).hexdigest()!=artifact_sha:
   raise RuntimeError("completed artifact did not round-trip exact owned bytes")
  # Exercise Axon's real source handler for every alternate form that resolves
  # exclusively to the owned provider endpoint. The provider counter is the
  # zero-connection sentinel; /stats itself is excluded from its count.
  provider=descriptor["environment"]["AXON_OPENAI_BASE_URL"].removesuffix("/v1")
  provider_port=int(urllib.parse.urlsplit(provider).port)
  before=json.loads(urllib.request.urlopen(provider+"/stats").read())["calls"]
  actual_urls=[f"http://127.0.0.1:{provider_port}/corpus/atlas",f"http://2130706433:{provider_port}/corpus/atlas",
               f"http://0x7f000001:{provider_port}/corpus/atlas",f"http://0177.0.0.1:{provider_port}/corpus/atlas",
               f"http://user@127.0.0.1:{provider_port}/corpus/atlas",f"http://[::ffff:127.0.0.1]:{provider_port}/corpus/atlas"]
  simulations={"http://rebind.axon-e2e.invalid/x":f"http://127.0.0.1:{provider_guard_port}/rebind",
               "http://redirect.axon-e2e.invalid/x":f"http://127.0.0.1:{provider_guard_port}/redirect"}
  sink_before=json.loads(urllib.request.urlopen(f"http://127.0.0.1:{provider_guard_port}/stats").read())["sinks"]
  ssrf=[]
  for declared in security.SSRF_CASES:
   routed=simulations.get(declared,declared)
   ssrf.append({"url":declared,"routed_url":routed,**post_source(base,routed,canary)})
  if len(ssrf)!=len(security.SSRF_CASES):raise RuntimeError("SSRF execution matrix incomplete")
  after=json.loads(urllib.request.urlopen(provider+"/stats").read())["calls"]
  security.assert_zero_connections(before,after,"url.private_address")
  sink_after=json.loads(urllib.request.urlopen(f"http://127.0.0.1:{provider_guard_port}/stats").read())["sinks"]
  if sink_after!=sink_before:raise RuntimeError(f"Axon reached redirect/rebind sink: {sink_before} -> {sink_after}")
  # Production-shaped provider bypass requests are guarded before transport;
  # prove the provider's durable state remained byte-identical.
  qstate=run_root/"launcher/qdrant.json";qbefore=hashlib.sha256(qstate.read_bytes()).hexdigest()
  boundary_state_before=json.loads(urllib.request.urlopen(f"http://127.0.0.1:{provider_guard_port}/state").read())
  provider_before=json.loads(urllib.request.urlopen(provider+"/stats").read())["calls"]
  provider_cases=[];boundary_base=f"http://127.0.0.1:{provider_guard_port}"
  boundary_requests=(("qdrant","production","collection.delete","DELETE","/collections/production"),
      ("qdrant","production","alias.delete","POST","/aliases"),("qdrant","production","snapshot.delete","DELETE","/collections/production/snapshots/snap"),
      ("qdrant","production","admin.delete","POST","/cluster"),("qdrant","production","collection.list","GET","/collections/production"),
      ("chrome","operator-profile","profile.delete","DELETE","/profiles/operator-profile"),
      ("chrome","operator-session","session.close","DELETE","/sessions/operator-session"),
      ("chrome","global","admin.delete","POST","/admin"),("chrome","global","session.list","GET","/json"))
  for resource,identity,operation_name,method,path in boundary_requests:
   classification=security.provider_boundary(resource,identity,operation_name,run_id,None)
   if classification=="allowed":raise RuntimeError("provider bypass guard accepted non-owned resource")
   rejection=provider_request(boundary_base,method,path)
   provider_cases.append({"resource":resource,"operation":operation_name,"classification":classification,"response":rejection})
  active_classification=security.provider_boundary("chrome",f"axon_e2e_{run_id}_session","session.close",run_id,run_id,active=True)
  active_response=http.request(boundary_base,None,http.HttpRequest("DELETE","/active/session"),8)
  if active_classification!="provider.resource_active" or (active_response.status,error_code(active_response))!=(409,"provider.resource_active"):
   raise RuntimeError("active provider resource did not fail closed through the composed boundary")
  provider_cases.append({"resource":"chrome","operation":"session.close","classification":active_classification,
    "response":{"error":{"code":error_code(active_response)}}})
  provider_after=json.loads(urllib.request.urlopen(provider+"/stats").read())["calls"]
  boundary_state_after=json.loads(urllib.request.urlopen(f"http://127.0.0.1:{provider_guard_port}/state").read())
  if hashlib.sha256(qstate.read_bytes()).hexdigest()!=qbefore or provider_after!=provider_before:
   raise RuntimeError("provider-boundary guard contacted or changed Qdrant/Chrome")
  if boundary_state_after!=boundary_state_before:raise RuntimeError("rejected provider operations mutated durable provider state")
  # Stdio schema is exercised from the actual binary; HTTP/OpenAPI inventory is
  # loaded by the real adapter and must carry every security route used above.
  stdio=subprocess.run([descriptor["binary"],"mcp","--help"],env=env,capture_output=True,text=True,timeout=8)
  assert_clean_capture("MCP help stdout/stderr",stdio.stdout+stdio.stderr,[canary,*issued_tokens])
  if stdio.returncode or "transport" not in (stdio.stdout+stdio.stderr).casefold():raise RuntimeError("MCP stdio schema unavailable")
  stdio_transport=taskwire.Stdio(Path(descriptor["binary"]),env,evidence/"mcp-stdio.stderr",str(manifest.path))
  try:
   taskwire.initialize(stdio_transport)
   message,_=stdio_transport.request(taskwire.rpc(44,"tools/call",{"name":"axon","arguments":{"action":"capabilities"}}))
   stdio_result=taskwire.result(message,"stdio capabilities")
   if "capabil" not in json.dumps(stdio_result).casefold():raise RuntimeError("MCP stdio tool result omitted capabilities")
  finally:stdio_transport.close()
  routes=set(http.inventory());required={"GET /v1/status","POST /v1/sources","POST /v1/prune/plan","POST /v1/prune/exec",
    "POST /v1/reset/plan","POST /v1/reset/exec","POST /v1/uploads","GET /v1/artifacts/{artifact_id}/content"}
  if not required<=routes:raise RuntimeError(f"generated OpenAPI auth routes missing: {required-routes}")
  report={"schema_version":1,"passed":True,"real_http_probes":probes,"nonloopback":nonloop,"mcp_http":mcp,
          "oauth":{"fixture_flows":2,"axon_flows":2,"mcp_cases":len(oauth_mcp["cases"]),"negative_probes_passed":True,"passed":True},"oauth_insufficient_scope":insufficient.status,"transformed_canary_auth":transformed_auth,"route_auth":route_auth,"validation":validation,
          "oversized_upload":{"status":oversized.status,"code":oversized_code},"hostile_status":hostile_response.status,
          "artifact_canary":{"upload_id":upload_id,"artifact_id":artifact_id,"sha256":artifact_sha,"bytes":len(artifact_payload)},
          "mcp_stdio_schema":True,"ssrf":ssrf,"ssrf_sentinel":{"before":before,"after":after,"sink_before":sink_before,"sink_after":sink_after},
          "provider_boundary":provider_cases,"provider_state":{"before":boundary_state_before,"after":boundary_state_after},"provider_zero_calls":{"before":provider_before,"after":provider_after},
          "manifest":str(manifest.path)}
  # OAuth credentials remain runtime-only; the report retains counts and
  # pass/fail metadata, and scan_tree verifies no credential reached evidence.
  report_path.write_text(json.dumps(report,indent=2,sort_keys=True)+"\n")
  security.scan_tree(evidence,[canary,*issued_tokens])
  handoff=owned_root/"teardown-handoff.json";teardown_report=run_root/"launcher/security-teardown-report.json"
  handoff.write_text(json.dumps({"manifest":str(manifest.path),"report":str(teardown_report)})+"\n")
  manifest.register("temp_path",str(handoff));result={"result":"pass","manifest":str(manifest.path),"handoff":str(handoff)}
 finally:
  teardown_report=run_root/"launcher/security-teardown-report.json"
  cleaned=subprocess.run([sys.executable,str(ROOT/"scripts/e2e/lib/teardown.py"),str(manifest.path),
                          "--report",str(teardown_report)],cwd=ROOT,capture_output=True,text=True,timeout=30)
  assert_clean_capture("teardown stdout/stderr",cleaned.stdout+cleaned.stderr,[canary,*locals().get("issued_tokens",[])])
  if cleaned.returncode:raise RuntimeError(f".15 teardown handoff failed: {teardown_report.read_text()}")
  teardown_evidence=json.loads(teardown_report.read_text())
  if teardown_evidence.get("success") is not True or teardown_evidence.get("residual") or teardown_evidence.get("refused"):
   raise RuntimeError("authoritative .15 teardown did not prove zero residual state")
 if result is None:raise RuntimeError("security entry produced no result")
 result["teardown_report"]=str(teardown_report);result["teardown_sha256"]=hashlib.sha256(teardown_report.read_bytes()).hexdigest()
 result["teardown_zero_residual"]=True;print(json.dumps(result));return 0
if __name__=="__main__":raise SystemExit(main())
