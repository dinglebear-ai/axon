const axonUrlInput = document.querySelector("#axon-url");
const axonTokenInput = document.querySelector("#axon-token");
const copyAxonButton = document.querySelector("#copy-axon");
const checkApiButton = document.querySelector("#check-api");
const openSidebarButton = document.querySelector("#open-sidebar");
const summarizeButton = document.querySelector("#summarize-page");
const mapButton = document.querySelector("#map-page");
const copyDomButton = document.querySelector("#copy-dom");
const askAxonButton = document.querySelector("#ask-axon");
const askQuestionInput = document.querySelector("#ask-question");
const preview = document.querySelector("#preview");
const statusText = document.querySelector("#status");
const apiStatusText = document.querySelector("#api-status");
const crawlStatusText = document.querySelector("#crawl-status");
const crawlJobText = document.querySelector("#crawl-job");
const cancelCrawlButton = document.querySelector("#cancel-crawl");

let crawlPollRun = 0;
let currentCrawlJobId = "";

init();

async function init() {
  await loadConfig();
  axonUrlInput.addEventListener("change", saveConfig);
  axonTokenInput.addEventListener("change", saveConfig);
  checkApiButton.addEventListener("click", checkApi);
  openSidebarButton?.addEventListener("click", openSidebar);
  cancelCrawlButton.addEventListener("click", cancelCurrentCrawl);
  checkApi();
}

copyAxonButton.addEventListener("click", async () => {
  setBusy(true, "Capturing current tab through Axon...");
  setCrawlStatus("Idle", "", "neutral");
  const pollRun = ++crawlPollRun;

  try {
    const tab = await activeTab();
    const result = await scrapeWithAxon(tab.url);
    const crawl = await startCrawlWithAxon(tab.url);
    const text = formatAxonScrape(result, tab);
    preview.value = text;
    await writeClipboard(text);

    const words = text.trim().split(/\s+/).filter(Boolean).length;
    const jobId = crawl.job_id || crawl.jobId || crawl.id;
    setCrawlStatus("Queued", jobId || "", "info");
    setStatus(`Copied ${words.toLocaleString()} words from Axon.`);

    if (jobId) {
      pollCrawlStatus(jobId, pollRun);
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setStatus(`Axon scrape failed: ${message}`);
    setCrawlStatus("Not queued", "", "error");
  } finally {
    setBusy(false);
  }
});

async function openSidebar() {
  if (!chrome.sidePanel?.open) {
    setStatus("Chrome side panel API is unavailable in this browser.");
    return;
  }

  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    if (tab?.id && /^https?:\/\//i.test(tab.url || "")) {
      await chrome.sidePanel.open({ tabId: tab.id });
    } else {
      const currentWindow = await chrome.windows.getCurrent();
      await chrome.sidePanel.open({ windowId: currentWindow.id });
    }
    setStatus("Opened Axon in the Chrome side panel.");
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setStatus(`Open sidebar failed: ${message}`);
  }
}

async function cancelCurrentCrawl() {
  if (!currentCrawlJobId) {
    return;
  }

  const jobId = currentCrawlJobId;
  const pollRun = ++crawlPollRun;
  setCrawlStatus("Canceling", jobId, "warn");
  setStatus(`Canceling crawl ${jobId}...`);

  try {
    const result = await cancelCrawlWithAxon(jobId);
    const canceled = result.canceled ?? result.payload?.canceled ?? false;
    setCrawlStatus(canceled ? "Canceled" : "Not cancellable", jobId, canceled ? "warn" : "neutral");
    setStatus(canceled ? `Canceled crawl ${jobId}.` : `No cancellable crawl found for ${jobId}.`);

    if (!canceled) {
      pollCrawlStatus(jobId, pollRun);
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setCrawlStatus("Cancel failed", jobId, "error");
    setStatus(`Cancel failed: ${message}`);
  }
}

summarizeButton.addEventListener("click", async () => {
  setBusy(true, "Summarizing current page through Axon...");

  try {
    const tab = await activeTab();
    const result = await summarizeWithAxon(tab.url);
    const text = formatSummary(result, tab);
    preview.value = text;
    await writeClipboard(text);
    setStatus("Copied Axon summary to clipboard.");
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setStatus(`Summarize failed: ${message}`);
  } finally {
    setBusy(false);
  }
});

mapButton.addEventListener("click", async () => {
  setBusy(true, "Mapping URLs through Axon...");

  try {
    const tab = await activeTab();
    const result = await mapWithAxon(tab.url);
    const text = formatMap(result, tab);
    preview.value = text;
    await writeClipboard(text);
    setStatus(`Copied ${result.urls?.length || 0} mapped URLs to clipboard.`);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setStatus(`Map failed: ${message}`);
  } finally {
    setBusy(false);
  }
});

askAxonButton.addEventListener("click", async () => {
  setBusy(true, "Asking Axon...");

  try {
    const tab = await activeTab();
    const question = askQuestionInput.value.trim();

    if (!question) {
      throw new Error("Ask question is required.");
    }

    const result = await askWithAxon(tab.url, question);
    const text = formatAsk(result, question, tab);
    preview.value = text;
    await writeClipboard(text);
    setStatus("Copied Axon answer to clipboard.");
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setStatus(`Ask failed: ${message}`);
  } finally {
    setBusy(false);
  }
});

copyDomButton.addEventListener("click", async () => {
  setBusy(true, "Copying visible page text...");

  try {
    const tab = await activeTab();
    const [{ result }] = await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      func: scrapeCurrentPage
    });

    const text = formatDomScrape(result);
    preview.value = text;
    await writeClipboard(text);

    const words = text.trim().split(/\s+/).filter(Boolean).length;
    setStatus(`Copied ${words.toLocaleString()} words from the visible page. This bypasses Axon.`);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setStatus(`DOM scrape failed: ${message}`);
  } finally {
    setBusy(false);
  }
});

