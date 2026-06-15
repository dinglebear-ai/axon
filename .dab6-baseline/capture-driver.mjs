// agent-os CDP driver: serves the palette dist, launches Chrome with a debug port,
// drives interactive states, screenshots each. Node 24 built-in WebSocket + http.
import { spawn } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";
import { mkdirSync, writeFileSync, readFileSync, existsSync, statSync } from "node:fs";
import http from "node:http";
import path from "node:path";

const DIST = "C:\\dab6\\dist";
const OUT = "C:\\dab6\\shots";
const CHROME = "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";
const HTTP_PORT = 8731;
const DBG_PORT = 9876;
const W = 900, H = 640;
mkdirSync(OUT, { recursive: true });

const MIME = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css",
  ".woff2": "font/woff2", ".svg": "image/svg+xml", ".json": "application/json",
  ".png": "image/png", ".ico": "image/x-icon", ".map": "application/json" };

// static server (serves dist; SPA: unknown -> index.html)
const server = http.createServer((req, res) => {
  let urlPath = decodeURIComponent(req.url.split("?")[0]);
  if (urlPath === "/") urlPath = "/index.html";
  let file = path.join(DIST, urlPath);
  if (!existsSync(file) || !statSync(file).isFile()) file = path.join(DIST, "index.html");
  const ext = path.extname(file).toLowerCase();
  res.writeHead(200, { "content-type": MIME[ext] || "application/octet-stream" });
  res.end(readFileSync(file));
});
await new Promise((r) => server.listen(HTTP_PORT, "127.0.0.1", r));
const ORIGIN = `http://127.0.0.1:${HTTP_PORT}/index.html`;

const chrome = spawn(CHROME, [
  "--headless=new", "--disable-gpu", "--hide-scrollbars",
  `--remote-debugging-port=${DBG_PORT}`, `--window-size=${W},${H}`,
  "--user-data-dir=C:\\dab6\\chrome-profile", "about:blank",
], { stdio: "ignore" });

async function getWsUrl() {
  for (let i = 0; i < 60; i++) {
    try { const j = await (await fetch(`http://127.0.0.1:${DBG_PORT}/json/version`)).json();
      if (j.webSocketDebuggerUrl) return j.webSocketDebuggerUrl; } catch {}
    await sleep(300);
  }
  throw new Error("chrome CDP never came up");
}

function makeClient(ws) {
  let nextId = 1; const pending = new Map();
  ws.addEventListener("message", (ev) => {
    const m = JSON.parse(ev.data.toString());
    if (m.id != null && pending.has(m.id)) {
      const { resolve, reject, t } = pending.get(m.id); clearTimeout(t); pending.delete(m.id);
      m.error ? reject(new Error(JSON.stringify(m.error))) : resolve(m.result);
    }
  });
  return (method, params = {}, sessionId) => {
    const id = nextId++; const msg = { id, method, params }; if (sessionId) msg.sessionId = sessionId;
    ws.send(JSON.stringify(msg));
    return new Promise((resolve, reject) => {
      const t = setTimeout(() => { pending.delete(id); reject(new Error(`timeout ${method}`)); }, 25000);
      pending.set(id, { resolve, reject, t });
    });
  };
}

async function main() {
  const wsUrl = await getWsUrl();
  const ws = new WebSocket(wsUrl);
  await new Promise((res, rej) => { ws.addEventListener("open", () => res(), { once: true });
    ws.addEventListener("error", () => rej(new Error("ws error")), { once: true }); });
  const send = makeClient(ws);
  const { targetId } = await send("Target.createTarget", { url: "about:blank" });
  const { sessionId: S } = await send("Target.attachToTarget", { targetId, flatten: true });
  await send("Page.enable", {}, S);
  await send("Runtime.enable", {}, S);
  await send("DOM.enable", {}, S);
  await send("Emulation.setDeviceMetricsOverride", { width: W, height: H, deviceScaleFactor: 1, mobile: false }, S);

  async function nav(url) { await send("Page.navigate", { url }, S); await sleep(2200); }
  async function evalJs(expr) {
    const r = await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true }, S);
    if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
    return r.result.value;
  }
  async function shot(name, fullPage = false) {
    const params = { format: "png", captureBeyondViewport: !!fullPage };
    if (fullPage) {
      const m = await send("Page.getLayoutMetrics", {}, S);
      const cs = m.cssContentSize || m.contentSize;
      params.clip = { x: 0, y: 0, width: Math.ceil(cs.width), height: Math.max(H, Math.ceil(cs.height)), scale: 1 };
    }
    const { data } = await send("Page.captureScreenshot", params, S);
    writeFileSync(path.join(OUT, name), Buffer.from(data, "base64"));
    console.log("wrote", name, fullPage ? "(full)" : "");
  }
  async function typeText(text) {
    // Input.insertText fires a single React-visible input event per char (no
    // keyDown+char double-insertion). Reliable for controlled inputs.
    for (const ch of text) {
      await send("Input.insertText", { text: ch }, S);
      await sleep(55);
    }
  }
  const focusInput = () => evalJs(`(()=>{const el=document.querySelector('input,textarea,[contenteditable]');if(el){el.focus();return true;}return false;})()`);

  // 1) default command bar (idle)
  await nav(ORIGIN);
  await shot("01-app-default-command-bar.png");

  // 2) command bar focused + ActionList filtered by "s"
  await nav(ORIGIN); await focusInput(); await typeText("s"); await sleep(800);
  await shot("02-app-actionlist-query-s.png", true);

  // 3) ActionList filtered by "crawl"
  await typeText("crawl"); await sleep(800);
  await shot("03-app-actionlist-query-crawl.png", true);

  // 4) settings panel
  await nav(ORIGIN); await sleep(400);
  const okSettings = await evalJs(`(()=>{const b=Array.from(document.querySelectorAll('button,[role=button]'));let s=b.find(x=>/setting/i.test((x.getAttribute('title')||'')+(x.getAttribute('aria-label')||'')));if(!s&&b.length)s=b[b.length-1];if(s){s.click();return true;}return false;})()`);
  await sleep(1100);
  await shot("04-app-settings-panel.png", true);
  console.log("settings:", okSettings);

  // 5) footer + content (query active)
  await nav(ORIGIN); await focusInput(); await typeText("ask"); await sleep(800);
  await shot("05-app-footer-and-content.png", true);

  // 10) fixture route (OutputPanel matrix) — wide + tall, longer settle for shiki
  await send("Emulation.setDeviceMetricsOverride", { width: 1200, height: 3000, deviceScaleFactor: 1, mobile: false }, S);
  await nav(`http://127.0.0.1:${HTTP_PORT}/index.html?fixture=operation-results`);
  await sleep(5000);
  await shot("10-fixture-operation-results.png", true);

  await send("Target.closeTarget", { targetId }, S).catch(() => {});
  ws.close(); chrome.kill(); server.close();
  await sleep(400);
  console.log("DONE");
}

main().catch((e) => { console.error("FAIL", e); try { chrome.kill(); } catch {} process.exit(1); });
