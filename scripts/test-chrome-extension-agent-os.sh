#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXT_DIR="$ROOT/apps/chrome-extension"
PORT="${AXON_AGENT_OS_STAGING_PORT:-8766}"
HOST_IP="${AXON_AGENT_OS_HOST_IP:-100.88.16.79}"
TARGET_URL="${AXON_AGENT_OS_TARGET_URL:-https://code.claude.com}"
EXPECTED_FINAL_URL="${AXON_AGENT_OS_EXPECTED_FINAL_URL:-https://claude.com/product/claude-code}"
COLLECTION_DB="${AXON_AGENT_OS_SQLITE_NAME:-regression-jobs.db}"

if ! command -v labby >/dev/null 2>&1; then
  echo "labby is required on PATH" >&2
  exit 1
fi

if [[ ! -f "$HOME/.axon/.env" ]]; then
  echo "Missing $HOME/.axon/.env; agent-os needs it for AXON_HTTP_TOKEN" >&2
  exit 1
fi

if [[ ! -f "$HOME/.axon/config.toml" ]]; then
  echo "Missing $HOME/.axon/config.toml" >&2
  exit 1
fi

cd "$ROOT"
node --check "$EXT_DIR/background.js" >/dev/null
"$EXT_DIR/package.sh" >/dev/null

version="$(node -e "console.log(require('$EXT_DIR/manifest.json').version)")"
zip="$EXT_DIR/dist/axon-$version.zip"
if [[ ! -f "$zip" ]]; then
  echo "Expected package not found: $zip" >&2
  exit 1
fi

stage="$(mktemp -d)"
template_file="$(mktemp)"
js_file="$(mktemp)"
server_pid=""
cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$stage" "$template_file" "$js_file"
}
trap cleanup EXIT

cp "$zip" "$stage/axon-extension.zip"
cp "$HOME/.axon/.env" "$stage/axon.env"
cp "$HOME/.axon/config.toml" "$stage/config.toml"

release_json="$(curl -fsSL -H "User-Agent: axon-agent-os-regression" https://api.github.com/repos/jmagar/axon/releases/latest)"
release_tag="$(node -e "const r=JSON.parse(process.argv[1]); console.log(r.tag_name)" "$release_json")"
cli_asset_url="$(node -e "const r=JSON.parse(process.argv[1]); const a=(r.assets||[]).find(x=>x.name==='axon-windows-x86_64.zip'); if(!a) process.exit(2); console.log(a.browser_download_url)" "$release_json")"
curl -fL --retry 3 --retry-delay 2 -o "$stage/axon-windows-x86_64.zip" "$cli_asset_url"
cli_extract="$(mktemp -d)"
unzip -q "$stage/axon-windows-x86_64.zip" -d "$cli_extract"
cli_exe="$(find "$cli_extract" -iname axon.exe -print -quit)"
if [[ -z "$cli_exe" ]]; then
  echo "axon.exe missing from latest CLI zip" >&2
  exit 1
fi
cp "$cli_exe" "$stage/axon.exe"
rm -rf "$cli_extract"

(
  cd "$stage"
  python3 -m http.server "$PORT" --bind 0.0.0.0 >/tmp/axon-agent-os-extension-regression-http.log 2>&1
) &
server_pid="$!"
sleep 1

