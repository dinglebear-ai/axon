// Client-side pre-redaction + blocked-capture-scheme guard shared with the
// popup (see capture-redaction.js — AxonRedact.redactText/redactUrl/
// isBlockedCaptureUrl/blockedCaptureReason).
// Optional host-permission helper (see host-permissions.js —
// AxonHostPermissions.hasAxonHostPermission/requestAxonHostPermission).
// manifest.json declares the Axon server origins as
// `optional_host_permissions`, not a blanket `host_permissions` grant, so
// every fetch path here must check (or, for a real user gesture, request)
// the grant first.
importScripts("capture-redaction.js", "host-permissions.js");

const DEFAULT_AXON_URL = "http://100.88.16.79:8001";
const AUTO_SCRAPE_HISTORY_KEY = "autoScrapeHistory";
const AUTO_SCRAPE_COOLDOWN_MS = 24 * 60 * 60 * 1000;
// `SourceRequest.execution` has no per-field defaults once the key is
// present, so a synchronous ("foreground") request must spell out the whole
// policy rather than just `{ mode: "foreground" }`.
const FOREGROUND_EXECUTION = { mode: "foreground", priority: "normal", detached: false, heartbeat_interval_secs: 5 };

chrome.runtime.onInstalled.addListener(() => {
  chrome.action.setBadgeBackgroundColor({ color: "#24536c" });
  setupSidePanelAction();
  setupContextMenus();
});

chrome.runtime.onStartup.addListener(setupSidePanelAction);
setupSidePanelAction();

chrome.tabs.onUpdated.addListener((tabId, changeInfo, tab) => {
  if (changeInfo.status !== "complete") {
    return;
  }

  const url = tab.url || changeInfo.url || "";
  if (!isScrapableUrl(url)) {
    return;
  }

  scrapeVisitedUrl(tabId, url);
});

async function scrapeVisitedUrl(tabId, url) {
  const config = await loadConfig();
  if (!config.autoScrapeEnabled) {
    return;
  }

  const urlKey = scrapeHistoryKey(url);
  if (await wasScrapedWithinCooldown(urlKey)) {
    return;
  }

  // A tab-update listener is not a user gesture, so this can only *check*
  // the grant (chrome.permissions.contains never prompts) — it must never
  // call chrome.permissions.request here. Open Settings and click Save (a
  // real gesture) to grant the server origin.
  if (!(await hasAxonServerPermission(config.axonUrl))) {
    await markAutoScrapeAttempt(urlKey, url, false);
    await chrome.storage.local.set({
      lastAutoScrape: {
        ok: false,
        url,
        at: new Date().toISOString(),
        error: `Axon needs permission for ${config.axonUrl}. Open Settings and click "Save settings" to grant it.`
      }
    });
    return;
  }

  await markAutoScrapeAttempt(urlKey, url, null);
  await setBadge(tabId, "SCR", "#0e7490");

  try {
    // Background execution (the SourceRequest default) — this just submits
    // the page for indexing, it doesn't need the content back.
    await postAxon(config, "/v1/sources", { source: url, scope: "page" });
    await markAutoScrapeAttempt(urlKey, url, true);
    await chrome.storage.local.set({
      lastAutoScrape: {
        ok: true,
        url,
        at: new Date().toISOString()
      }
    });
    await flashBadge(tabId, "OK", "#247a6b");
  } catch (error) {
    await markAutoScrapeAttempt(urlKey, url, false);
    await chrome.storage.local.set({
      lastAutoScrape: {
        ok: false,
        url,
        at: new Date().toISOString(),
        error: error instanceof Error ? error.message : String(error)
      }
    });
    await flashBadge(tabId, "ERR", "#8f4b5c");
  }
}

async function loadConfig() {
  const stored = await chrome.storage.local.get(["axonUrl", "axonToken", "autoScrapeEnabled"]);
  return {
    axonUrl: stored.axonUrl || DEFAULT_AXON_URL,
    axonToken: stored.axonToken || "",
    autoScrapeEnabled: stored.autoScrapeEnabled === true
  };
}

