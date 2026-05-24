async function setWatchFromCommand(arg) {
  const args = splitArgs(arg || "");
  const value = (args[0] || "").toLowerCase();

  if (value === "list") {
    const result = await getAxon("/v1/watch");
    const output = formatGenericResult("watch list", result);
    return {
      output,
      copyText: output,
      copiedMessage: "Copied watch list to clipboard.",
      doneMessage: "Watch list loaded."
    };
  }

  if (value === "run" && args[1]) {
    const result = await postAxon(`/v1/watch/${encodeURIComponent(args[1])}/run`, {});
    const output = formatGenericResult("watch run", result);
    return {
      output,
      copyText: output,
      copiedMessage: "Copied watch run result to clipboard.",
      doneMessage: "Watch run complete."
    };
  }

  if (!["on", "off", "toggle", ""].includes(value)) {
    throw new Error("Use `watch on`, `watch off`, `watch toggle`, `watch list`, or `watch run <id>`.");
  }

  const stored = await chrome.storage.local.get(["autoScrapeEnabled"]);
  const enabled = value === "on" ? true : value === "off" ? false : stored.autoScrapeEnabled !== true;
  await chrome.storage.local.set({ autoScrapeEnabled: enabled });
  await refreshAutomationStatus();
  return {
    output: `Auto-scrape is now ${enabled ? "on" : "off"}.`,
    doneMessage: `Auto-scrape ${enabled ? "enabled" : "paused"}.`
  };
}

async function describeConfig() {
  const config = await loadConfig();
  const stored = await chrome.storage.local.get(["autoScrapeEnabled", "lastAutoScrape"]);
  const output = [
    "Axon extension config",
    "",
    `Server: ${config.axonUrl}`,
    `Token: ${config.axonToken ? `configured (${config.axonToken.length} chars)` : "missing"}`,
    `Auto-scrape: ${stored.autoScrapeEnabled === true ? "on" : "off"}`,
    stored.lastAutoScrape ? `Last scrape: ${stored.lastAutoScrape.ok ? "ok" : "failed"} ${stored.lastAutoScrape.url || ""}` : "Last scrape: none"
  ].join("\n");

  return {
    output,
    copyText: output,
    copiedMessage: "Copied extension config to clipboard.",
    doneMessage: "Config displayed."
  };
}

function isWebTab(tab) {
  return /^https?:\/\//i.test(tab?.url || "");
}

function mostRecentWebTab(tabs) {
  return tabs
    .filter(isWebTab)
    .sort((left, right) => (right.lastAccessed || 0) - (left.lastAccessed || 0))[0];
}

async function scrapeWithAxon(urls) {
  const targets = Array.isArray(urls) ? urls : [urls];
  const results = await Promise.all(targets.map((url) => postAxon("/v1/scrape", { url })));
  return results.length === 1 ? results[0] : { results, urls: targets };
}

async function startCrawlWithAxon(urls, flags = {}) {
  return postAxon("/v1/crawl", {
    urls,
    max_pages: flags.maxPages,
    max_depth: flags.maxDepth,
    include_subdomains: flags.includeSubdomains,
    respect_robots: flags.respectRobots,
    discover_sitemaps: flags.discoverSitemaps,
    sitemap_since_days: flags.sitemapSinceDays,
    render_mode: flags.renderMode,
    delay_ms: flags.delayMs
  });
}

async function startExtractWithAxon(urls, flags = {}) {
  return postAxon("/v1/extract", {
    urls,
    prompt: flags.prompt || flags.query,
    max_pages: flags.maxPages
  });
}

async function startEmbedWithAxon(input) {
  return postAxon("/v1/embed", { input });
}

async function startIngestWithAxon(args) {
  const parsed = parseCliArgs(args);
  const target = parsed.positionals.join(" ").trim();
  if (!target) {
    throw new Error("ingest requires a target.");
  }
  return postAxon("/v1/ingest", {
    source_type: parsed.flags.sourceType || parsed.flags.type,
    target,
    include_source: parsed.flags.includeSource !== false
  });
}

async function startSessionsWithAxon(args) {
  const parsed = parseCliArgs(args);
  return postAxon("/v1/ingest", {
    source_type: "sessions",
    sessions: {
      claude: parsed.flags.claude !== false,
      codex: parsed.flags.codex !== false,
      gemini: parsed.flags.gemini === true,
      project: parsed.flags.project || parsed.positionals.join(" ").trim() || undefined
    }
  });
}

async function cancelCrawlWithAxon(jobId) {
  return postAxon(`/v1/crawl/${encodeURIComponent(jobId)}/cancel`, {});
}

async function summarizeWithAxon(urls) {
  return postAxon("/v1/summarize", { urls: Array.isArray(urls) ? urls : [urls] });
}

async function mapWithAxon(url) {
  return postAxon("/v1/map", { url, limit: 100 });
}

async function retrieveWithAxon(url) {
  return postAxon("/v1/retrieve", { url, max_points: 12, token_budget: 6000 });
}

async function queryWithAxon(query) {
  return postAxon("/v1/query", { query, limit: 8, offset: 0 });
}

