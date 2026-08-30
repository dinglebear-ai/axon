#!/usr/bin/env python3
"""Request, locally validate, and discard a GitHub OIDC token before discovery."""
from __future__ import annotations
import argparse,base64,importlib.util,json,os,urllib.parse,urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location("axon_wif_attest_validator",ROOT/"scripts/e2e/validate-wif-claims.py");validator=importlib.util.module_from_spec(spec);spec.loader.exec_module(validator)
def main():
 p=argparse.ArgumentParser();p.add_argument("--audience",required=True);p.add_argument("--out",type=Path,required=True);a=p.parse_args()
 request_url=os.environ.get("ACTIONS_ID_TOKEN_REQUEST_URL");request_token=os.environ.get("ACTIONS_ID_TOKEN_REQUEST_TOKEN")
 if not request_url or not request_token:raise SystemExit("GitHub OIDC request capability unavailable")
 separator="&" if "?" in request_url else "?";request=urllib.request.Request(request_url+separator+urllib.parse.urlencode({"audience":a.audience}),headers={"Authorization":f"Bearer {request_token}"})
 with urllib.request.urlopen(request,timeout=15) as response:token=json.load(response).get("value","")
 parts=token.split(".")
 if len(parts)!=3:raise SystemExit("GitHub OIDC token is malformed")
 claims=json.loads(base64.urlsafe_b64decode(parts[1]+"="*(-len(parts[1])%4)));policy=json.loads((ROOT/"config/tailscale/axon-ci-wif.json").read_text())
 validator.validate(claims,policy,a.audience,"tag:axon-ci-e2e",set())
 a.out.parent.mkdir(parents=True,exist_ok=True);a.out.write_text(json.dumps({"schema":1,"status":"passed","issuer":claims["iss"],"audience_sha256":__import__("hashlib").sha256(a.audience.encode()).hexdigest(),"repository_id":claims["repository_id"],"workflow":claims["job_workflow_ref"],"ref":claims["ref"],"event":claims["event_name"],"environment":claims["environment"],"token_discarded":True},indent=2,sort_keys=True)+"\n");return 0
if __name__=="__main__":raise SystemExit(main())