async function postAxon(config, path, body) {
  const server = config.axonUrl.trim().replace(/\/+$/, "");
  const headers = { "Content-Type": "application/json" };

  if (!server) {
    throw new Error("Axon server URL is required.");
  }

  if (config.axonToken) {
    headers.Authorization = `Bearer ${config.axonToken}`;
  }

  const response = await fetch(`${server}${path}`, {
    method: "POST",
    headers,
    body: JSON.stringify(body)
  });

  if (!response.ok) {
    const text = await response.text();
    throw new Error(`${response.status} ${response.statusText}${text ? `: ${text}` : ""}`);
  }

  const text = await response.text();
  if (!text) {
    return {};
  }
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

function isScrapableUrl(url) {
  return /^https?:\/\//i.test(url) && !AxonRedact.isBlockedCaptureUrl(url);
}

// `AxonHostPermissions` is loaded via importScripts above; guard anyway so a
// missing/failed import degrades to "assume granted" rather than throwing.
async function hasAxonServerPermission(serverUrl) {
  if (typeof AxonHostPermissions === "undefined") {
    return true;
  }
  return AxonHostPermissions.hasAxonHostPermission(serverUrl);
}

// Only call from a real user gesture (a context-menu click counts; a
// tab-update listener does not — see scrapeVisitedUrl).
async function ensureAxonServerPermissionForGesture(serverUrl) {
  if (typeof AxonHostPermissions === "undefined") {
    return true;
  }
  if (await AxonHostPermissions.hasAxonHostPermission(serverUrl)) {
    return true;
  }
  const granted = await AxonHostPermissions.requestAxonHostPermission(serverUrl);
  if (!granted) {
    throw new Error(`Axon needs permission for ${serverUrl}. Grant it when Chrome prompts, or open Settings and click "Save settings".`);
  }
  return true;
}

async function wasScrapedWithinCooldown(urlKey) {
  const history = await loadAutoScrapeHistory();
  const previous = history[urlKey];
  if (!previous?.ok || !previous?.at) {
    return false;
  }

  const previousAt = Date.parse(previous.at);
  return Number.isFinite(previousAt) && Date.now() - previousAt < AUTO_SCRAPE_COOLDOWN_MS;
}

async function markAutoScrapeAttempt(urlKey, url, ok) {
  const history = pruneAutoScrapeHistory(await loadAutoScrapeHistory());
  history[urlKey] = {
    ok,
    url,
    at: new Date().toISOString()
  };
  await chrome.storage.local.set({ [AUTO_SCRAPE_HISTORY_KEY]: history });
}

async function loadAutoScrapeHistory() {
  const stored = await chrome.storage.local.get([AUTO_SCRAPE_HISTORY_KEY]);
  const history = stored[AUTO_SCRAPE_HISTORY_KEY];
  return history && typeof history === "object" && !Array.isArray(history) ? history : {};
}

function pruneAutoScrapeHistory(history) {
  const now = Date.now();
  for (const [key, entry] of Object.entries(history)) {
    const timestamp = Date.parse(entry?.at || "");
    if (!Number.isFinite(timestamp) || now - timestamp > AUTO_SCRAPE_COOLDOWN_MS) {
      delete history[key];
    }
  }
  return history;
}

function scrapeHistoryKey(url) {
  try {
    const parsed = new URL(url);
    parsed.hash = "";
    return parsed.toString();
  } catch {
    return url;
  }
}

async function flashBadge(tabId, text, color) {
  await setBadge(tabId, text, color);
  setTimeout(() => {
    chrome.action.setBadgeText({ tabId, text: "" }).catch(() => {});
  }, 2_200);
}

async function setBadge(tabId, text, color) {
  await chrome.action.setBadgeBackgroundColor({ color });
  await chrome.action.setBadgeText({ tabId, text });
}

function setupSidePanelAction() {
  chrome.sidePanel
    ?.setPanelBehavior?.({ openPanelOnActionClick: true })
    .catch(() => {});
}

// Right-click menus. Page actions run in the background; Ask opens the side
// panel because the answer needs a visible reading surface.
function setupContextMenus() {
  if (!chrome.contextMenus?.create) {
    return;
  }
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({ id: "axon-scrape", title: "Scrape with Axon (copy markdown)", contexts: ["page", "link"] });
    chrome.contextMenus.create({ id: "axon-crawl", title: "Crawl this page with Axon", contexts: ["page"] });
    chrome.contextMenus.create({ id: "axon-ask", title: 'Ask Axon about "%s"', contexts: ["selection"] });
  });
}

chrome.contextMenus?.onClicked?.addListener(async (info, tab) => {
  if (info.menuItemId === "axon-scrape") {
    await scrapeAndCopyFromContext(info.linkUrl || info.pageUrl || tab?.url || "", tab);
    return;
  }

  if (info.menuItemId === "axon-crawl") {
    await crawlFromContext(info.pageUrl || tab?.url || "", tab);
    return;
  }

  if (info.menuItemId !== "axon-ask") {
    return;
  }

  const blockedReason = AxonRedact.blockedCaptureReason(tab?.url || info.pageUrl || "");
  if (blockedReason) {
    await notifyContextAction("Axon capture blocked", blockedReason);
    return;
  }

  const intent = { op: "ask", arg: AxonRedact.redactText(info.selectionText || "").text };

  // Stash the intent so a freshly-opened panel can pick it up on load,
  // then nudge any already-open panel via runtime message.
  await chrome.storage.local.set({ axonPendingIntent: { ...intent, ts: Date.now() } });

  try {
    if (tab?.id && /^https?:\/\//i.test(tab.url || "")) {
      await chrome.sidePanel.open({ tabId: tab.id });
    } else if (tab?.windowId != null) {
      await chrome.sidePanel.open({ windowId: tab.windowId });
    }
  } catch {
    // Side panel may already be open, or the surface may be restricted.
  }

  chrome.runtime.sendMessage({ type: "axon-intent", ...intent }).catch(() => {});
});

