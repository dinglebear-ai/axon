const DEFAULT_AXON_URL = "http://100.88.16.79:8001";
const AUTO_SCRAPE_HISTORY_KEY = "autoScrapeHistory";
const AUTO_SCRAPE_COOLDOWN_MS = 24 * 60 * 60 * 1000;

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

  await markAutoScrapeAttempt(urlKey, url, null);
  await setBadge(tabId, "SCR", "#0e7490");

  try {
    await postAxon(config, "/v1/scrape", { url });
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
}

function isScrapableUrl(url) {
  return /^https?:\/\//i.test(url);
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

// Right-click menus → open the side panel and forward a pre-filled action
// intent ({ op, arg }) that the launcher runs. Mirrors the design handoff
// ("Scrape with Axon", "Ingest this page", "Ask Axon about <selection>").
function setupContextMenus() {
  if (!chrome.contextMenus?.create) {
    return;
  }
  chrome.contextMenus.removeAll(() => {
    chrome.contextMenus.create({ id: "axon-scrape", title: "Scrape with Axon", contexts: ["page", "link"] });
    chrome.contextMenus.create({ id: "axon-ingest", title: "Ingest this page into Axon", contexts: ["page"] });
    chrome.contextMenus.create({ id: "axon-ask", title: 'Ask Axon about "%s"', contexts: ["selection"] });
  });
}

chrome.contextMenus?.onClicked?.addListener(async (info, tab) => {
  const intent =
    info.menuItemId === "axon-scrape" ? { op: "scrape", arg: info.linkUrl || info.pageUrl || tab?.url || "" }
    : info.menuItemId === "axon-ingest" ? { op: "ingest", arg: info.pageUrl || tab?.url || "" }
    : info.menuItemId === "axon-ask" ? { op: "ask", arg: info.selectionText || "" }
    : null;

  if (!intent) {
    return;
  }

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