cat >"$template_file" <<'EOF_JS'
async () => {
  const phase = __PHASE_JSON__;
  const targetUrl = __TARGET_URL_JSON__;
  const expectedFinalUrl = __EXPECTED_FINAL_URL_JSON__;
  const hostBase = __HOST_BASE_JSON__;
  const sqliteName = __SQLITE_NAME_JSON__;
  const releaseTag = __RELEASE_TAG_JSON__;

  const ps = async (command) => {
    const response = await codemode.agent_os_windows_mcp.PowerShell({ command });
    const text = typeof response === "string" ? response : JSON.stringify(response);
    if (/Status Code:\s*[1-9]\d*/.test(text)) {
      throw new Error(text);
    }
    return response;
  };

  if (phase === "install") {
    const install = await ps(`\$ErrorActionPreference='Stop'
\$ProgressPreference='SilentlyContinue'
\$homeDir=\$env:USERPROFILE
\$downloadDir=Join-Path \$homeDir 'axon-regression-downloads'
\$bin=Join-Path (Join-Path \$homeDir '.local') 'bin'
New-Item -ItemType Directory -Force -Path \$downloadDir,\$bin | Out-Null
\$axonExe=Join-Path \$downloadDir 'axon.exe'
Invoke-WebRequest -UseBasicParsing -Uri '${hostBase}/axon.exe' -OutFile \$axonExe
Get-Process axon -ErrorAction SilentlyContinue | Stop-Process -Force
\$destAxon=Join-Path \$bin 'axon.exe'
for (\$i = 0; \$i -lt 10; \$i++) {
  try {
    Copy-Item -Force \$axonExe \$destAxon
    break
  } catch {
    if (\$i -eq 9) { throw }
    Start-Sleep -Milliseconds 500
  }
}
\$version=& \$destAxon --version
[pscustomobject]@{ release='${releaseTag}'; axon_version=\$version; path=\$destAxon } | ConvertTo-Json -Depth 5`);
    return { install };
  }

  if (phase === "configure") {
    const configure = await ps(`\$ErrorActionPreference='Stop'
\$ProgressPreference='SilentlyContinue'
\$homeDir=\$env:USERPROFILE
\$downloadDir=Join-Path \$homeDir 'axon-regression-downloads'
\$extDir=Join-Path \$homeDir 'axon-extension-current'
\$axonHome=Join-Path \$homeDir '.axon'
New-Item -ItemType Directory -Force -Path \$downloadDir,\$extDir,\$axonHome | Out-Null
Invoke-WebRequest -UseBasicParsing -Uri '${hostBase}/axon.env' -OutFile (Join-Path \$axonHome '.env')
Invoke-WebRequest -UseBasicParsing -Uri '${hostBase}/config.toml' -OutFile (Join-Path \$axonHome 'config.toml')
\$extZip=Join-Path \$downloadDir 'axon-extension.zip'
Invoke-WebRequest -UseBasicParsing -Uri '${hostBase}/axon-extension.zip' -OutFile \$extZip
if (Test-Path \$extDir) { Remove-Item -Recurse -Force \$extDir }
New-Item -ItemType Directory -Force -Path \$extDir | Out-Null
Expand-Archive -Path \$extZip -DestinationPath \$extDir -Force
Get-Content -Raw (Join-Path \$extDir 'manifest.json')`);
    return { configure };
  }

  if (phase === "start-server") {
    const startServer = await ps(`\$ErrorActionPreference='Stop'
\$ProgressPreference='SilentlyContinue'
\$homeDir=\$env:USERPROFILE
\$bin=Join-Path (Join-Path \$homeDir '.local') 'bin'
\$env:HOME=\$homeDir
\$env:Path="\$bin;\$env:Path"
\$env:AXON_SQLITE_PATH=Join-Path (Join-Path \$homeDir '.axon') '${sqliteName}'
\$env:QDRANT_URL='http://127.0.0.1:1'
\$env:TEI_URL='http://100.88.16.79:52000'
\$env:AXON_CHROME_REMOTE_URL=''
\$launcher=Join-Path \$homeDir 'axon-regression-serve.ps1'
\$axonPath=Join-Path \$bin 'axon.exe'
\$sqlitePath=Join-Path (Join-Path \$homeDir '.axon') '${sqliteName}'
Remove-Item -Force \$sqlitePath -ErrorAction SilentlyContinue
\$scriptLines=@(
  ('\$env:HOME = ' + "'\$homeDir'"),
  ('\$env:Path = ' + "'\$bin;'" + ' + \$env:Path'),
  ('\$env:AXON_SQLITE_PATH = ' + "'\$sqlitePath'"),
  "\`$env:QDRANT_URL = 'http://127.0.0.1:1'",
  "\`$env:TEI_URL = 'http://100.88.16.79:52000'",
  "\`$env:AXON_CHROME_REMOTE_URL = ''",
  ('& ' + "'\$axonPath'" + ' serve mcp')
)
Set-Content -Path \$launcher -Value \$scriptLines -Encoding UTF8
Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',\$launcher) -WindowStyle Hidden
Start-Sleep -Seconds 1
\$process=Get-Process axon -ErrorAction SilentlyContinue | Select-Object -First 1
[pscustomobject]@{ process=\$process.Id; launcher=\$launcher } | ConvertTo-Json -Depth 5`);
    return { startServer };
  }

  if (phase === "wait-server") {
    const waitServer = await ps(`\$ErrorActionPreference='Stop'
\$ProgressPreference='SilentlyContinue'
\$homeDir=\$env:USERPROFILE
\$bin=Join-Path (Join-Path \$homeDir '.local') 'bin'
for (\$i = 0; \$i -lt 45; \$i++) {
  try {
    \$health=(Invoke-WebRequest -UseBasicParsing -Uri 'http://127.0.0.1:8001/healthz' -TimeoutSec 2).Content
    break
  } catch {
    if (\$i -eq 44) { throw }
    Start-Sleep -Seconds 1
  }
}
\$version=& (Join-Path \$bin 'axon.exe') --version
[pscustomobject]@{ health=\$health; axon_version=\$version; process=(Get-Process axon | Select-Object -First 1 -ExpandProperty Id) } | ConvertTo-Json -Depth 5`);
    return { waitServer };
  }

  if (phase === "start-worker") {
    const startWorker = await ps(`\$ErrorActionPreference='Stop'
\$ProgressPreference='SilentlyContinue'
\$homeDir=\$env:USERPROFILE
\$bin=Join-Path (Join-Path \$homeDir '.local') 'bin'
\$launcher=Join-Path \$homeDir 'axon-regression-crawl-worker.ps1'
\$axonPath=Join-Path \$bin 'axon.exe'
\$sqlitePath=Join-Path (Join-Path \$homeDir '.axon') '${sqliteName}'
\$scriptLines=@(
  ('\$env:HOME = ' + "'\$homeDir'"),
  ('\$env:Path = ' + "'\$bin;'" + ' + \$env:Path'),
  ('\$env:AXON_SQLITE_PATH = ' + "'\$sqlitePath'"),
  "\`$env:QDRANT_URL = 'http://127.0.0.1:1'",
  "\`$env:TEI_URL = 'http://100.88.16.79:52000'",
  "\`$env:AXON_CHROME_REMOTE_URL = ''",
  ('& ' + "'\$axonPath'" + ' crawl worker')
)
Set-Content -Path \$launcher -Value \$scriptLines -Encoding UTF8
Start-Process -FilePath 'powershell.exe' -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',\$launcher) -WindowStyle Hidden
Start-Sleep -Seconds 1
[pscustomobject]@{ launcher=\$launcher; worker_processes=(Get-Process axon -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id) } | ConvertTo-Json -Depth 5`);
    return { startWorker };
  }

  if (phase === "start-chrome") {
    const startChrome = await ps(`\$ErrorActionPreference='Stop'
\$ProgressPreference='SilentlyContinue'
\$homeDir=\$env:USERPROFILE
cmd.exe /c "taskkill /F /IM chrome.exe /T >NUL 2>NUL"
\$chrome='C:/Program Files/Google/Chrome/Application/chrome.exe'
\$profile=Join-Path \$homeDir 'axon-chrome-extension-regression-profile'
\$log=Join-Path \$homeDir 'axon-chrome-extension-regression.log'
\$args=@("--user-data-dir=\$profile",'--no-first-run','--no-default-browser-check','--remote-debugging-address=127.0.0.1','--remote-debugging-port=9222','--enable-unsafe-extension-debugging','--enable-logging','--v=1',"--log-file=\$log", '${targetUrl}')
Start-Process -FilePath \$chrome -ArgumentList \$args
for (\$i = 0; \$i -lt 20; \$i++) {
  try {
    \$version=(Invoke-WebRequest -UseBasicParsing -Uri 'http://127.0.0.1:9222/json/version' -TimeoutSec 2).Content
    break
  } catch {
    if (\$i -eq 19) { throw }
    Start-Sleep -Seconds 1
  }
}
[pscustomobject]@{ chrome_cdp=\$version } | ConvertTo-Json -Depth 5`);
    return { startChrome };
  }

  if (phase === "launch") {
    const launch = await ps(`\$ErrorActionPreference='Stop'
\$ProgressPreference='SilentlyContinue'
\$homeDir=\$env:USERPROFILE
\$bin=Join-Path (Join-Path \$homeDir '.local') 'bin'
\$env:HOME=\$homeDir
\$env:Path="\$bin;\$env:Path"
\$env:AXON_SQLITE_PATH=Join-Path (Join-Path \$homeDir '.axon') '${sqliteName}'
\$env:QDRANT_URL='http://127.0.0.1:1'
\$env:TEI_URL='http://100.88.16.79:52000'
\$env:AXON_CHROME_REMOTE_URL=''
Get-Process axon -ErrorAction SilentlyContinue | Stop-Process -Force
\$out=Join-Path \$homeDir 'axon-regression-serve.log'
\$err=Join-Path \$homeDir 'axon-regression-serve.err.log'
Remove-Item -Force \$out,\$err -ErrorAction SilentlyContinue
Start-Process -FilePath (Join-Path \$bin 'axon.exe') -ArgumentList @('serve','mcp') -RedirectStandardOutput \$out -RedirectStandardError \$err -WindowStyle Hidden
Start-Sleep -Seconds 5
\$health=(Invoke-WebRequest -UseBasicParsing -Uri 'http://127.0.0.1:8001/healthz' -TimeoutSec 5).Content
Get-Process chrome -ErrorAction SilentlyContinue | Stop-Process -Force
\$chrome='C:/Program Files/Google/Chrome/Application/chrome.exe'
\$profile=Join-Path \$homeDir 'axon-chrome-extension-regression-profile'
\$log=Join-Path \$homeDir 'axon-chrome-extension-regression.log'
\$args=@("--user-data-dir=\$profile",'--no-first-run','--no-default-browser-check','--remote-debugging-address=127.0.0.1','--remote-debugging-port=9222','--enable-unsafe-extension-debugging','--enable-logging','--v=1',"--log-file=\$log", '${targetUrl}')
Start-Process -FilePath \$chrome -ArgumentList \$args
Start-Sleep -Seconds 5
\$version=& (Join-Path \$bin 'axon.exe') --version
[pscustomobject]@{ release='${releaseTag}'; health=\$health; axon_version=\$version } | ConvertTo-Json -Depth 5`);
    return { launch };
  }

  if (phase === "setup") {
  const setup = await ps(`\$ErrorActionPreference='Stop'
\$ProgressPreference='SilentlyContinue'
\$homeDir=\$env:USERPROFILE
\$downloadDir=Join-Path \$homeDir 'axon-regression-downloads'
\$extDir=Join-Path \$homeDir 'axon-extension-current'
\$axonHome=Join-Path \$homeDir '.axon'
\$bin=Join-Path (Join-Path \$homeDir '.local') 'bin'
New-Item -ItemType Directory -Force -Path \$downloadDir,\$extDir,\$axonHome,\$bin | Out-Null

\$axonExe=Join-Path \$downloadDir 'axon.exe'
Invoke-WebRequest -UseBasicParsing -Uri '${hostBase}/axon.exe' -OutFile \$axonExe
Get-Process axon -ErrorAction SilentlyContinue | Stop-Process -Force
\$destAxon=Join-Path \$bin 'axon.exe'
for (\$i = 0; \$i -lt 10; \$i++) {
  try {
    Copy-Item -Force \$axonExe \$destAxon
    break
  } catch {
    if (\$i -eq 9) { throw }
    Start-Sleep -Milliseconds 500
  }
}

Invoke-WebRequest -UseBasicParsing -Uri '${hostBase}/axon.env' -OutFile (Join-Path \$axonHome '.env')
Invoke-WebRequest -UseBasicParsing -Uri '${hostBase}/config.toml' -OutFile (Join-Path \$axonHome 'config.toml')
\$extZip=Join-Path \$downloadDir 'axon-extension.zip'
Invoke-WebRequest -UseBasicParsing -Uri '${hostBase}/axon-extension.zip' -OutFile \$extZip
if (Test-Path \$extDir) { Remove-Item -Recurse -Force \$extDir }
New-Item -ItemType Directory -Force -Path \$extDir | Out-Null
Expand-Archive -Path \$extZip -DestinationPath \$extDir -Force

\$env:HOME=\$homeDir
\$env:Path="\$bin;\$env:Path"
\$env:AXON_SQLITE_PATH=Join-Path \$axonHome '${sqliteName}'
\$env:QDRANT_URL='http://127.0.0.1:1'
\$env:TEI_URL='http://100.88.16.79:52000'
\$env:AXON_CHROME_REMOTE_URL=''
\$out=Join-Path \$homeDir 'axon-regression-serve.log'
\$err=Join-Path \$homeDir 'axon-regression-serve.err.log'
Remove-Item -Force \$out,\$err -ErrorAction SilentlyContinue
Start-Process -FilePath (Join-Path \$bin 'axon.exe') -ArgumentList @('serve','mcp') -RedirectStandardOutput \$out -RedirectStandardError \$err -WindowStyle Hidden
Start-Sleep -Seconds 5
\$health=(Invoke-WebRequest -UseBasicParsing -Uri 'http://127.0.0.1:8001/healthz' -TimeoutSec 5).Content

Get-Process chrome -ErrorAction SilentlyContinue | Stop-Process -Force
\$chrome='C:/Program Files/Google/Chrome/Application/chrome.exe'
\$profile=Join-Path \$homeDir 'axon-chrome-extension-regression-profile'
\$log=Join-Path \$homeDir 'axon-chrome-extension-regression.log'
\$args=@("--user-data-dir=\$profile",'--no-first-run','--no-default-browser-check','--remote-debugging-address=127.0.0.1','--remote-debugging-port=9222','--enable-unsafe-extension-debugging','--enable-logging','--v=1',"--log-file=\$log", '${targetUrl}')
Start-Process -FilePath \$chrome -ArgumentList \$args
Start-Sleep -Seconds 5
\$version=& (Join-Path \$bin 'axon.exe') --version
[pscustomobject]@{ release='${releaseTag}'; health=\$health; axon_version=\$version; extension=(Get-Content -Raw (Join-Path \$extDir 'manifest.json')) } | ConvertTo-Json -Depth 5`);
    return { setup };
  }

  if (phase === "automation") {
  const automation = await ps(`\$ErrorActionPreference='Stop'
\$script = @'
async function main() {
const fs = require('node:fs');
const home = process.env.USERPROFILE;
const extDir = home + '/axon-extension-current';
const extIdExpectedUrl = ${JSON.stringify(expectedFinalUrl)};
const tokenLine = fs.readFileSync(home + '/.axon/.env', 'utf8').split(/\\r?\\n/).find((line) => /^\\s*AXON_HTTP_TOKEN\\s*=/.test(line));
if (!tokenLine) throw new Error('AXON_HTTP_TOKEN missing');
const token = tokenLine.replace(/^\\s*AXON_HTTP_TOKEN\\s*=\\s*/, '').trim().replace(/^["']|["']$/g, '');

async function json(url, options) {
  const response = await fetch(url, options);
  return await response.json();
}

function connect(wsUrl) {
  let nextId = 1;
  const ws = new WebSocket(wsUrl);
  const pending = new Map();
  ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);
    if (msg.id && pending.has(msg.id)) {
      const p = pending.get(msg.id);
      pending.delete(msg.id);
      msg.error ? p.reject(new Error(JSON.stringify(msg.error))) : p.resolve(msg.result);
    }
  };
  const opened = new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
  return {
    opened,
    close: () => ws.close(),
    call(method, params = {}) {
      const id = nextId++;
      ws.send(JSON.stringify({ id, method, params }));
      return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
    }
  };
}

const browserVersion = await json('http://127.0.0.1:9222/json/version');
const browser = connect(browserVersion.webSocketDebuggerUrl);
await browser.opened;
const load = await browser.call('Extensions.loadUnpacked', { path: extDir.replaceAll('\\\\', '/') });
const extensionId = load.id;

await fetch('http://127.0.0.1:9222/json/new?' + encodeURIComponent('chrome-extension://' + extensionId + '/options.html'), { method: 'PUT' });
await new Promise((resolve) => setTimeout(resolve, 800));
let targets = await json('http://127.0.0.1:9222/json/list');
const optionsTarget = targets.find((target) => target.url === 'chrome-extension://' + extensionId + '/options.html');
if (!optionsTarget) throw new Error('extension options target not found');
const options = connect(optionsTarget.webSocketDebuggerUrl);
await options.opened;
async function evalIn(target, expression) {
  const result = await target.call('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true, userGesture: true });
  if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
  return result.result.value;
}
await evalIn(options, "(async()=>{ await chrome.storage.local.set({axonUrl:'http://127.0.0.1:8001', axonToken:" + JSON.stringify(token) + ", autoScrapeEnabled:false}); return true; })()");
options.close();

targets = await json('http://127.0.0.1:9222/json/list');
const pageTarget = targets.find((target) => target.url === extIdExpectedUrl) || targets.find((target) => target.url && target.url.includes('/product/claude-code'));
if (!pageTarget) throw new Error('target page not found');
await fetch('http://127.0.0.1:9222/json/activate/' + pageTarget.id).catch(() => {});

let allTargets = (await browser.call('Target.getTargets')).targetInfos;
let workerInfo = allTargets.find((target) => target.type === 'service_worker' && target.url.includes(extensionId));
if (!workerInfo) {
  await fetch('http://127.0.0.1:9222/json/new?' + encodeURIComponent('chrome-extension://' + extensionId + '/options.html'), { method: 'PUT' });
  await new Promise((resolve) => setTimeout(resolve, 800));
  allTargets = (await browser.call('Target.getTargets')).targetInfos;
  workerInfo = allTargets.find((target) => target.type === 'service_worker' && target.url.includes(extensionId));
}
if (!workerInfo) throw new Error('extension service worker not found');
targets = await json('http://127.0.0.1:9222/json/list');
const workerTarget = targets.find((target) => target.id === workerInfo.targetId || target.url === workerInfo.url);
if (!workerTarget?.webSocketDebuggerUrl) throw new Error('extension worker websocket not found');
const worker = connect(workerTarget.webSocketDebuggerUrl);
await worker.opened;

const actionExpression = "(async()=>{ const tabs = await chrome.tabs.query({url:'https://claude.com/*'}); const tab = tabs.find(t => t.url && t.url.includes('/product/claude-code')) || tabs[0] || {}; await scrapeAndCopyFromContext('https://claude.com/product/claude-code', {id: tab.id}); const scrape = await chrome.storage.local.get(['lastContextAction']); const tabs2 = await chrome.tabs.query({url:'https://claude.com/*'}); const tab2 = tabs2.find(t => t.url && t.url.includes('/product/claude-code')) || tabs2[0] || {}; await crawlFromContext('https://claude.com/product/claude-code', {id: tab2.id}); const crawl = await chrome.storage.local.get(['lastContextAction']); return {scrape: scrape.lastContextAction, crawl: crawl.lastContextAction}; })()";
const actions = await evalIn(worker, actionExpression);
worker.close();
browser.close();

console.log(JSON.stringify({ extensionId, pageUrl: pageTarget.url, actions }, null, 2));
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
'@
\$path=Join-Path \$env:TEMP 'axon-extension-regression.cjs'
Set-Content -Path \$path -Value \$script -Encoding UTF8
node \$path`);
    return { automation };
  }

  if (phase === "verify") {
  const verify = await ps(`\$ErrorActionPreference='Stop'
\$bin = Join-Path (Join-Path \$env:USERPROFILE '.local') 'bin'
\$env:HOME=\$env:USERPROFILE
\$env:Path="\$bin;\$env:Path"
\$env:AXON_SQLITE_PATH=Join-Path (Join-Path \$env:USERPROFILE '.axon') '${sqliteName}'
\$env:QDRANT_URL='http://127.0.0.1:1'
\$env:TEI_URL='http://100.88.16.79:52000'
\$env:AXON_CHROME_REMOTE_URL=''
\$clip=Get-Clipboard -Raw
if (\$clip.Length -lt 1000) { throw "clipboard markdown too short: \$($clip.Length)" }
for (\$i = 0; \$i -lt 3; \$i++) {
  \$crawlJsonText=(& (Join-Path \$bin 'axon.exe') crawl list --json) -join "\`n"
  \$crawlData=\$crawlJsonText | ConvertFrom-Json
  \$unfinished=@(\$crawlData.jobs | Where-Object { \$_.status -in @('running','pending','queued') })
  if (\$unfinished.Count -eq 0) { break }
  Start-Sleep -Seconds 2
}
\$crawlJsonText=(& (Join-Path \$bin 'axon.exe') crawl list --json) -join "\`n"
\$crawlData=\$crawlJsonText | ConvertFrom-Json
\$unfinished=@(\$crawlData.jobs | Where-Object { \$_.status -in @('running','pending','queued') })
\$completed=@(\$crawlData.jobs | Where-Object { \$_.status -eq 'completed' })
if (\$unfinished.Count -gt 0) { throw "crawl did not finish:\`n\$crawlJsonText" }
if (\$completed.Count -eq 0) { throw "no completed crawl found:\`n\$crawlJsonText" }
\$status=& (Join-Path \$bin 'axon.exe') status
[pscustomobject]@{ clipboard_length=\$clip.Length; clipboard_preview=\$clip.Substring(0, [Math]::Min(600, \$clip.Length)); status=(\$status -join "\`n"); crawl_list=\$crawlData } | ConvertTo-Json -Depth 8`);
    return {
      verify,
      note: "Windows-MCP native Chrome context menu selection is still not reliable; this regression invokes the installed extension handlers that the context menu calls."
    };
  }

  throw new Error(`Unknown phase: ${phase}`);
}
EOF_JS

