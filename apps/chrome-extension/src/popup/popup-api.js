// host-permissions.js declares `AxonHostPermissions` as a bare top-level
// `const` (see popup.html) — classic-script `const`/`let` share the global
// lexical scope but are never installed as `window` properties, so it's
// referenced directly here, guarded the same way background.js does.
async function hasAxonServerPermission(serverUrl) {
  if (typeof AxonHostPermissions === "undefined") {
    return true;
  }
  return AxonHostPermissions.hasAxonHostPermission(serverUrl);
}

// Only call from a real user gesture — the command-send button/Enter
// keypress and the "Check API" click/keydown handlers count; the automatic
// checkApi() at popup init does not, so it only ever checks (never prompts).
async function ensureAxonServerPermissionForGesture(serverUrl) {
  if (typeof AxonHostPermissions === "undefined") {
    return true;
  }
  if (await AxonHostPermissions.hasAxonHostPermission(serverUrl)) {
    return true;
  }
  const granted = await AxonHostPermissions.requestAxonHostPermission(serverUrl);
  if (!granted) {
    throw new Error(`Axon needs permission for ${serverUrl}. Grant it when Chrome prompts, or open Settings.`);
  }
  return true;
}

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

// The extension submits sources to the unified `/v1/sources` pipeline rather
// than orchestrating scrape/crawl/embed/ingest itself — those legacy routes
// were removed server-side and fold into source requests (see
// docs/pipeline-unification/surfaces/chrome-extension-contract.md).
//
// `SourceRequest.execution` has no per-field defaults once the key is
// present, so a synchronous ("foreground") request must spell out the whole
// policy rather than just `{ mode: "foreground" }`.
const FOREGROUND_EXECUTION = { mode: "foreground", priority: "normal", detached: false, heartbeat_interval_secs: 5 };

function markdownFromSourceResult(raw, fallbackUrl) {
  const content = raw && typeof raw === "object" ? raw.inline?.content : null;
  const markdown = content && content.kind === "inline_text" ? content.text : "";
  return { markdown, url: raw?.canonical_uri || fallbackUrl, job_id: raw?.job_id, source_id: raw?.source_id };
}

async function scrapeWithAxon(urls) {
  const targets = Array.isArray(urls) ? urls : [urls];
  const results = await Promise.all(targets.map(async (url) => {
    const raw = await postAxon("/v1/sources", { source: url, scope: "page", execution: FOREGROUND_EXECUTION });
    return markdownFromSourceResult(raw, url);
  }));
  return results.length === 1 ? results[0] : { results, urls: targets };
}

async function startCrawlWithAxon(urls, flags = {}) {
  const targets = Array.isArray(urls) ? urls : [urls];
  const limits = {};
  if (flags.maxPages !== undefined) limits.max_pages = flags.maxPages;
  if (flags.maxDepth !== undefined) limits.max_depth = flags.maxDepth;
  const values = {};
  if (flags.includeSubdomains !== undefined) values.include_subdomains = String(flags.includeSubdomains);
  if (flags.respectRobots !== undefined) values.respect_robots = String(flags.respectRobots);
  if (flags.discoverSitemaps !== undefined) values.discover_sitemaps = String(flags.discoverSitemaps);
  if (flags.sitemapSinceDays !== undefined) values.sitemap_since_days = String(flags.sitemapSinceDays);
  if (flags.renderMode !== undefined) values.render_mode = String(flags.renderMode);
  if (flags.delayMs !== undefined) values.delay_ms = String(flags.delayMs);

  // The removed `/v1/crawl` route accepted one batch job for many URLs; the
  // unified source pipeline is one job per source. Fire one request per URL
  // and surface the first job for progress tracking (matches prior UI, which
  // only ever tracked a single crawl job at a time).
  const results = await Promise.all(targets.map((url) => postAxon("/v1/sources", {
    source: url,
    scope: "site",
    limits,
    options: { values }
  })));
  return results.length === 1 ? results[0] : { ...results[0], results };
}

async function startEmbedWithAxon(input) {
  return postAxon("/v1/sources", { source: input });
}

async function startIngestWithAxon(args) {
  const parsed = parseCliArgs(args);
  const target = parsed.positionals.join(" ").trim();
  if (!target) {
    throw new Error("ingest requires a target.");
  }
  // IngestRequest.source_type is adapter-selected server-side — there is no
  // client-settable override, so a `--type`/`--sourceType` flag is dropped
  // rather than sent as a dead option key (matches launcher.js's ingest case).
  const values = {};
  values.include_source = String(parsed.flags.includeSource !== false);
  return postAxon("/v1/sources", { source: target, options: { values } });
}

async function cancelCrawlWithAxon(jobId) {
  const result = await postAxon(`/v1/jobs/${encodeURIComponent(jobId)}/cancel`, {});
  // JobCancelResult (crates/axon-api/src/source/job.rs) has no `canceled`
  // boolean field — derive it from the LifecycleStatus the cancel landed on.
  // "canceling" means the cancel was accepted for a running job (it finishes
  // asynchronously); "canceled" means it was already terminal (queued/pending).
  const status = result?.status ?? result?.payload?.status;
  const canceled = status === "canceled" || status === "canceling";
  return { ...result, canceled };
}

async function retrieveWithAxon(url) {
  return postAxon("/v1/retrieve", { url });
}

function withOptionalPaging(body, flags = {}) {
  if (flags.limit !== undefined) body.limit = flags.limit;
  if (flags.offset !== undefined) body.offset = flags.offset;
  if (flags.timeRange !== undefined) body.time_range = flags.timeRange;
  return body;
}

async function queryWithAxon(query, flags = {}) {
  return postAxon("/v1/query", withOptionalPaging({ query }, flags));
}

async function searchWithAxon(query, flags = {}) {
  return postAxon("/v1/search", withOptionalPaging({ query }, flags));
}

async function researchWithAxon(query, flags = {}) {
  return postAxon("/v1/research", withOptionalPaging({ query }, flags));
}

async function askWithAxon(url, question) {
  // Blocked-scheme tabs (chrome://, file://, ...) never leave the browser as
  // page context; the question text is still redacted defensively since it
  // may contain pasted secrets.
  const safeUrl = url && !AxonRedact.isBlockedCaptureUrl(url) ? AxonRedact.redactUrl(url) : "";
  const safeQuestion = AxonRedact.redactText(question || "").text;
  return postAxon("/v1/ask", {
    query: safeUrl ? `${safeQuestion}\n\nCurrent page URL: ${safeUrl}` : safeQuestion,
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
    // "Check API" is bound to a click/keydown handler, so this may prompt.
    const config = await loadConfig();
    await ensureAxonServerPermissionForGesture(config.axonUrl.trim().replace(/\/+$/, ""));
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

  if (!(await hasAxonServerPermission(server))) {
    throw new Error(`Axon needs permission for ${server}. Open Settings and run a command to grant it.`);
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

async function pollCrawlStatus(jobId, pollRun) {
  for (let attempt = 0; attempt < 120; attempt += 1) {
    await delay(attempt === 0 ? 750 : 2000);

    if (pollRun !== crawlPollRun) {
      return;
    }

    try {
      const result = await getAxon(`/v1/jobs/${encodeURIComponent(jobId)}`);
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

  if (["completed", "completed_degraded", "failed", "canceled", "cancelled", "expired"].includes(status)) {
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