async function activeTab() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });

  if (!tab?.id || !tab.url) {
    throw new Error("No active tab found.");
  }

  if (!/^https?:\/\//i.test(tab.url)) {
    throw new Error("Axon can only scrape http:// or https:// pages.");
  }

  return tab;
}

async function scrapeWithAxon(url) {
  return postAxon("/v1/scrape", { url });
}

async function startCrawlWithAxon(url) {
  return postAxon("/v1/crawl", { urls: [url] });
}

async function cancelCrawlWithAxon(jobId) {
  return postAxon(`/v1/crawl/${encodeURIComponent(jobId)}/cancel`, {});
}

async function summarizeWithAxon(url) {
  return postAxon("/v1/summarize", { urls: [url] });
}

async function mapWithAxon(url) {
  return postAxon("/v1/map", { url, limit: 100 });
}

async function askWithAxon(url, question) {
  return postAxon("/v1/ask", {
    query: `${question}\n\nCurrent page URL: ${url}`,
    explain: false,
    diagnostics: false
  });
}

async function postAxon(path, body) {
  return requestAxon("POST", path, body);
}

async function getAxon(path, options) {
  return requestAxon("GET", path, options);
}

async function checkApi() {
  setApiStatus("Checking", "info");

  try {
    await getAxon("/healthz", { parseJson: false });
    setApiStatus("Online", "success");
    setStatus("Axon API reachable.");
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setApiStatus("Offline", "error");
    setStatus(`Axon API check failed: ${message}`);
  }
}

async function requestAxon(method, path, body) {
  const options = typeof body === "object" && body?.parseJson === false ? body : {};
  const requestBody = options.parseJson === false ? undefined : body;
  const server = axonUrlInput.value.trim().replace(/\/+$/, "");
  const token = axonTokenInput.value.trim();
  const headers = { "Content-Type": "application/json" };

  if (!server) {
    throw new Error("Axon server URL is required.");
  }

  if (token) {
    headers.Authorization = `Bearer ${token}`;
  }

  const response = await fetch(`${server}${path}`, {
    method,
    headers,
    body: requestBody ? JSON.stringify(requestBody) : undefined
  });

  if (!response.ok) {
    const body = await response.text();
    throw new Error(`${response.status} ${response.statusText}${body ? `: ${body}` : ""}`);
  }

  if (options.parseJson === false) {
    return response.text();
  }

  return response.json();
}