run_phase() {
local phase="$1"
cp "$template_file" "$js_file"
AXON_SCRIPT_TARGET_URL="$TARGET_URL" \
AXON_SCRIPT_EXPECTED_FINAL_URL="$EXPECTED_FINAL_URL" \
AXON_SCRIPT_HOST_BASE="http://$HOST_IP:$PORT" \
AXON_SCRIPT_SQLITE_NAME="$COLLECTION_DB" \
AXON_SCRIPT_PHASE="$phase" \
AXON_SCRIPT_RELEASE_TAG="$release_tag" \
node - "$js_file" <<'NODE'
const fs = require('node:fs');
const path = process.argv[2];
let script = fs.readFileSync(path, 'utf8');
const replacements = {
  __PHASE_JSON__: JSON.stringify(process.env.AXON_SCRIPT_PHASE),
  __TARGET_URL_JSON__: JSON.stringify(process.env.AXON_SCRIPT_TARGET_URL),
  __EXPECTED_FINAL_URL_JSON__: JSON.stringify(process.env.AXON_SCRIPT_EXPECTED_FINAL_URL),
  __HOST_BASE_JSON__: JSON.stringify(process.env.AXON_SCRIPT_HOST_BASE),
  __SQLITE_NAME_JSON__: JSON.stringify(process.env.AXON_SCRIPT_SQLITE_NAME),
  __RELEASE_TAG_JSON__: JSON.stringify(process.env.AXON_SCRIPT_RELEASE_TAG),
};
for (const [placeholder, value] of Object.entries(replacements)) {
  script = script.replaceAll(placeholder, value);
}
fs.writeFileSync(path, script);
NODE

labby gateway code exec --json --file "$js_file"
}

for phase in install configure start-server wait-server start-chrome automation start-worker; do
  run_phase "$phase"
done

for attempt in $(seq 1 36); do
  if run_phase verify; then
    exit 0
  fi
  echo "crawl not complete yet; verify retry $attempt/36" >&2
  sleep 10
done

echo "crawl did not complete before verification timeout" >&2
exit 1
