#!/usr/bin/env python3
"""Fail-closed identity and enforcement preflight for live provider gateways."""
from __future__ import annotations
import argparse,hashlib,ipaddress,json,os,subprocess,urllib.error,urllib.parse,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
class PreflightError(RuntimeError):pass
def load_config(path=ROOT/"config/e2e/live-services.json"):return json.loads(path.read_text())
def tailscale_peers():
 result=subprocess.run(["tailscale","status","--json"],capture_output=True,text=True,check=True);body=json.loads(result.stdout);return list(body.get("Peer",{}).values())
def validate_provider(item,env=os.environ,fetch=None,ping=None,peers=None):
 url=env.get(item["url_env"],"");peer=env.get(item["peer_env"],"");token=env.get(item["auth_env"],"")
 parsed=urllib.parse.urlparse(url)
 if parsed.scheme!="https" or parsed.port not in (None,item["port"]) or parsed.username or parsed.password or parsed.query or parsed.fragment:raise PreflightError(f"{item['name']}: exact HTTPS gateway required")
 if not peer or parsed.hostname!=peer or not peer.endswith(".ts.net") or peer in {"localhost","localhost.localdomain"}:raise PreflightError(f"{item['name']}: URL/peer identity mismatch")
 if not token:raise PreflightError(f"{item['name']}: application bearer auth required")
 (ping or (lambda host:subprocess.run(["tailscale","ping","--timeout=10s",host],capture_output=True,text=True,check=True)))(peer)
 peers=tailscale_peers() if peers is None else peers;matches=[]
 for node in peers:
  dns=str(node.get("DNSName","")).rstrip(".");ips=node.get("TailscaleIPs",[])
  valid_ips=bool(ips) and all(ipaddress.ip_address(value) in ipaddress.ip_network("100.64.0.0/10") or ipaddress.ip_address(value) in ipaddress.ip_network("fd7a:115c:a1e0::/48") for value in ips)
  if dns==peer and item["tag"] in node.get("Tags",[]) and node.get("Online") is True and valid_ips:matches.append(node)
 if len(matches)!=1:raise PreflightError(f"{item['name']}: exact Tailscale node/tag/IP identity unavailable")
 if fetch is None:
  request=urllib.request.Request(url.rstrip("/")+"/v1/e2e/identity",headers={"Authorization":f"Bearer {token}"})
  fetch=lambda _item:json.load(urllib.request.urlopen(request,timeout=10))
 body=fetch(item);required={"schema","service","peer","tag","enforcement","lease_api","application_auth","version"}
 if set(body)!=required or body.get("schema")!=1 or (body.get("service"),body.get("peer"),body.get("tag"))!=(item["name"],peer,item["tag"]):raise PreflightError(f"{item['name']}: provider identity mismatch")
 if body.get("enforcement")!="disposable-tenant-proxy" or body.get("lease_api") is not True or body.get("application_auth")!="bearer-required":raise PreflightError(f"{item['name']}: raw/shared provider access forbidden")
 # Retained CI evidence proves the exact identity check occurred without
 # publishing private MagicDNS names or tailnet tags.
 identity=f"{peer}\0{item['tag']}\0{body['version']}".encode()
 return {"service":item["name"],"identity_sha256":hashlib.sha256(identity).hexdigest(),"enforcement":body["enforcement"]}
def main():
 p=argparse.ArgumentParser();p.add_argument("--out",type=Path,required=True);a=p.parse_args();a.out.parent.mkdir(parents=True,exist_ok=True)
 try:
  config=load_config();declared={os.environ.get(item["peer_env"],"") for item in config["providers"]};expected={value.strip() for value in os.environ.get("AXON_E2E_EXPECTED_PEERS","").split(",") if value.strip()}
  if declared!=expected or len(declared)!=4:raise PreflightError("expected peer set does not exactly match provider peers")
  peers=tailscale_peers();results=[validate_provider(item,peers=peers) for item in config["providers"]];body={"schema":1,"status":"passed","classification":None,"providers":results,"sanitized":True};code=0
 except urllib.error.HTTPError as error:
  try:body={"schema":1,"status":"failed","classification":"auth" if error.code in (401,403) else "provider","error":"provider HTTP preflight failed","sanitized":True};code=2
  finally:error.close()
 except (urllib.error.URLError,subprocess.SubprocessError,OSError) as error:body={"schema":1,"status":"failed","classification":"network","error":type(error).__name__,"sanitized":True};code=2
 except (PreflightError,ValueError,json.JSONDecodeError) as error:body={"schema":1,"status":"failed","classification":"provider","error":str(error),"sanitized":True};code=2
 a.out.write_text(json.dumps(body,indent=2,sort_keys=True)+"\n");return code
if __name__=="__main__":raise SystemExit(main())
