const DEFAULT_AXON_URL = "http://100.88.16.79:8001";
const AUTO_SCRAPE_HISTORY_KEY = "autoScrapeHistory";
const AUTO_SCRAPE_COOLDOWN_MS = 24 * 60 * 60 * 1000;

chrome.runtime.onInstalled.addListener(() => {
  chrome.action.setBadgeBackgroundColor({ color: "#24536c" });
  setupSidePanelAction();
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
  if (!previous?.at) {
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