async function scrapeAndCopyFromContext(url, tab) {
  if (!isScrapableUrl(url)) {
    await flashContextError(tab);
    await notifyContextAction("Axon capture blocked", AxonRedact.blockedCaptureReason(url) || "Only http:// and https:// pages can be scraped.");
    return;
  }

  await setContextBadge(tab, "SCR", "#0e7490");
  try {
    const config = await loadConfig();
    // Context-menu clicks are a real user gesture, so this may prompt.
    await ensureAxonServerPermissionForGesture(config.axonUrl);
    const raw = await postAxon(config, "/v1/sources", {
      source: url,
      scope: "page",
      embed: false,
      execution: FOREGROUND_EXECUTION
    });
    const markdown = scrapeMarkdownFromRaw(raw);
    if (!markdown) {
      throw new Error("Scrape response did not include markdown.");
    }
    await copyTextOffscreen(markdown);
    await chrome.storage.local.set({
      lastContextAction: {
        action: "scrape",
        ok: true,
        url,
        copied_chars: markdown.length,
        at: new Date().toISOString()
      }
    });
    await setContextBadge(tab, "CPY", "#247a6b");
    await notifyContextAction("Axon copied markdown", `${formatUrlHost(url)} scraped and copied to clipboard.`);
  } catch (error) {
    await chrome.storage.local.set({
      lastContextAction: {
        action: "scrape",
        ok: false,
        url,
        at: new Date().toISOString(),
        error: error instanceof Error ? error.message : String(error)
      }
    });
    await flashContextError(tab);
    await notifyContextAction("Axon scrape failed", error instanceof Error ? error.message : String(error));
  }
}

async function crawlFromContext(url, tab) {
  if (!isScrapableUrl(url)) {
    await flashContextError(tab);
    await notifyContextAction("Axon capture blocked", AxonRedact.blockedCaptureReason(url) || "Only http:// and https:// pages can be crawled.");
    return;
  }

  await setContextBadge(tab, "CRL", "#c96a1c");
  try {
    const config = await loadConfig();
    // Context-menu clicks are a real user gesture, so this may prompt.
    await ensureAxonServerPermissionForGesture(config.axonUrl);
    const raw = await postAxon(config, "/v1/sources", { source: url, scope: "site" });
    await chrome.storage.local.set({
      lastContextAction: {
        action: "crawl",
        ok: true,
        url,
        job_id: raw?.job_id || raw?.jobId || raw?.id || raw?.payload?.job_id || "",
        at: new Date().toISOString()
      }
    });
    await setContextBadge(tab, "JOB", "#247a6b");
    await notifyContextAction("Axon crawl queued", `${formatUrlHost(url)} is crawling${raw?.job_id ? ` (${raw.job_id})` : ""}.`);
  } catch (error) {
    await chrome.storage.local.set({
      lastContextAction: {
        action: "crawl",
        ok: false,
        url,
        at: new Date().toISOString(),
        error: error instanceof Error ? error.message : String(error)
      }
    });
    await flashContextError(tab);
    await notifyContextAction("Axon crawl failed", error instanceof Error ? error.message : String(error));
  }
}

function scrapeMarkdownFromRaw(raw) {
  const content = raw && typeof raw === "object" ? raw.inline?.content : null;
  if (content && content.kind === "inline_text") {
    return content.text;
  }
  const payload = raw && typeof raw === "object" && raw.payload && typeof raw.payload === "object" ? raw.payload : {};
  return raw?.markdown || payload.markdown || raw?.content || payload.content || raw?.output || "";
}

async function copyTextOffscreen(text) {
  if (!chrome.offscreen?.createDocument) {
    throw new Error("Chrome offscreen documents are required for background clipboard writes.");
  }

  if (!chrome.offscreen.hasDocument || !(await chrome.offscreen.hasDocument())) {
    try {
      await chrome.offscreen.createDocument({
        url: chrome.runtime.getURL("offscreen.html"),
        reasons: [chrome.offscreen.Reason.CLIPBOARD],
        justification: "Copy scraped markdown from the context menu."
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (!/Only a single offscreen document/i.test(message)) {
        throw error;
      }
    }
  }

  const response = await chrome.runtime.sendMessage({
    target: "axon-offscreen",
    type: "copy-text",
    text
  });
  if (!response?.ok) {
    throw new Error(response?.error || "Clipboard copy failed.");
  }
}

async function setContextBadge(tab, text, color) {
  if (tab?.id != null) {
    await flashBadge(tab.id, text, color);
  }
}

async function flashContextError(tab) {
  await setContextBadge(tab, "ERR", "#8f4b5c");
}

async function notifyContextAction(title, message) {
  if (!chrome.notifications?.create) {
    return;
  }
  try {
    await chrome.notifications.create({
      type: "basic",
      iconUrl: chrome.runtime.getURL("assets/png/axon-icon-128.png"),
      title,
      message: String(message || "").slice(0, 240)
    });
  } catch {
    // Badge + lastContextAction are still available if notifications are blocked.
  }
}

function formatUrlHost(url) {
  try {
    return new URL(url).host;
  } catch {
    return "Page";
  }
}
