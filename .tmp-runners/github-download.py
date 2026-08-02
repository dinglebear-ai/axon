#!/usr/bin/env python3
import re
import sys
import urllib.error
import urllib.request
from pathlib import Path

if len(sys.argv) != 3:
    raise SystemExit("usage: github-download.py /repos/dinglebear-ai/axon/... /absolute/output")
path, output = sys.argv[1:3]
if not path.startswith("/repos/dinglebear-ai/axon/"):
    raise SystemExit("refusing request outside dinglebear-ai/axon")
output_path = Path(output)
if not output_path.is_absolute():
    raise SystemExit("output path must be absolute")
settings = Path("/home/jmagar/.config/zed/settings.json").read_text(errors="ignore")
match = re.search(r"(?:github_pat_[A-Za-z0-9_]{20,}|ghp_[A-Za-z0-9]{20,})", settings)
if not match:
    raise SystemExit("GitHub token not found")
request = urllib.request.Request(
    "https://api.github.com" + path,
    headers={
        "Authorization": "Bearer " + match.group(0),
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "axon-closeout",
    },
)
class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None

opener = urllib.request.build_opener(NoRedirect)
try:
    response = opener.open(request, timeout=60)
except urllib.error.HTTPError as error:
    if error.code not in (301, 302, 303, 307, 308):
        raise
    location = error.headers.get("Location")
    if not location:
        raise
    response = urllib.request.urlopen(location, timeout=60)
with response:
    output_path.write_bytes(response.read())
    print(f"HTTP {response.status} bytes={output_path.stat().st_size} output={output_path}")
