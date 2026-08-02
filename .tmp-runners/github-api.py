#!/usr/bin/env python3
import json
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

if len(sys.argv) not in (3, 4):
    raise SystemExit("usage: github-api.py METHOD /repos/dinglebear-ai/axon/... [JSON]")
method, path = sys.argv[1:3]
if not path.startswith("/repos/dinglebear-ai/axon/"):
    raise SystemExit("refusing request outside dinglebear-ai/axon")
settings = Path("/home/jmagar/.config/zed/settings.json").read_text(errors="ignore")
match = re.search(r"(?:github_pat_[A-Za-z0-9_]{20,}|ghp_[A-Za-z0-9]{20,})", settings)
if not match:
    raise SystemExit("GitHub token not found")
body = sys.argv[3].encode() if len(sys.argv) == 4 else None
headers = {
    "Authorization": "Bearer " + match.group(0),
    "Accept": "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
    "User-Agent": "axon-closeout",
}
if body is not None:
    headers["Content-Type"] = "application/json"
request = urllib.request.Request("https://api.github.com" + path, data=body, headers=headers, method=method)
try:
    with urllib.request.urlopen(request, timeout=30) as response:
        payload = response.read()
        print(f"HTTP {response.status}")
        if payload:
            try:
                print(json.dumps(json.loads(payload), indent=2))
            except json.JSONDecodeError:
                sys.stdout.buffer.write(payload)
except urllib.error.HTTPError as error:
    payload = error.read()
    print(f"HTTP {error.code}")
    if payload:
        try:
            print(json.dumps(json.loads(payload), indent=2))
        except json.JSONDecodeError:
            sys.stdout.buffer.write(payload)
    raise SystemExit(1)