async function searchWithAxon(query, flags = {}) {
  return postAxon("/v1/search", {
    query,
    limit: flags.limit || 10,
    offset: flags.offset || 0,
    time_range: flags.timeRange
  });
}

async function researchWithAxon(query, flags = {}) {
  return postAxon("/v1/research", {
    query,
    limit: flags.limit || 10,
    offset: flags.offset || 0,
    time_range: flags.timeRange
  });
}

async function evaluateWithAxon(question) {
  return postAxon("/v1/evaluate", { question });
}

async function suggestWithAxon(focus) {
  return postAxon("/v1/suggest", { focus: focus || undefined });
}

async function askWithAxon(url, question) {
  return postAxon("/v1/ask", {
    query: url ? `${question}\n\nCurrent page URL: ${url}` : question,
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
    await probeAxonAuth();
    setApiStatus("Online", "success");
    setStatus("Axon API reachable and token accepted.");
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setApiStatus("Offline", "error");
    setStatus(`Axon API check failed: ${message}`);
  }
}

async function probeAxonAuth() {
  const config = await loadConfig();
  const server = config.axonUrl.trim().replace(/\/+$/, "");
  const token = config.axonToken.trim();
  const headers = { "Content-Type": "application/json" };
  if (token) {
    headers.Authorization = `Bearer ${token}`;
  }
  if (!token && !isLoopbackServer(server)) {
    throw new Error("Missing bearer token. Open Settings or run `auth` to check extension config.");
  }

  const response = await fetch(`${server}/v1/scrape`, {
    method: "POST",
    headers,
    body: JSON.stringify({ url: "" })
  });
  const body = await response.text();
  if (isExpectedScrapeProbeResponse(response, body)) {
    return;
  }
  throw new Error(`${response.status} ${response.statusText}${body ? `: ${body}` : ""}`);
}

async function requestAxon(method, path, body) {
  const options = typeof body === "object" && body?.parseJson === false ? body : {};
  const requestBody = options.parseJson === false ? undefined : body;
  const config = await loadConfig();
  const server = config.axonUrl.trim().replace(/\/+$/, "");
  const token = config.axonToken.trim();
  const headers = { "Content-Type": "application/json" };

  if (!server) {
    throw new Error("Axon server URL is required.");
  }

  if (token) {
    headers.Authorization = `Bearer ${token}`;
  }

  if (!token && path !== "/healthz" && !isLoopbackServer(server)) {
    throw new Error("Missing bearer token. Open Settings or run `auth` to check extension config.");
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

function friendlyError(error) {
  const message = error instanceof Error ? error.message : String(error);
  const lower = message.toLowerCase();

  if (lower.includes("missing bearer token") || lower.includes("auth_failed") || lower.includes("401")) {
    return "Auth failed. The extension needs the Axon bearer token for this server. Run `auth` or open Settings.";
  }

  if (lower.includes("403") || lower.includes("forbidden")) {
    return "Forbidden by the Axon server or proxy. Check that the Axon URL points at the API listener and that this token is accepted.";
  }

  if (lower.includes("failed to fetch") || lower.includes("networkerror")) {
    return "Axon is unreachable from Chrome. Check the server URL, Tailscale route, and whether Axon is listening.";
  }

  return message.replace(/<!--[\s\S]*?-->/g, "").trim();
}

function isLoopbackServer(server) {
  try {
    const hostname = new URL(server).hostname;
    return hostname === "127.0.0.1" || hostname === "localhost" || hostname === "::1";
  } catch {
    return false;
  }
}

function isExpectedScrapeProbeResponse(response, body) {
  if (response.status !== 400) {
    return false;
  }
  try {
    const payload = JSON.parse(body);
    return payload?.kind === "bad_request" && payload?.message === "url or urls is required";
  } catch {
    return false;
  }
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
      if (currentCrawlOutputMessage) {
        updateChatMessage(currentCrawlOutputMessage, status.detail || status.label);
      }

      if (status.done) {
        currentCrawlOutputMessage = null;
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
  const errors = resultJson.error_count || resultJson.errors_count || resultJson.errors?.length;
  const suffix = pages ? ` (${pages} pages)` : "";

  if (["completed", "failed", "canceled", "cancelled"].includes(status)) {
    return { label: `${capitalize(status)}${suffix}`, detail: crawlDetail(result, status, pages, errors), tone: statusTone(status), done: true };
  }

  return { label: `${capitalize(status)}${suffix}`, detail: crawlDetail(result, status, pages, errors), tone: statusTone(status), done: false };
}

function crawlDetail(result, status, pages, errors) {
  return [
    `# Crawl`,
    "",
    `${badge(toneForStatus(status), status)}${pages ? ` ${pages.toLocaleString()} pages` : ""}`,
    errors ? `${badge("error", `${errors} errors`)}` : "",
    jobSummary(result)
  ].filter(Boolean).join("\n");
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function answerWithAxon(question, tab) {
  setChatStatus("Asking", "info");
  const result = await askWithAxon(tab?.url || "", question);
  const answer = {
    answer: result.answer || result.payload?.answer || "(Axon returned no answer.)"
  };
  setChatStatus("Axon", "success");
  return answer;
}
