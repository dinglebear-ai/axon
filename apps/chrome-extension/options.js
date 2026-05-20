const DEFAULT_AXON_URL = "http://100.88.16.79:8001";

const axonUrlInput = document.querySelector("#axon-url");
const axonTokenInput = document.querySelector("#axon-token");
const autoScrapeInput = document.querySelector("#auto-scrape-enabled");
const saveButton = document.querySelector("#save-options");
const checkApiButton = document.querySelector("#check-api");
const statusText = document.querySelector("#status");
const apiStatusText = document.querySelector("#api-status");

init();

async function init() {
  await loadConfig();
  saveButton.addEventListener("click", saveConfig);
  checkApiButton.addEventListener("click", checkApi);
}

async function loadConfig() {
  if (!globalThis.chrome?.storage?.local) {
    return;
  }

  const stored = await chrome.storage.local.get(["axonUrl", "axonToken", "autoScrapeEnabled"]);
  axonUrlInput.value = stored.axonUrl || DEFAULT_AXON_URL;
  axonTokenInput.value = stored.axonToken || "";
  autoScrapeInput.checked = stored.autoScrapeEnabled === true;
}

async function saveConfig() {
  if (!globalThis.chrome?.storage?.local) {
    setStatus("Chrome storage is unavailable.");
    return;
  }

  const axonUrl = axonUrlInput.value.trim() || DEFAULT_AXON_URL;
  const axonToken = axonTokenInput.value.trim();
  const autoScrapeEnabled = autoScrapeInput.checked;

  await chrome.storage.local.set({ axonUrl, axonToken, autoScrapeEnabled });
  axonUrlInput.value = axonUrl;
  setStatus("Settings saved.");
}

async function checkApi() {
  setApiStatus("Checking", "info");
  checkApiButton.disabled = true;

  try {
    await saveConfig();
    await requestHealth();
    setApiStatus("Online", "success");
    setStatus("Axon API reachable.");
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setApiStatus("Offline", "error");
    setStatus(`Axon API check failed: ${message}`);
  } finally {
    checkApiButton.disabled = false;
  }
}

async function requestHealth() {
  const server = axonUrlInput.value.trim().replace(/\/+$/, "");
  const token = axonTokenInput.value.trim();
  const headers = {};

  if (!server) {
    throw new Error("Axon server URL is required.");
  }

  if (token) {
    headers.Authorization = `Bearer ${token}`;
  }

  const response = await fetch(`${server}/healthz`, { headers });

  if (!response.ok) {
    const body = await response.text();
    throw new Error(`${response.status} ${response.statusText}${body ? `: ${body}` : ""}`);
  }
}

function setStatus(message) {
  statusText.textContent = message;
}

function setApiStatus(message, tone = "neutral") {
  apiStatusText.textContent = message;
  apiStatusText.className = `header-badge tone-${tone}`;
}
