#!/usr/bin/env python3
from __future__ import annotations
import argparse,datetime as dt,json,os,urllib.request
parser=argparse.ArgumentParser();parser.parse_args()
allowed={"push","schedule","workflow_dispatch"};event=os.environ.get("GITHUB_EVENT_NAME")
if os.environ.get("GITHUB_REPOSITORY")!="dinglebear-ai/axon" or event not in allowed or os.environ.get("GITHUB_REF")!="refs/heads/main":raise SystemExit("untrusted live E2E invocation")
if os.environ.get("GITHUB_HEAD_REF") or "/pull/" in os.environ.get("GITHUB_REF",""):raise SystemExit("pull/merge refs are forbidden")
current=int(os.environ.get("GITHUB_RUN_ID","0"));headers={"Accept":"application/vnd.github+json","User-Agent":"axon-e2e-live-admission"}
try:
 with urllib.request.urlopen(urllib.request.Request(f"https://api.github.com/repos/dinglebear-ai/axon/actions/runs/{current}",headers=headers),timeout=15) as response:created=json.load(response)["created_at"]
except Exception as error:raise SystemExit("live E2E public run provenance unavailable") from error
if created:
 age=(dt.datetime.now(dt.timezone.utc)-dt.datetime.fromisoformat(created.replace("Z","+00:00"))).total_seconds()
 if age>21600:raise SystemExit("live E2E queue age exceeded")
priorities={"schedule":1,"push":2,"workflow_dispatch":3};current_priority=priorities[event]
try:
 with urllib.request.urlopen(urllib.request.Request("https://api.github.com/repos/dinglebear-ai/axon/actions/workflows/e2e-live.yml/runs?branch=main&per_page=50",headers=headers),timeout=15) as response:runs=json.load(response)["workflow_runs"]
except Exception as error:raise SystemExit("live E2E public queue governance unavailable") from error
for run in runs:
 if run.get("id")==current or run.get("status") not in {"queued","in_progress"}:continue
 if priorities.get(run.get("event"),0)>current_priority or (priorities.get(run.get("event"),0)==current_priority and run.get("created_at","")>created):raise SystemExit("live E2E safely coalesced before provider mutation")
