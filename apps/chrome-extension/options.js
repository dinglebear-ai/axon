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
    await requestAuthProbe();
    setApiStatus("Online", "success");
    setStatus("Axon API reachable and token accepted.");
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

async function requestAuthProbe() {
  const server = axonUrlInput.value.trim().replace(/\/+$/, "");
  const token = axonTokenInput.value.trim();

  if (!token && !isLoopbackServer(server)) {
    throw new Error("Bearer token is required for this Axon server.");
  }

  const headers = { "Content-Type": "application/json" };
  if (token) {
    headers.Authorization = `Bearer ${token}`;
  }

  const response = await fetch(`${server}/v1/sources`, {
    method: "POST",
    headers,
    body: JSON.stringify({ source: "" })
  });

  const body = await response.text();
  if (isExpectedSourceProbeResponse(response, body)) {
    return;
  }

  throw new Error(`${response.status} ${response.statusText}${body ? `: ${body}` : ""}`);
}

function isExpectedSourceProbeResponse(response, body) {
  if (response.status !== 400) {
    return false;
  }
  try {
    const payload = JSON.parse(body);
    return payload?.error?.code === "route.validation.missing_field" && payload?.error?.message === "source is required";
  } catch {
    return false;
  }
}

function isLoopbackServer(server) {
  try {
    const hostname = new URL(server).hostname;
    return hostname === "127.0.0.1" || hostname === "localhost" || hostname === "::1";
  } catch {
    return false;
  }
}

function setStatus(message) {
  statusText.textContent = message;
}

function setApiStatus(message, tone = "neutral") {
  apiStatusText.textContent = message;
  apiStatusText.className = `header-badge tone-${tone}`;
}