async function pollCrawlStatus(jobId, pollRun) {
  for (let attempt = 0; attempt < 120; attempt += 1) {
    await delay(attempt === 0 ? 750 : 2000);

    if (pollRun !== crawlPollRun) {
      return;
    }

    try {
      const result = await getAxon(`/v1/crawl/${encodeURIComponent(jobId)}`);
      const status = crawlStatus(result);
      setCrawlStatus(status.label, jobId, status.tone);

      if (status.done) {
        return;
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setCrawlStatus(`Status unavailable`, jobId, "warn");
      setStatus(`Crawl status unavailable: ${message}`);
      return;
    }
  }

  setCrawlStatus("Still running", jobId, "info");
}

function crawlStatus(result) {
  const job = result.job || result.payload?.job || result;
  const rawStatus = job.status || result.status || result.payload?.status || result.state || "unknown";
  const status = String(rawStatus).toLowerCase();
  const resultJson = job.result_json || result.result_json || result.payload?.result_json || {};
  const pages = resultJson.pages_crawled;
  const suffix = pages ? ` (${pages} pages)` : "";

  if (["completed", "failed", "canceled", "cancelled"].includes(status)) {
    return { label: `${capitalize(status)}${suffix}`, tone: statusTone(status), done: true };
  }

  return { label: `${capitalize(status)}${suffix}`, tone: statusTone(status), done: false };
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function scrapeCurrentPage() {
  const selection = window.getSelection()?.toString().trim() ?? "";
  const article =
    document.querySelector("article") ||
    document.querySelector("main") ||
    document.body;
  const text = normalizeText((selection || article?.innerText || document.body?.innerText || "").trim());
  const description =
    document.querySelector('meta[name="description"]')?.getAttribute("content")?.trim() ||
    document.querySelector('meta[property="og:description"]')?.getAttribute("content")?.trim() ||
    "";

  return {
    title: document.title || "",
    url: location.href,
    description,
    text
  };
}

function formatAxonScrape(result, tab) {
  const markdown = result.markdown || result.payload?.markdown || result.output || "";
  const url = result.url || tab.url;

  return [
    `# ${tab.title || result.payload?.title || "Untitled page"}`,
    "",
    `URL: ${url}`,
    "",
    markdown || "(Axon returned no markdown.)"
  ].join("\n");
}

function formatSummary(result, tab) {
  const summary = result.summary || result.payload?.summary || "";
  const urls = result.urls || result.payload?.urls || [tab.url];
  const contextChars = result.context_chars || result.payload?.context_chars;

  return [
    `# Summary: ${tab.title || "Untitled page"}`,
    "",
    `URL: ${urls[0] || tab.url}`,
    contextChars ? `Context: ${contextChars.toLocaleString()} chars` : "",
    "",
    summary || "(Axon returned no summary.)"
  ].filter(Boolean).join("\n");
}

function formatMap(result, tab) {
  const urls = result.urls || [];
  const total = result.total ?? urls.length;
  const source = result.map_source || "unknown";

  return [
    `# URL map: ${tab.title || "Untitled page"}`,
    "",
    `Start URL: ${result.url || tab.url}`,
    `Discovered: ${total.toLocaleString()} URLs`,
    `Source: ${source}`,
    result.warning ? `Warning: ${result.warning}` : "",
    "",
    ...urls
  ].filter(Boolean).join("\n");
}

function formatAsk(result, question, tab) {
  return [
    `# Ask: ${question}`,
    "",
    `Current page: ${tab.url}`,
    "",
    result.answer || result.payload?.answer || "(Axon returned no answer.)"
  ].join("\n");
}

function formatDomScrape(page) {
  const parts = [
    `# ${page.title || "Untitled page"}`,
    "",
    `URL: ${page.url}`
  ];

  if (page.description) {
    parts.push("", page.description);
  }

  parts.push("", page.text || "(No readable page text found.)");
  return parts.join("\n");
}

async function writeClipboard(text) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }

  preview.value = text;
  preview.focus();
  preview.select();

  if (!document.execCommand("copy")) {
    throw new Error("Clipboard write failed.");
  }
}

function normalizeText(text) {
  return text
    .replace(/\u00a0/g, " ")
    .replace(/[ \t]+/g, " ")
    .replace(/\n{3,}/g, "\n\n");
}

function setBusy(isBusy, message) {
  axonUrlInput.disabled = isBusy;
  axonTokenInput.disabled = isBusy;
  checkApiButton.disabled = isBusy;
  if (openSidebarButton) {
    openSidebarButton.disabled = isBusy;
  }
  copyAxonButton.disabled = isBusy;
  summarizeButton.disabled = isBusy;
  mapButton.disabled = isBusy;
  copyDomButton.disabled = isBusy;
  askAxonButton.disabled = isBusy;
  askQuestionInput.disabled = isBusy;
  if (message) {
    setStatus(message);
  }
}

function setStatus(message) {
  statusText.textContent = message;
}

function setApiStatus(message, tone = "neutral") {
  apiStatusText.textContent = message;
  apiStatusText.className = `header-badge tone-${tone}`;
}

function setCrawlStatus(message, jobId, tone = "neutral") {
  const normalized = message.toLowerCase();
  currentCrawlJobId = jobId || "";
  crawlStatusText.textContent = message;
  crawlStatusText.className = `status-badge tone-${tone}`;
  crawlJobText.textContent = jobId ? `Job ${jobId}` : "";
  crawlJobText.title = jobId || "";
  cancelCrawlButton.disabled =
    !jobId ||
    normalized.startsWith("completed") ||
    normalized.startsWith("failed") ||
    normalized.startsWith("canceled") ||
    normalized.startsWith("cancelled") ||
    normalized.startsWith("not cancellable");
}

function capitalize(value) {
  return value ? value.charAt(0).toUpperCase() + value.slice(1) : value;
}

function statusTone(status) {
  if (status === "completed") {
    return "success";
  }
  if (status === "failed") {
    return "error";
  }
  if (status === "canceled" || status === "cancelled") {
    return "warn";
  }
  if (status === "running" || status === "pending" || status === "queued") {
    return "info";
  }
  return "neutral";
}

async function loadConfig() {
  if (!globalThis.chrome?.storage?.local) {
    return;
  }

  const stored = await chrome.storage.local.get(["axonUrl", "axonToken"]);
  if (stored.axonUrl) {
    axonUrlInput.value = stored.axonUrl;
  }
  if (stored.axonToken) {
    axonTokenInput.value = stored.axonToken;
  }
}

async function saveConfig() {
  if (!globalThis.chrome?.storage?.local) {
    return;
  }

  await chrome.storage.local.set({
    axonUrl: axonUrlInput.value.trim(),
    axonToken: axonTokenInput.value.trim()
  });
}
