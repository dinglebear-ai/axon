/* ============================================================
 * Axon Chrome Extension — side-panel launcher controller
 * Owns config + the /v1/* request layer, current-tab tracking, and
 * the browse → run → result/doc flow from the design handoff
 * (Reference/ext/sidepanel.jsx). Wired to the real Axon server.
 * ============================================================ */

(function () {
  const { el, Icon, ActionBody, RetrieveDocFromRaw } = window.AxonRender;
  const { iconSvg, axonMark } = window.AxonIcons;
  const { OPS_BY_CATEGORY, OP_BY_ID, toneOf, tint, hostOf } = window.AxonData;

  const DEFAULT_AXON_URL = "http://100.88.16.79:8001";
  const COLOR_CODE = true; // color-code actions by family (design default on)
  // `SourceRequest.execution` has no per-field defaults once the key is
  // present, so a synchronous ("foreground") request must spell out the
  // whole policy rather than just `{ mode: "foreground" }`.
  const FOREGROUND_EXECUTION = { mode: "foreground", priority: "normal", detached: false, heartbeat_interval_secs: 5 };

  const state = { view: "browse", tab: null, online: null, host: "", op: null, lastStatus: 200, bodyEl: null };
  // config is read once from storage and refreshed on change (see storage.onChanged),
  // so requestAxon reads it synchronously instead of an IPC round-trip per request.
  let config = { axonUrl: DEFAULT_AXON_URL, axonToken: "" };

  /* ── config + request layer ── */
  async function refreshConfig() {
    const stored = (chrome.storage && chrome.storage.local) ? await chrome.storage.local.get(["axonUrl", "axonToken"]) : {};
    config = { axonUrl: stored.axonUrl || DEFAULT_AXON_URL, axonToken: stored.axonToken || "" };
    return config;
  }
  function isLoopback(server) {
    try { const h = new URL(server).hostname; return h === "127.0.0.1" || h === "localhost" || h === "::1"; } catch { return false; }
  }
  async function requestAxon(method, path, body) {
    const server = config.axonUrl.trim().replace(/\/+$/, "");
    const token = config.axonToken.trim();
    if (!server) throw new Error("Axon server URL is required. Open Settings.");
    const headers = {};
    if (body !== undefined) headers["Content-Type"] = "application/json";
    if (token) headers.Authorization = `Bearer ${token}`;
    if (!token && path !== "/healthz" && !isLoopback(server)) throw new Error("Missing bearer token for this server. Open Settings.");
    const res = await fetch(`${server}${path}`, { method, headers, body: body !== undefined ? JSON.stringify(body) : undefined });
    state.lastStatus = res.status;
    const text = await res.text();
    if (!res.ok) {
      let message = text;
      try { const j = JSON.parse(text); message = j.message || j.error || text; } catch { /* keep text */ }
      const err = new Error(message || `HTTP ${res.status}`); err.status = res.status; err.statusText = res.statusText; throw err;
    }
    if (!text) return {};
    try { return JSON.parse(text); } catch { return text; }
  }
  function friendlyError(error) {
    const message = error instanceof Error ? error.message : String(error);
    const lower = message.toLowerCase();
    if (lower.includes("missing bearer token") || lower.includes("401") || lower.includes("auth_failed")) return "Auth failed — Axon needs the bearer token for this server. Open Settings.";
    if (lower.includes("403") || lower.includes("forbidden")) return "Forbidden by the Axon server or proxy. Check the URL and token.";
    if (lower.includes("failed to fetch") || lower.includes("networkerror")) return "Axon is unreachable from Chrome. Check the server URL and that Axon is listening.";
    return message.replace(/<!--[\s\S]*?-->/g, "").trim() || "Request failed.";
  }

  /* ── current tab ── */
  function isWeb(t) { return /^https?:\/\//i.test((t && t.url) || ""); }
  async function getActiveTab() {
    try {
      const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
      if (isWeb(tab)) return tab;
      const all = await chrome.tabs.query({ currentWindow: true });
      const web = all.filter(isWeb).sort((a, b) => (b.lastAccessed || 0) - (a.lastAccessed || 0))[0];
      return web || tab || null;
    } catch { return null; }
  }

  /* ── arg model (derived from the catalog: arg / noArg / optionalArg) ──
   *   optional → run now, optional filter/focus input (sources, domains, suggest)
   *   none     → run now, no input (doctor, status, stats)
   *   url      → prefill the active tab + run (url/input/target args)
   *   query    → wait for a typed query (search, query, ask, …) */
  function argMode(op) {
    if (op.optionalArg) return "optional";
    if (op.noArg) return "none";
    return op.arg === "url" || op.arg === "input" || op.arg === "target" ? "url" : "query";
  }
  function splitUrls(arg) {
    return String(arg || "").split(/[\s,]+/).map((s) => s.trim()).filter(Boolean);
  }

  /* ── API dispatch per action ── */
  // Every URL-taking op refuses browser-internal/privileged schemes
  // (chrome://, file://, about:, devtools://, ...) with a user-visible
  // reason before dispatch; every free-text ("query") op is redacted
  // client-side first. See capture-redaction.js (window.AxonRedact) —
  // chrome-extension-contract.md Capture Contract / Security Contract.
  function assertUrlCaptureAllowed(url) {
    const reason = window.AxonRedact.blockedCaptureReason(url);
    if (reason) {
      const e = new Error(reason);
      e.status = 400;
      throw e;
    }
    return url;
  }

  function callApi(op, arg) {
    const mode = argMode(op);
    if (mode === "url" && arg) {
      splitUrls(arg).forEach(assertUrlCaptureAllowed);
    } else if (mode === "query" && typeof arg === "string" && arg) {
      arg = window.AxonRedact.redactText(arg).text;
    }
    switch (op.id) {
      case "scrape": return requestAxon("POST", "/v1/sources", { source: arg, scope: "page", embed: true, execution: FOREGROUND_EXECUTION });
      case "map": return requestAxon("POST", "/v1/map", { url: arg });
      case "retrieve": return requestAxon("POST", "/v1/retrieve", { url: arg });
      case "screenshot": return requestAxon("POST", "/v1/screenshot", { url: arg, full_page: true });
      case "brand": return requestAxon("POST", "/v1/brand", { url: arg });
      case "endpoints": return requestAxon("POST", "/v1/endpoints", { url: arg });
      case "diff": { const u = splitUrls(arg); if (u.length < 2) { const e = new Error("Diff needs two URLs — “URL A  URL B”."); e.status = 400; throw e; } return requestAxon("POST", "/v1/diff", { url_a: u[0], url_b: u[1] }); }
      case "crawl": return requestAxon("POST", "/v1/sources", { source: arg, scope: "site" });
      case "extract": return requestAxon("POST", "/v1/extract", { urls: [arg] });
      case "embed": return requestAxon("POST", "/v1/sources", { source: arg });
      case "ingest": return requestAxon("POST", "/v1/sources", { source: arg });
      case "search": return requestAxon("POST", "/v1/search", { query: arg, limit: 10 });
      case "research": return requestAxon("POST", "/v1/research", { query: arg, limit: 10 });
      case "query": return requestAxon("POST", "/v1/query", { query: arg, limit: 10 });
      case "suggest": return requestAxon("POST", "/v1/suggest", arg ? { focus: arg } : {});
      case "sources": return requestAxon("GET", "/v1/sources");
      case "domains": return requestAxon("GET", "/v1/domains");
      case "ask": return requestAxon("POST", "/v1/ask", { query: arg });
      case "summarize": return requestAxon("POST", "/v1/summarize", { urls: [arg] });
      case "evaluate": return requestAxon("POST", "/v1/evaluate", { question: arg });
      case "doctor": return requestAxon("GET", "/v1/doctor");
      case "status": return requestAxon("GET", "/v1/status");
      case "stats": return requestAxon("GET", "/v1/stats");
      default: { const e = new Error(`Unsupported action: ${op.id}`); throw e; }
    }
  }

  /* ── root ── */
  let panel;
  function mount() {
    panel = el("div", { class: "ext-panel" });
    document.getElementById("root").appendChild(panel);
    render();
  }
  function render() {
    panel.textContent = "";
    if (state.view === "browse") renderBrowse();
    else if (state.view === "result") renderResult();
    else if (state.view === "doc") renderDoc();
  }

  /* ── header ── */
  function brandHeader() {
    const dot = el("span", { class: `ext-dot ${state.online === false ? "is-off" : state.online == null ? "is-wait" : ""}`, title: state.online === false ? "Axon offline" : state.online == null ? "Checking…" : "Axon online" });
    const gear = el("button", { class: "ext-icon", title: "Settings", "aria-label": "Settings", onclick: openOptions }, iconSvg("settings", { size: 16 }));
    return el("header", { class: "ext-head" }, [
      el("span", { class: "ext-brand" }, [axonMark(22), el("span", { class: "ext-word" }, "Axon"), dot]),
      el("span", { class: "ext-host", title: state.host }, state.host || "not configured"),
      gear,
    ]);
  }
  function applyStatusBadge(badge, code) {
    const ok = code >= 200 && code < 300;
    badge.className = `ext-statusbadge ${code === 202 ? "is-accepted" : !ok ? "is-error" : ""}`;
    badge.textContent = "";
    badge.appendChild(Icon(ok ? "check" : "alert", 11));
    badge.appendChild(document.createTextNode(code >= 100 ? String(code) : "ERR"));
  }
  function resultHeader(title, back) {
    const badge = el("span", { class: "ext-statusbadge" });
    applyStatusBadge(badge, state.lastStatus);
    return el("header", { class: "ext-result-head" }, [
      el("button", { class: "ext-icon", title: "Back", "aria-label": "Back", onclick: back }, iconSvg("arrowLeft", { size: 17 })),
      el("span", { class: "ext-result-title" }, title),
      badge,
    ]);
  }
  function footer() {
    return el("div", { class: "ext-foot" }, [Icon("command", 12), el("span", null, "Right-click: Scrape+copy / Crawl / Ask")]);
  }

  /* ── browse view ── */
  function renderBrowse() {
    panel.appendChild(brandHeader());
    const scroll = el("div", { class: "ext-scroll aurora-scrollbar" });

    const url = (state.tab && state.tab.url) || "";
    const dom = url ? hostOf(url).split("/")[0] : "";
    const rest = url ? hostOf(url).slice(dom.length) : "";
    const tabCard = el("div", { class: "ext-tabcard" }, [
      el("div", { class: "ext-tabcard-top" }, [iconSvg("globe", { size: 13, color: "var(--aurora-accent-strong)" }), el("span", { class: "ext-tablabel" }, "THIS PAGE")]),
      el("div", { class: "ext-taburl", title: url }, url ? [dom, el("span", { class: "dim" }, rest)] : [el("span", { class: "dim" }, "Open an http:// or https:// tab")]),
      el("div", { class: "ext-quick" }, [
        quickBtn("scrape", "Scrape", "scrape"),
        quickBtn("crawl", "Crawl", "crawl"),
        quickBtn("extract", "Extract", "braces"),
      ]),
    ]);
    scroll.appendChild(tabCard);

    OPS_BY_CATEGORY.forEach((cat) => {
      if (!cat.ops.length) return;
      scroll.appendChild(el("div", { class: "ext-seclabel" }, cat.label));
      cat.ops.forEach((op) => scroll.appendChild(actionRow(op)));
    });

    panel.appendChild(scroll);
    panel.appendChild(footer());
  }
  function quickBtn(opId, label, icon) {
    const url = (state.tab && state.tab.url) || "";
    return el("button", { class: "ext-quickbtn", disabled: url ? undefined : "", onclick: () => openAction(OP_BY_ID[opId], url || undefined) }, [iconSvg(icon, { size: 13 }), label]);
  }
  function actionRow(op) {
    const t = toneOf(op.tone, COLOR_CODE);
    const tile = el("span", { class: "ext-tile", style: { color: t.fg, background: tint(t.base, 14, "var(--axon-page)"), border: `1px solid ${tint(t.base, 28)}` } }, iconSvg(op.icon, { size: 15 }));
    const name = el("span", { class: "ext-row-name" }, [op.name, op.async ? el("span", { class: "ext-async" }, "ASYNC") : null]);
    return el("button", { class: "ext-row", onclick: () => openAction(op) }, [
      tile,
      el("span", { class: "ext-row-main" }, [name, el("span", { class: "ext-row-sub" }, op.short)]),
      iconSvg("chevronRight", { size: 15, color: "var(--aurora-text-muted)" }),
    ]);
  }

  /* ── result view ── */
  function renderResult() {
    const op = state.op;
    const mode = argMode(op);
    panel.appendChild(resultHeader(op.name, () => { state.view = "browse"; render(); }));

    let argInput = null;
    if (mode !== "none") {
      const placeholder = mode === "url" ? "URL…" : op.argLabel || "input…";
      argInput = el("input", { class: "ext-arginput", type: "text", spellcheck: "false", autocomplete: "off", placeholder });
      const run = () => runWithArg(op, argInput.value.trim());
      argInput.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); run(); } });
      panel.appendChild(el("div", { class: "ext-argbar" }, [
        el("span", { class: "ext-arg-icon" }, iconSvg(mode === "url" ? "link" : "search", { size: 14 })),
        argInput,
        el("button", { class: "ext-argrun", title: "Run", onclick: run }, [iconSvg("enter", { size: 13 }), "Run"]),
      ]));
    }

    const body = el("div", { class: "ext-scroll aurora-scrollbar", style: { paddingTop: "12px" } });
    state.bodyEl = body;
    panel.appendChild(body);
    panel.appendChild(footer());

    // decide initial run: url/optional/none auto-run; query waits for a typed
    // query unless an override (context-menu intent) was supplied.
    const initial = state.initialArg != null ? state.initialArg : (mode === "url" ? ((state.tab && state.tab.url) || "") : "");
    state.initialArg = null;
    if (argInput) argInput.value = initial;
    if (mode === "query" && !initial) {
      body.appendChild(promptHint(op));
      if (argInput) setTimeout(() => argInput.focus(), 40);
    } else {
      runWithArg(op, initial);
    }
  }
  function promptHint(op) {
    return el("div", { class: "ext-loading" }, [Icon("arrowUp", 14, "var(--aurora-text-muted)"), `Enter a ${op.argLabel || "query"} above to run ${op.name}.`]);
  }
  async function runWithArg(op, arg) {
    const body = state.bodyEl;
    if (!body) return;
    body.textContent = "";
    body.appendChild(el("div", { class: "ext-loading" }, [axonMark(18, true), `Running ${op.name}…`]));
    const tones = toneOf(op.tone, COLOR_CODE);
    try {
      const raw = await callApi(op, arg);
      // refresh header status badge
      refreshResultBadge();
      body.textContent = "";
      if (op.id === "scrape") {
        const markdown = scrapeMarkdownFromRaw(raw);
        if (markdown) {
          body.appendChild(scrapeActionBar(markdown, state.copyAfterRun));
        }
      }
      body.appendChild(ActionBody(op, raw, tones, {
        onOpenDoc: (u) => openDoc(u),
        onAct: (kind, u) => openAction(OP_BY_ID[kind], u),
      }));
      state.copyAfterRun = false;
    } catch (error) {
      state.lastStatus = (error && error.status) || 0; // 0 = transport/network error
      refreshResultBadge();
      body.textContent = "";
      body.appendChild(errorCard(op, error));
    }
  }
  function scrapeMarkdownFromRaw(raw) {
    const content = raw && typeof raw === "object" ? raw.inline?.content : null;
    if (content && content.kind === "inline_text") return content.text;
    const payload = raw && typeof raw === "object" && raw.payload && typeof raw.payload === "object" ? raw.payload : {};
    return raw?.markdown || payload.markdown || raw?.content || payload.content || raw?.output || "";
  }
  function scrapeActionBar(markdown, shouldCopy) {
    const status = el("span", { class: "ext-copy-status" }, shouldCopy ? "copying..." : "markdown ready");
    const copy = async () => {
      try {
        await navigator.clipboard.writeText(markdown);
        status.textContent = "copied";
        status.classList.add("is-ok");
      } catch {
        status.textContent = "copy failed";
        status.classList.add("is-error");
      }
    };
    const bar = el("div", { class: "ext-scrape-actions" }, [
      el("span", { class: "ext-scrape-label" }, [Icon("scrape", 13), "Scraped markdown"]),
      status,
      el("button", { class: "ext-copybtn", type: "button", onclick: copy }, [Icon("copy", 13), "Copy"]),
    ]);
    if (shouldCopy) setTimeout(copy, 0);
    return bar;
  }
  function refreshResultBadge() {
    // update just the header badge in place to reflect the latest HTTP status
    const badge = panel.querySelector(".ext-result-head .ext-statusbadge");
    if (badge) applyStatusBadge(badge, state.lastStatus);
  }
  function errorCard(op, error) {
    const status = error && error.status ? error.status : "ERR";
    return el("div", { class: "ext-errorcard" }, [
      el("div", { class: "err-title" }, [Icon("alert", 15), `${op.name} failed${typeof status === "number" ? ` · ${status}` : ""}`]),
      el("div", { class: "err-body" }, friendlyError(error)),
      el("div", { class: "err-hint" }, "Check Settings (server URL + token), or that Axon is running."),
    ]);
  }

  /* ── doc viewer (sources → retrieve) ── */
  async function openDoc(url) {
    state.view = "doc";
    state.docUrl = url;
    render();
  }
  function renderDoc() {
    panel.appendChild(resultHeader("Document", () => { state.view = "result"; render(); }));
    const body = el("div", { class: "ext-scroll aurora-scrollbar", style: { paddingTop: "12px" } });
    panel.appendChild(body);
    panel.appendChild(footer());
    body.appendChild(el("div", { class: "ext-loading" }, [axonMark(18, true), "Loading document…"]));
    requestAxon("POST", "/v1/retrieve", { url: state.docUrl })
      .then((raw) => { refreshResultBadge(); body.textContent = ""; body.appendChild(RetrieveDocFromRaw(raw, toneOf("cyan", COLOR_CODE))); })
      .catch((error) => { refreshResultBadge(); body.textContent = ""; body.appendChild(errorCard({ name: "Retrieve" }, error)); });
  }

  /* ── open an action (from row / quick action / context-menu intent) ── */
  function openAction(op, argOverride) {
    if (!op) return;
    state.op = op;
    state.initialArg = argOverride != null ? argOverride : null;
    state.view = "result";
    render();
  }
  function openOptions() {
    if (chrome.runtime && chrome.runtime.openOptionsPage) chrome.runtime.openOptionsPage();
    else window.open(chrome.runtime.getURL("options.html"));
  }

  /* ── server status dot ── */
  function paintStatus() {
    if (state.view !== "browse" || !panel) return; // browse rebuilds from state on its own
    const dot = panel.querySelector(".ext-head .ext-dot");
    if (dot) {
      dot.className = `ext-dot ${state.online === false ? "is-off" : state.online == null ? "is-wait" : ""}`;
      dot.title = state.online === false ? "Axon offline" : state.online == null ? "Checking…" : "Axon online";
    }
    const host = panel.querySelector(".ext-head .ext-host");
    if (host) { host.textContent = state.host || "not configured"; host.title = state.host; }
  }
  async function checkHealth() {
    try { state.host = new URL(config.axonUrl).host; } catch { state.host = config.axonUrl; }
    state.online = null;
    paintStatus();
    try {
      await requestAxon("GET", "/healthz");
      state.online = true;
    } catch { state.online = false; }
    paintStatus();
  }

  /* ── context-menu intents ── */
  const INTENT_OPS = new Set(["scrape", "crawl", "ask"]); // ops the context menu may trigger
  function applyIntent(intent) {
    if (!intent || !INTENT_OPS.has(intent.op)) return;
    state.copyAfterRun = intent.op === "scrape" && intent.copy === true;
    openAction(OP_BY_ID[intent.op], intent.arg || undefined);
  }
  async function consumePendingIntent() {
    if (!(chrome.storage && chrome.storage.local)) return;
    const { axonPendingIntent } = await chrome.storage.local.get(["axonPendingIntent"]);
    if (axonPendingIntent && Date.now() - (axonPendingIntent.ts || 0) < 60000) {
      await chrome.storage.local.remove(["axonPendingIntent"]);
      applyIntent(axonPendingIntent);
    }
  }

  // Re-resolve the active tab; only rebuild browse when the URL actually changed
  // (onUpdated fires repeatedly during a single page load).
  async function refreshTab() {
    const next = await getActiveTab();
    const changed = (next && next.url) !== (state.tab && state.tab.url);
    state.tab = next;
    if (changed && state.view === "browse") render();
  }

  /* ── boot ── */
  async function init() {
    await refreshConfig();
    state.tab = await getActiveTab();
    mount();
    checkHealth();
    consumePendingIntent();

    chrome.runtime && chrome.runtime.onMessage && chrome.runtime.onMessage.addListener((msg) => {
      if (msg && msg.type === "axon-intent") applyIntent(msg);
    });
    chrome.tabs && chrome.tabs.onActivated && chrome.tabs.onActivated.addListener(() => refreshTab());
    chrome.tabs && chrome.tabs.onUpdated && chrome.tabs.onUpdated.addListener((_id, info) => { if (info.status === "complete" || info.url) refreshTab(); });
    chrome.storage && chrome.storage.onChanged && chrome.storage.onChanged.addListener((changes, area) => { if (area === "local" && (changes.axonUrl || changes.axonToken)) refreshConfig().then(checkHealth); });
  }

  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", init);
  else init();
})();
