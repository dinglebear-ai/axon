#!/usr/bin/env python3
"""Operator policy oracle for exact GitHub OIDC claims; no token is logged."""
from __future__ import annotations
import argparse,json,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
class ClaimError(RuntimeError):pass
def validate(claims,policy,audience,tag,seen_in_evaluation,now=None):
 now=int(time.time() if now is None else now);expected={"iss":policy["issuer"],"aud":audience,"repository_owner":policy["repository_owner"],"repository_owner_id":policy["repository_owner_id"],"repository":policy["repository"],"repository_id":policy["repository_id"],"job_workflow_ref":policy["job_workflow_ref"],"ref":policy["refs"][0],"environment":policy["environment"],"sub":policy["subject"]}
 for key,value in expected.items():
  if claims.get(key)!=value:raise ClaimError(f"WIF {key} mismatch")
 if claims.get("event_name") not in policy["events"]:raise ClaimError("WIF event mismatch")
 if tag not in policy["scope"]["tags"]:raise ClaimError("WIF tag escalation")
 for key in ("iat","nbf","exp"):
  if not isinstance(claims.get(key),int):raise ClaimError(f"WIF {key} absent")
 if claims["iat"]>now+policy["clock_skew_seconds"] or claims["nbf"]>now+policy["clock_skew_seconds"] or claims["exp"]<now-policy["clock_skew_seconds"]:raise ClaimError("WIF token time invalid")
 if claims["exp"]-claims["iat"]>policy["token_max_lifetime_seconds"] or now-claims["iat"]>policy["token_max_age_seconds"]:raise ClaimError("WIF token lifetime/age exceeded")
 jti=claims.get("jti")
 if not isinstance(jti,str) or len(jti)<16 or jti in seen_in_evaluation:raise ClaimError("WIF token replay/identity invalid")
 seen_in_evaluation.add(jti);return True
def main():
 p=argparse.ArgumentParser();p.add_argument("claims",type=Path);p.add_argument("--audience",required=True);p.add_argument("--tag",default="tag:axon-ci-e2e");a=p.parse_args();validate(json.loads(a.claims.read_text()),json.loads((ROOT/"config/tailscale/axon-ci-wif.json").read_text()),a.audience,a.tag,set());return 0
if __name__=="__main__":raise SystemExit(main())
