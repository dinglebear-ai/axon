const DEFAULT_AXON_URL = "http://100.88.16.79:8001";

const openSidebarButton = document.querySelector("#open-sidebar");
const openOptionsButton = document.querySelector("#open-options");
const chatResetButton = document.querySelector("#chat-reset");
const chatLog = document.querySelector("#chat-log");
const chatStatusText = document.querySelector("#chat-status");
const commandInput = document.querySelector("#command-input");
const commandSendButton = document.querySelector("#command-send");
const commandStatusText = document.querySelector("#command-status");
const statusText = document.querySelector("#status");
const targetTitleText = document.querySelector("#target-title");
const targetUrlText = document.querySelector("#target-url");
const targetStateText = document.querySelector("#target-state");
const watchStatusText = document.querySelector("#watch-status");
const apiStatusText = document.querySelector("#api-status");
const crawlPanel = document.querySelector(".crawl-panel");
const crawlStatusText = document.querySelector("#crawl-status");
const crawlJobText = document.querySelector("#crawl-job");
const cancelCrawlButton = document.querySelector("#cancel-crawl");

let crawlPollRun = 0;
let currentCrawlJobId = "";
let currentCrawlOutputMessage = null;
let chatMessages = [];
let commandHistory = [];
let commandHistoryIndex = 0;
let persistOutputTimer = 0;

const COMMAND_HISTORY_KEY = "commandHistory";
const OUTPUT_LOG_KEY = "outputLog";
const OUTPUT_LIMIT = 30;

const COMMANDS = [
  { name: "ask", meta: "Ask Axon RAG", kind: "chat" },
  { name: "scrape", meta: "Scrape URL(s)", kind: "write" },
  { name: "crawl", meta: "Start a crawl job", kind: "job" },
  { name: "map", meta: "Discover URLs", kind: "read" },
  { name: "extract", meta: "Start extract job", kind: "job" },
  { name: "search", meta: "Web search and auto-crawl", kind: "write" },
  { name: "research", meta: "Research synthesis", kind: "write" },
  { name: "embed", meta: "Start embed job", kind: "job" },
  { name: "query", meta: "Search indexed sources", kind: "read" },
  { name: "retrieve", meta: "Retrieve indexed chunks", kind: "read" },
  { name: "summarize", meta: "Summarize URL(s)", kind: "write" },
  { name: "evaluate", meta: "Evaluate RAG answer", kind: "write" },
  { name: "suggest", meta: "Suggest URLs to crawl", kind: "write" },
  { name: "ingest", meta: "Start ingest job", kind: "job" },
  { name: "sessions", meta: "Ingest AI sessions", kind: "job" },
  { name: "sources", meta: "List indexed sources", kind: "read" },
  { name: "domains", meta: "List indexed domains", kind: "read" },
  { name: "stats", meta: "Show vector stats", kind: "read" },
  { name: "doctor", meta: "Diagnose services", kind: "read" },
  { name: "debug", meta: "CLI-only debug workflow", kind: "local" },
  { name: "screenshot", meta: "CLI-only screenshot capture", kind: "local" },
  { name: "status", meta: "Show current crawl job status" },
  { name: "dedupe", meta: "Deduplicate collection", kind: "write" },
  { name: "migrate", meta: "Migrate collection", kind: "write" },
  { name: "setup", meta: "CLI-only setup workflow", kind: "local" },
  { name: "mcp", meta: "CLI-only MCP server", kind: "local" },
  { name: "serve", meta: "CLI-only server process", kind: "local" },
  { name: "completions", meta: "CLI-only shell completions", kind: "local" },
  { name: "open", meta: "Navigate current tab to URL" },
  { name: "watch", meta: "Turn auto-scrape on or off" },
  { name: "auth", meta: "Show API auth state" },
  { name: "config", meta: "Show extension config" },
  { name: "clear", meta: "Clear output" }
];

const COMMAND_ALIASES = {
  a: "ask",
  s: "scrape",
  c: "crawl",
  r: "retrieve",
  q: "query",
  sum: "summarize",
  m: "map",
  d: "doctor",
  o: "open",
  w: "watch"
};

init();

async function init() {
  await restoreCommandHistory();
  openSidebarButton?.addEventListener("click", openSidebar);
  openOptionsButton.addEventListener("click", openOptions);
  apiStatusText.addEventListener("click", checkApi);
  apiStatusText.addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      checkApi();
    }
  });
  cancelCrawlButton.addEventListener("click", cancelCurrentCrawl);
  watchStatusText.addEventListener("click", toggleWatch);
  watchStatusText.addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      toggleWatch();
    }
  });
  chatResetButton.addEventListener("click", resetChat);
  commandSendButton.addEventListener("click", runCommandInput);
  commandInput.addEventListener("input", updateCommandHint);
  commandInput.addEventListener("keydown", (event) => {
    if (event.key === "Tab") {
      event.preventDefault();
      completeCommand();
      return;
    }

    if (event.key === "Enter") {
      event.preventDefault();
      runCommandInput();
      return;
    }

    if (event.key === "Escape") {
      if (commandInput.value) {
        commandInput.value = "";
        updateCommandHint();
        setStatus("Command cleared.");
      }
      return;
    }

    if (event.key === "ArrowUp") {
      event.preventDefault();
      recallCommand(-1);
      return;
    }

    if (event.key === "ArrowDown") {
      event.preventDefault();
      recallCommand(1);
    }
  });
  document.addEventListener("keydown", (event) => {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      commandInput.focus();
      commandInput.select();
    }
  });
  const restoredOutput = await restoreOutputLog();
  if (!restoredOutput) {
    appendChatMessage("assistant", "Ask anything, or type an Axon command like `scrape this`, `crawl https://example.com`, `query rust embeddings`, or `doctor`.");
  }
  refreshTargetContext();
  refreshAutomationStatus();
  chrome.tabs?.onActivated?.addListener(refreshTargetContext);
  chrome.tabs?.onUpdated?.addListener((_tabId, changeInfo) => {
    if (changeInfo.status === "complete" || changeInfo.url) {
      refreshTargetContext();
    }
  });
  chrome.storage?.onChanged?.addListener((changes, area) => {
    if (area === "local" && (changes.autoScrapeEnabled || changes.lastAutoScrape)) {
      refreshAutomationStatus();
    }
  });
  setChatStatus("Axon", "success");
  setCrawlStatus("Idle", "", "neutral");
  updateCommandHint();
  checkApi();
  setTimeout(() => commandInput.focus(), 40);
}

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
    setTimeout(() => window.close(), 80);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setStatus(`Open sidebar failed: ${message}`);
  }
}

function openOptions() {
  if (chrome.runtime?.openOptionsPage) {
    chrome.runtime.openOptionsPage();
    return;
  }

  window.open(chrome.runtime.getURL("options.html"));
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

async function runCommandInput() {
  const raw = commandInput.value.trim();
  if (!raw) {
    setStatus("Command is required.");
    return;
  }

  const command = parseCommand(raw);
  rememberCommand(raw);
  if (command.name === "clear") {
    commandInput.value = "";
    updateCommandHint();
    resetChat();
    commandInput.focus();
    return;
  }

  commandInput.value = "";
  updateCommandHint();
  const activity = command.isCommand ? `Running ${command.name}...` : "Asking Axon...";
  setBusy(true, activity);
  setCommandStatus(command.isCommand ? command.name : "chat", "info");
  appendChatMessage("user", command.isCommand ? `> ${command.raw}` : command.raw);
  const pending = appendChatMessage("assistant", activity);

  try {
    const tab = await activeTab().catch(() => null);
    refreshTargetContext();
    const result = await executeCommand(command, tab);
    updateChatMessage(pending, result.output);
    attachResultActions(pending, result);

    if (result.copyText) {
      const copied = await tryWriteClipboard(result.copyText);
      setStatus(copied ? result.copiedMessage : `${result.doneMessage} Focus the panel to copy.`);
    } else {
      setStatus(result.doneMessage);
    }

    setCommandStatus(command.name, "success");
    setChatStatus("Axon", "success");
    if (result.crawlJobId) {
      currentCrawlOutputMessage = pending;
    }
  } catch (error) {
    const message = friendlyError(error);
    setCommandStatus("Error", "error");
    setChatStatus("Error", "error");
    updateChatMessage(pending, `${capitalize(command.name)} failed: ${message}`);
    setStatus(`${capitalize(command.name)} failed: ${message}`);
  } finally {
    setBusy(false);
    commandInput.focus();
  }
}

function resetChat() {
  chatMessages = [];
  chatLog.textContent = "";
  appendChatMessage("assistant", "Output cleared. Continue the conversation or run an Axon command.");
  setStatus("Output cleared.");
}

function parseCommand(raw) {
  const trimmed = raw.trim();
  if (trimmed.startsWith("?")) {
    const arg = trimmed.slice(1).trim();
    return { name: "ask", args: [arg || trimmed], arg: arg || trimmed, raw, isCommand: false };
  }

  const commandText = trimmed.startsWith("/") ? trimmed.slice(1).trim() : trimmed;
  const parts = splitArgs(commandText);
  if (parts[0]?.toLowerCase() === "axon") {
    parts.shift();
  }
  const [head = "", ...rest] = parts;
  const normalizedHead = COMMAND_ALIASES[head.toLowerCase()] || head.toLowerCase();
  const exact = COMMANDS.find((command) => command.name === normalizedHead);

  if (!exact) {
    return { name: "ask", args: [raw], arg: raw, raw, isCommand: false };
  }

  return {
    name: exact.name,
    args: rest,
    arg: rest.join(" ").trim(),
    raw,
    isCommand: true
  };
}

function splitArgs(input) {
  const args = [];
  let current = "";
  let quote = "";
  let escaping = false;

  for (const char of input) {
    if (escaping) {
      current += char;
      escaping = false;
      continue;
    }

    if (char === "\\") {
      escaping = true;
      continue;
    }

    if (quote) {
      if (char === quote) {
        quote = "";
      } else {
        current += char;
      }
      continue;
    }

    if (char === "\"" || char === "'") {
      quote = char;
      continue;
    }

    if (/\s/.test(char)) {
      if (current) {
        args.push(current);
        current = "";
      }
      continue;
    }

    current += char;
  }

  if (current) {
    args.push(current);
  }

  return args;
}

function parseCliArgs(args) {
  const flags = {};
  const positionals = [];

  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (!arg.startsWith("--")) {
      positionals.push(arg);
      continue;
    }

    const raw = arg.slice(2);
    const [key, inlineValue] = raw.split(/=(.*)/s).filter((part) => part !== undefined);
    const normalizedKey = key.replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
    const next = args[index + 1];
    if (inlineValue != null && inlineValue !== "") {
      flags[normalizedKey] = coerceCliValue(inlineValue);
    } else if (next && !next.startsWith("--")) {
      flags[normalizedKey] = coerceCliValue(next);
      index += 1;
    } else {
      flags[normalizedKey] = true;
    }
  }

  return { flags, positionals };
}

function coerceCliValue(value) {
  if (/^(true|false)$/i.test(value)) {
    return value.toLowerCase() === "true";
  }
  if (/^-?\d+$/.test(value)) {
    return Number(value);
  }
  return value;
}

async function executeCommand(command, tab) {
  if (command.name === "ask") {
    const question = command.arg || command.raw;
    const result = await answerWithAxon(question, tab);
    chatMessages.push({ role: "user", content: question });
    chatMessages.push({ role: "assistant", content: result.answer });
    const copyText = formatChatAnswer(result, question, tab);
    return {
      output: result.answer,
      copyText,
      copiedMessage: "Copied Axon answer to clipboard.",
      doneMessage: "Axon answer ready."
    };
  }

  if (command.name === "scrape") {
    const urls = resolveCommandUrls(command.args, tab);
    const result = await scrapeWithAxon(urls);
    const output = formatAxonScrape(result, tab, urls[0]);
    const words = output.trim().split(/\s+/).filter(Boolean).length;
    return {
      output,
      copyText: output,
      copiedMessage: `Scraped and copied ${words.toLocaleString()} words from Axon.`,
      doneMessage: "Axon scrape ready."
    };
  }

  if (command.name === "crawl") {
    const parsed = parseCliArgs(command.args);
    const urls = resolveCommandUrls(parsed.positionals, tab);
    const pollRun = ++crawlPollRun;
    const crawl = await startCrawlWithAxon(urls, parsed.flags);
    const jobId = crawl.job_id || crawl.jobId || crawl.id;
    setCrawlStatus("Queued", jobId || "", "info");
    if (jobId) {
      pollCrawlStatus(jobId, pollRun);
    }
    return {
      output: [
        "# Crawl",
        "",
        `${badge("info", "queued")} ${urls.length.toLocaleString()} URL${urls.length === 1 ? "" : "s"}`,
        jobId ? `Job: \`${jobId}\`` : badge("warn", "job id unavailable"),
        "",
        ...urls.map((url) => `- ${url}`)
      ].join("\n"),
      doneMessage: jobId ? `Queued crawl ${jobId}.` : "Crawl queued.",
      crawlJobId: jobId || ""
    };
  }

  if (command.name === "extract") {
    const parsed = parseCliArgs(command.args);
    const urls = resolveCommandUrls(parsed.positionals, tab);
    const result = await startExtractWithAxon(urls, parsed.flags);
    return acceptedJobResult("Extract queued", result, "extract");
  }

  if (command.name === "embed") {
    const input = command.arg || tab?.url || "";
    if (!input) {
      throw new Error("embed requires an input path, text, or URL.");
    }
    const result = await startEmbedWithAxon(input);
    return acceptedJobResult("Embed queued", result, "embed");
  }

  if (command.name === "ingest") {
    const result = await startIngestWithAxon(command.args);
    return acceptedJobResult("Ingest queued", result, "ingest");
  }

  if (command.name === "sessions") {
    const result = await startSessionsWithAxon(command.args);
    return acceptedJobResult("Sessions ingest queued", result, "sessions");
  }

  if (command.name === "retrieve") {
    const url = resolveCommandUrl(command.arg, tab);
    const result = await retrieveWithAxon(url);
    const output = formatRetrieve(result, url);
    return {
      output,
      copyText: output,
      sourceUrl: url,
      copiedMessage: "Copied retrieved Axon chunks to clipboard.",
      doneMessage: "Retrieved Axon chunks."
    };
  }

  if (command.name === "query") {
    const query = command.arg || command.raw;
    const result = await queryWithAxon(query);
    const output = formatQuery(result, query);
    return {
      output,
      copyText: output,
      copiedMessage: "Copied Axon query results to clipboard.",
      doneMessage: "Axon query complete."
    };
  }

  if (command.name === "search" || command.name === "research") {
    const parsed = parseCliArgs(command.args);
    const query = parsed.positionals.join(" ").trim();
    if (!query) {
      throw new Error(`${command.name} requires a query.`);
    }
    const result = command.name === "search" ? await searchWithAxon(query, parsed.flags) : await researchWithAxon(query, parsed.flags);
    const output = formatGenericResult(command.name, result);
    return {
      output,
      copyText: output,
      copiedMessage: `Copied Axon ${command.name} result to clipboard.`,
      doneMessage: `Axon ${command.name} complete.`
    };
  }

  if (command.name === "summarize") {
    const urls = resolveCommandUrls(command.args, tab);
    const result = await summarizeWithAxon(urls);
    const output = formatSummary(result, tab, urls[0]);
    return {
      output,
      copyText: output,
      sourceUrl: urls[0],
      copiedMessage: "Copied Axon summary to clipboard.",
      doneMessage: "Axon summary ready."
    };
  }

  if (command.name === "map") {
    const url = resolveCommandUrl(command.arg, tab);
    const result = await mapWithAxon(url);
    const output = formatMap(result, tab, url);
    return {
      output,
      copyText: output,
      sourceUrl: url,
      copiedMessage: `Copied ${result.urls?.length || 0} mapped URLs to clipboard.`,
      doneMessage: "Axon URL map ready."
    };
  }

  if (command.name === "evaluate") {
    const question = command.arg || command.raw;
    const result = await evaluateWithAxon(question);
    const output = formatGenericResult("evaluate", result);
    return {
      output,
      copyText: output,
      copiedMessage: "Copied Axon evaluation to clipboard.",
      doneMessage: "Axon evaluation complete."
    };
  }

  if (command.name === "suggest") {
    const result = await suggestWithAxon(command.arg);
    const output = formatGenericResult("suggest", result);
    return {
      output,
      copyText: output,
      copiedMessage: "Copied Axon suggestions to clipboard.",
      doneMessage: "Axon suggestions ready."
    };
  }

  if (["sources", "domains", "stats", "doctor"].includes(command.name)) {
    const result = await getAxon(`/v1/${command.name}`);
    const output = formatGenericResult(command.name, result);
    return {
      output,
      copyText: output,
      copiedMessage: `Copied Axon ${command.name} output to clipboard.`,
      doneMessage: `Axon ${command.name} complete.`
    };
  }

  if (command.name === "status") {
    if (!command.arg || command.arg === "all" || command.arg === "list") {
      const result = await getAxon("/v1/status");
      const output = formatGenericResult("status", result);
      return {
        output,
        copyText: output,
        copiedMessage: "Copied Axon status to clipboard.",
        doneMessage: "Axon status complete."
      };
    }

    const jobId = command.arg || currentCrawlJobId;
    if (!jobId) {
      return { output: "No crawl job is active.", doneMessage: "No crawl job is active." };
    }

    const result = await getAxon(`/v1/crawl/${encodeURIComponent(jobId)}`);
    const status = crawlStatus(result);
    setCrawlStatus(status.label, jobId, status.tone);
    return {
      output: formatSingleJobStatus(result, jobId),
      copyText: formatSingleJobStatus(result, jobId),
      copiedMessage: "Copied crawl status to clipboard.",
      doneMessage: `Crawl ${jobId}: ${status.label}.`
    };
  }

  if (command.name === "open") {
    const url = normalizeUrl(command.arg);
    const targetTab = tab?.id ? tab : (await activeTab());
    await chrome.tabs.update(targetTab.id, { url });
    return {
      output: ["# Open", "", `${badge("success", "opened")} ${url}`, "", "Auto-scrape will run after navigation if watch is on."].join("\n"),
      doneMessage: `Opened ${url}.`
    };
  }

  if (command.name === "watch") {
    return setWatchFromCommand(command.arg);
  }

  if (command.name === "dedupe") {
    const result = await postAxon("/v1/dedupe", {});
    const output = formatGenericResult("dedupe", result);
    return {
      output,
      copyText: output,
      copiedMessage: "Copied Axon dedupe result to clipboard.",
      doneMessage: "Axon dedupe complete."
    };
  }

  if (command.name === "migrate") {
    const [from, to] = command.args;
    if (!from || !to) {
      throw new Error("migrate requires: migrate <from_collection> <to_collection>");
    }
    const result = await postAxon("/v1/migrate", { from, to });
    const output = formatGenericResult("migrate", result);
    return {
      output,
      copyText: output,
      copiedMessage: "Copied Axon migrate result to clipboard.",
      doneMessage: "Axon migrate complete."
    };
  }

  if (command.name === "auth" || command.name === "config") {
    return describeConfig();
  }

  if (["debug", "screenshot", "setup", "mcp", "serve", "completions"].includes(command.name)) {
    return unsupportedCliCommand(command.name);
  }

  throw new Error(`Unknown command: ${command.name}`);
}

function resolveCommandUrl(arg, tab) {
  const normalized = (arg || "").trim().toLowerCase();
  if (["this", "current", "page", "here"].includes(normalized)) {
    if (tab?.url) {
      return tab.url;
    }
    throw new Error("No current page URL is available.");
  }

  if (arg && /^https?:\/\//i.test(arg)) {
    return arg;
  }
  if (arg) {
    return normalizeUrl(arg);
  }
  if (tab?.url) {
    return tab.url;
  }
  throw new Error("Provide a URL or open an http:// or https:// tab.");
}

function resolveCommandUrls(args, tab) {
  const positionals = Array.isArray(args) ? args : splitArgs(args || "");
  const urlArgs = positionals.filter((arg) => !arg.startsWith("--"));
  if (!urlArgs.length || urlArgs.some((arg) => ["this", "current", "page", "here"].includes(arg.toLowerCase()))) {
    return [resolveCommandUrl("this", tab)];
  }
  return urlArgs.map((arg) => resolveCommandUrl(arg, tab));
}

function normalizeUrl(value) {
  if (!value) {
    throw new Error("URL is required.");
  }
  if (/^https?:\/\//i.test(value)) {
    return value;
  }
  return `https://${value}`;
}

function completeCommand() {
  const value = commandInput.value;
  const leading = value.match(/^\s*/)?.[0] || "";
  const rest = value.slice(leading.length);
  const [partial = "", ...tail] = rest.split(/\s+/);
  const match = commandMatch(COMMAND_ALIASES[partial.toLowerCase()] || partial);

  if (!match) {
    return;
  }

  commandInput.value = `${leading}${match.name}${tail.length ? ` ${tail.join(" ")}` : " "}`;
  updateCommandHint();
}

function updateCommandHint() {
  const value = commandInput.value.trimStart();
  const first = value.split(/\s+/)[0] || "";
  const match = commandMatch(first);
  const mode = match ? `${match.name}: ${match.meta}` : "Chat with Axon";
  commandSendButton.title = mode;
  commandInput.setAttribute("aria-label", mode);
}

function commandMatch(partial) {
  if (!partial) {
    return null;
  }

  return COMMANDS.find((command) => command.name.startsWith(partial.toLowerCase()));
}

function rememberCommand(raw) {
  if (commandHistory[commandHistory.length - 1] !== raw) {
    commandHistory.push(raw);
  }
  commandHistory = commandHistory.slice(-40);
  commandHistoryIndex = commandHistory.length;
  chrome.storage?.local?.set({ [COMMAND_HISTORY_KEY]: commandHistory });
}

function recallCommand(direction) {
  if (!commandHistory.length) {
    return;
  }
  commandHistoryIndex = Math.max(0, Math.min(commandHistory.length, commandHistoryIndex + direction));
  commandInput.value = commandHistory[commandHistoryIndex] || "";
  updateCommandHint();
  setTimeout(() => commandInput.setSelectionRange(commandInput.value.length, commandInput.value.length), 0);
}

async function activeTab() {
  const currentWindowTabs = await chrome.tabs.query({ currentWindow: true });
  const tab =
    currentWindowTabs.find((candidate) => candidate.active && isWebTab(candidate)) ||
    mostRecentWebTab(currentWindowTabs) ||
    mostRecentWebTab(await chrome.tabs.query({}));

  if (!tab?.id || !tab.url) {
    throw new Error("No http:// or https:// tab found.");
  }

  return tab;
}

async function refreshTargetContext() {
  try {
    const tab = await activeTab();
    targetTitleText.textContent = tab.title || "Untitled page";
    targetUrlText.textContent = tab.url;
    targetUrlText.title = tab.url;
    refreshTargetScrapeState(tab.url);
  } catch (error) {
    targetTitleText.textContent = "No page selected";
    targetUrlText.textContent = error instanceof Error ? error.message : String(error);
    targetUrlText.title = "";
    targetStateText.textContent = "No target";
    targetStateText.className = "target-state tone-neutral";
  }
}

async function refreshAutomationStatus() {
  const stored = await chrome.storage.local.get(["autoScrapeEnabled", "lastAutoScrape"]);
  const enabled = stored.autoScrapeEnabled === true;
  watchStatusText.textContent = enabled ? "Watching" : "Paused";
  watchStatusText.className = `header-badge tone-${enabled ? "success" : "warn"}`;
  watchStatusText.title = enabled ? "Auto-scrape is on. Click to pause." : "Auto-scrape is paused. Click to resume.";

  if (!stored.lastAutoScrape) {
    refreshTargetContext();
    return;
  }

  refreshTargetContext();
}

async function refreshTargetScrapeState(url) {
  const stored = await chrome.storage.local.get(["lastAutoScrape"]);
  const scrape = stored.lastAutoScrape;
  if (!scrape || scrape.url !== url) {
    targetStateText.textContent = "Not scraped this session";
    targetStateText.className = "target-state tone-neutral";
    return;
  }

  const stamp = scrape.at ? new Date(scrape.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : "";
  if (scrape.ok) {
    targetStateText.textContent = `Scraped ${stamp}`.trim();
    targetStateText.className = "target-state tone-success";
    return;
  }

  targetStateText.textContent = `Scrape failed ${stamp}`.trim();
  targetStateText.title = scrape.error || "";
  targetStateText.className = "target-state tone-error";
}

async function toggleWatch() {
  const stored = await chrome.storage.local.get(["autoScrapeEnabled"]);
  const enabled = stored.autoScrapeEnabled !== true;
  await chrome.storage.local.set({ autoScrapeEnabled: enabled });
  await refreshAutomationStatus();
  setStatus(enabled ? "Auto-scrape enabled." : "Auto-scrape paused.");
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

async function scrapeWithAxon(urls) {
  return postAxon("/v1/scrape", { urls: Array.isArray(urls) ? urls : [urls] });
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

function formatAxonScrape(result, tab, fallbackUrl) {
  const markdown = result.markdown || result.payload?.markdown || result.output || "";
  const url = result.url || fallbackUrl || tab?.url || "";
  const words = markdown.trim().split(/\s+/).filter(Boolean).length;

  return [
    `# Scrape`,
    "",
    `${badge("success", "ready")} ${words.toLocaleString()} words`,
    `Page: ${tab?.title || result.payload?.title || "Untitled page"}`,
    `URL: ${url}`,
    "",
    markdown || "(Axon returned no markdown.)"
  ].join("\n");
}

function formatSummary(result, tab, fallbackUrl) {
  const summary = result.summary || result.payload?.summary || "";
  const urls = result.urls || result.payload?.urls || [fallbackUrl || tab?.url || ""];
  const contextChars = result.context_chars || result.payload?.context_chars;

  return [
    `# Summary`,
    "",
    `${badge(summary ? "success" : "warn", summary ? "ready" : "empty")}`,
    `Page: ${tab?.title || "Untitled page"}`,
    `URL: ${urls[0] || fallbackUrl || tab?.url || ""}`,
    contextChars ? `Context: ${contextChars.toLocaleString()} chars` : "",
    "",
    summary || "(Axon returned no summary.)"
  ].filter(Boolean).join("\n");
}

function formatMap(result, tab, fallbackUrl) {
  const urls = result.urls || [];
  const total = result.total ?? urls.length;
  const source = result.map_source || "unknown";

  return [
    `# Map`,
    "",
    `${badge(urls.length ? "success" : "warn", `${total.toLocaleString()} discovered`)}`,
    `Page: ${tab?.title || "Untitled page"}`,
    `Start URL: ${result.url || fallbackUrl || tab?.url || ""}`,
    `Source: ${source}`,
    result.warning ? `Warning: ${result.warning}` : "",
    "",
    ...urls
  ].filter(Boolean).join("\n");
}

function formatChatAnswer(result, question, tab) {
  return [
    `# Chat: ${question}`,
    "",
    `${badge("info", "/v1/ask")}`,
    tab?.url ? `Current page: ${tab.url}` : "",
    "",
    result.answer
  ].filter(Boolean).join("\n");
}

function formatRetrieve(result, url) {
  const chunks = result.chunks || result.points || result.payload?.chunks || result.payload?.points || [];
  const lines = [`# Retrieve`, "", `${badge(chunks.length ? "success" : "warn", `${chunks.length} chunks`)}`, `URL: ${url}`, ""];

  if (!chunks.length) {
    lines.push(readableValue(result));
    return lines.join("\n");
  }

  chunks.slice(0, 12).forEach((chunk, index) => {
    const payload = chunk.payload || chunk;
    const text = payload.text || payload.chunk_text || payload.content || payload.markdown || readableValue(payload);
    lines.push(`## Chunk ${index + 1}`, "", `${badge("neutral", payload.url || payload.source_url || "stored context")}`, "", text, "");
  });

  return lines.join("\n").trim();
}

function formatQuery(result, query) {
  const hits = result.results || result.hits || result.payload?.results || result.payload?.hits || [];
  const lines = [`# Query`, "", `Query: ${query}`, `${badge(hits.length ? "success" : "warn", `${hits.length} hits`)}`, ""];

  if (!hits.length) {
    lines.push(readableValue(result));
    return lines.join("\n");
  }

  hits.slice(0, 8).forEach((hit, index) => {
    const payload = hit.payload || hit;
    const url = payload.url || payload.source_url || hit.url || "";
    const score = hit.score ?? hit.similarity ?? payload.score;
    const text = payload.text || payload.chunk_text || payload.content || payload.title || "";
    lines.push(`## Result ${index + 1}`);
    if (score != null) {
      lines.push(`${badge(scoreTone(score), `score ${Number(score).toFixed(3)}`)}`);
    }
    if (url) {
      lines.push(url);
    }
    if (text) {
      lines.push("", truncate(text, 900));
    } else {
      lines.push("", readableValue(hit));
    }
    lines.push("");
  });

  return lines.join("\n").trim();
}

function acceptedJobResult(title, result, kind) {
  const jobId = result.job_id || result.jobId || result.id || "";
  const output = [
    `# ${title.replace(/\s+queued$/i, "")}`,
    "",
    badge("info", "queued"),
    jobId ? `Job: \`${jobId}\`` : badge("warn", "job id unavailable"),
    result.status_url ? `Status: ${result.status_url}` : "",
    result.status ? `State: ${badge(toneForStatus(result.status), result.status)}` : ""
  ].filter(Boolean).join("\n");
  return {
    output,
    copyText: output,
    copiedMessage: `Copied Axon ${kind} job to clipboard.`,
    doneMessage: jobId ? `Queued ${kind} job ${jobId}.` : `Queued ${kind} job.`
  };
}

function formatGenericResult(title, result) {
  const normalized = String(title || "").toLowerCase();
  if (normalized.includes("doctor")) return formatDoctor(result);
  if (normalized.includes("sources")) return formatSources(result);
  if (normalized.includes("domains")) return formatDomains(result);
  if (normalized.includes("stats")) return formatStats(result);
  if (normalized.includes("status")) return formatStatusResult(result);
  if (normalized.includes("search")) return formatSearchResult(result, "Search");
  if (normalized.includes("research")) return formatResearchResult(result);
  if (normalized.includes("evaluate")) return formatEvaluateResult(result);
  if (normalized.includes("suggest")) return formatSuggestResult(result);
  if (normalized.includes("watch")) return formatWatchResult(result);
  if (normalized.includes("dedupe")) return formatDedupeResult(result);
  if (normalized.includes("migrate")) return formatMigrateResult(result);
  return [`# ${capitalize(title)}`, "", readableValue(result)].join("\n");
}

function unsupportedCliCommand(name) {
  const output = [
    `# ${name}`,
    "",
    `\`${name}\` is a local Axon CLI command, but this extension can only call Axon's browser-exposed HTTP API.`,
    "Use it in a shell, or add a server endpoint if we want it available from Chrome."
  ].join("\n");
  return {
    output,
    doneMessage: `${name} is CLI-only from the extension.`
  };
}

function formatJson(value) {
  return JSON.stringify(value, null, 2);
}

function payloadOf(result) {
  return result?.payload && typeof result.payload === "object" ? result.payload : result;
}

function formatDoctor(result) {
  const payload = payloadOf(result) || {};
  const services = payload.services || {};
  const pipelines = payload.pipelines || {};
  const lines = [
    "# Doctor",
    "",
    payload.all_ok === true ? `${badge("success", "all ok")} All systems reachable.` : `${badge("warn", "attention")} Some checks need attention.`,
    payload.observed_at_utc ? `Observed: ${formatTime(payload.observed_at_utc)}` : "",
    statLine("Pending jobs", payload.pending_jobs, payload.pending_jobs ? "warn" : "success")
  ].filter(Boolean);

  if (Object.keys(services).length) {
    lines.push("", "Services");
    Object.entries(services).forEach(([name, service]) => {
      lines.push(`- ${badge(service?.ok ? "success" : "error", service?.ok ? "ok" : "problem")} ${humanizeKey(name)}${service?.detail ? ` - ${service.detail}` : ""}`);
    });
  }

  if (Object.keys(pipelines).length) {
    lines.push("", "Pipelines");
    Object.entries(pipelines).forEach(([name, ok]) => {
      lines.push(`- ${badge(ok ? "success" : "error", ok ? "ready" : "blocked")} ${humanizeKey(name)}`);
    });
  }

  return lines.join("\n");
}

function formatSources(result) {
  const payload = payloadOf(result) || {};
  const sources = payload.sources || payload.urls || result.sources || [];
  const lines = ["# Sources", "", badge(sources.length ? "success" : "warn", `${sources.length.toLocaleString()} indexed`)];
  sources.slice(0, 30).forEach((source) => {
    const url = typeof source === "string" ? source : source.url || source.source_url || source.id || "";
    const count = source.chunk_count ?? source.chunks ?? source.points;
    lines.push(`- ${count != null ? `${badge("neutral", `${count} chunks`)} ` : ""}${url || readableInline(source)}`);
  });
  if (sources.length > 30) lines.push(`- ...and ${(sources.length - 30).toLocaleString()} more`);
  return lines.join("\n");
}

function formatDomains(result) {
  const payload = payloadOf(result) || {};
  const domains = payload.domains || result.domains || [];
  const lines = ["# Domains", "", badge(domains.length ? "success" : "warn", `${domains.length.toLocaleString()} indexed`)];
  domains.slice(0, 30).forEach((domain) => {
    const name = typeof domain === "string" ? domain : domain.domain || domain.host || domain.name || "";
    const chunks = domain.chunk_count ?? domain.chunks;
    const urls = domain.url_count ?? domain.urls;
    lines.push(`- ${name || readableInline(domain)} ${urls != null ? badge("info", `${urls} URLs`) : ""} ${chunks != null ? badge("neutral", `${chunks} chunks`) : ""}`.trim());
  });
  return lines.join("\n");
}

function formatStats(result) {
  const payload = payloadOf(result) || {};
  return ["# Stats", "", readableValue(payload)].join("\n");
}

function formatStatusResult(result) {
  const payload = payloadOf(result) || {};
  const jobs = payload.jobs || result.jobs || [];
  if (Array.isArray(jobs) && jobs.length) {
    const lines = ["# Status", "", badge(jobs.length ? "info" : "neutral", `${jobs.length.toLocaleString()} recent jobs`)];
    jobs.slice(0, 20).forEach((job) => lines.push(`- ${formatJobLine(job, job.kind || "job")}`));
    return lines.join("\n");
  }

  const jobGroups = Object.entries(payload).filter(([key, value]) => key.endsWith("_jobs") && Array.isArray(value));
  if (jobGroups.length) {
    const lines = ["# Status"];
    jobGroups.forEach(([key, group]) => {
      lines.push("", `## ${humanizeKey(key)}`, badge(group.length ? "info" : "neutral", `${group.length.toLocaleString()} jobs`));
      group.slice(0, 12).forEach((job) => lines.push(`- ${formatJobLine(job, key.replace(/^local_/, "").replace(/_jobs$/, ""))}`));
      if (group.length > 12) lines.push(`- ...and ${(group.length - 12).toLocaleString()} more`);
    });
    return lines.join("\n");
  }

  return ["# Status", "", readableValue(payload)].join("\n");
}

function formatSearchResult(result, title) {
  const payload = payloadOf(result) || {};
  const results = payload.results || result.results || [];
  const jobs = payload.crawl_jobs || result.crawl_jobs || [];
  const lines = [`# ${title}`, "", `${badge(results.length ? "success" : "warn", `${results.length.toLocaleString()} results`)}${jobs.length ? ` ${badge("info", `${jobs.length} crawls queued`)}` : ""}`];
  results.slice(0, 10).forEach((item, index) => {
    lines.push("", `${index + 1}. ${item.title || item.url || item.href || "Result"}`);
    if (item.url || item.href) lines.push(`   ${item.url || item.href}`);
    if (item.content || item.snippet || item.text) lines.push(`   ${truncate(item.content || item.snippet || item.text, 260)}`);
  });
  return lines.join("\n");
}

function formatResearchResult(result) {
  const payload = payloadOf(result) || {};
  const answer = payload.answer || payload.summary || payload.synthesis;
  return answer ? ["# Research", "", answer].join("\n") : formatSearchResult(result, "Research");
}

function formatEvaluateResult(result) {
  const payload = payloadOf(result) || {};
  const lines = ["# Evaluate", ""];
  if (payload.verdict) lines.push(`Verdict: ${badge(toneForVerdict(payload.verdict), payload.verdict)}`);
  ["accuracy", "relevance", "completeness", "specificity"].forEach((key) => {
    if (payload[key] != null) lines.push(`${humanizeKey(key)}: ${badge(scoreTone(payload[key]), payload[key])}`);
  });
  if (payload.explanation || payload.reasoning) lines.push("", payload.explanation || payload.reasoning);
  return lines.length > 2 ? lines.join("\n") : ["# Evaluate", "", readableValue(payload)].join("\n");
}

function formatSuggestResult(result) {
  const payload = payloadOf(result) || {};
  const suggestions = payload.suggestions || payload.urls || result.suggestions || [];
  const lines = ["# Suggest", "", badge(suggestions.length ? "success" : "warn", `${suggestions.length.toLocaleString()} suggestions`)];
  suggestions.slice(0, 20).forEach((suggestion) => {
    const url = typeof suggestion === "string" ? suggestion : suggestion.url || suggestion.href || "";
    const reason = suggestion.reason || suggestion.title || "";
    lines.push(`- ${url || readableInline(suggestion)}${reason ? ` - ${reason}` : ""}`);
  });
  return lines.join("\n");
}

function formatWatchResult(result) {
  const payload = payloadOf(result) || {};
  const watches = payload.watches || result.watches || [];
  if (!Array.isArray(watches) || !watches.length) return ["# Watch", "", readableValue(payload)].join("\n");
  const lines = ["# Watch", "", badge(watches.length ? "info" : "neutral", `${watches.length.toLocaleString()} watches`)];
  watches.slice(0, 20).forEach((watch) => {
    lines.push(`- ${badge(watch.enabled === false ? "warn" : "success", watch.enabled === false ? "paused" : "enabled")} ${watch.name || watch.id} every ${watch.every_seconds || "?"}s`);
  });
  return lines.join("\n");
}

function formatDedupeResult(result) {
  const payload = payloadOf(result) || {};
  return [
    "# Dedupe",
    "",
    statLine("Deleted", payload.deleted ?? payload.points_deleted, "warn"),
    statLine("Scanned", payload.scanned ?? payload.points_scanned, "info"),
    statLine("Groups", payload.groups ?? payload.duplicate_groups, "neutral")
  ].filter(Boolean).join("\n") || ["# Dedupe", "", readableValue(payload)].join("\n");
}

function formatMigrateResult(result) {
  const payload = payloadOf(result) || {};
  return [
    "# Migrate",
    "",
    payload.from && payload.to ? `${payload.from} -> ${payload.to}` : "",
    statLine("Points migrated", payload.points_migrated, "success"),
    statLine("Pages processed", payload.pages_processed, "info")
  ].filter(Boolean).join("\n") || ["# Migrate", "", readableValue(payload)].join("\n");
}

function jobSummary(result) {
  const job = result.job || payloadOf(result)?.job || payloadOf(result);
  if (!job || typeof job !== "object") return "";
  return [
    job.id ? `Job: ${job.id}` : "",
    job.url ? `URL: ${job.url}` : "",
    job.error_text ? `Error: ${job.error_text}` : "",
    job.updated_at ? `Updated: ${formatTime(job.updated_at)}` : ""
  ].filter(Boolean).join("\n");
}

function formatJobLine(job, kind) {
  const id = job.id ? String(job.id).slice(0, 8) : "";
  const status = job.status || job.state || (job.error_text ? "failed" : job.finished_at ? "completed" : job.started_at ? "running" : "pending");
  const result = job.result_json || {};
  const pages = result.pages_crawled ?? result.ok_pages ?? result.md_created ?? result.total_pages;
  const target = job.url || job.target || job.input_text || "";
  const parts = [
    badge(toneForStatus(status), status),
    kind,
    id ? `${id}` : "",
    pages != null ? `(${Number(pages).toLocaleString()} pages)` : "",
    target ? `- ${target}` : "",
    job.error_text ? `- ${job.error_text}` : ""
  ];
  return parts.filter(Boolean).join(" ");
}

function formatSingleJobStatus(result, jobId) {
  const job = result.job || payloadOf(result)?.job || payloadOf(result) || {};
  const status = job.status || job.state || (job.error_text ? "failed" : job.finished_at ? "completed" : job.started_at ? "running" : "pending");
  return [
    "# Job status",
    "",
    `${badge(toneForStatus(status), status)} ${job.kind || "crawl"} \`${job.id || jobId}\``,
    job.url ? `URL: ${job.url}` : "",
    job.created_at ? `Created: ${formatTime(job.created_at)}` : "",
    job.started_at ? `Started: ${formatTime(job.started_at)}` : "",
    job.finished_at ? `Finished: ${formatTime(job.finished_at)}` : "",
    job.error_text ? `${badge("error", "error")} ${job.error_text}` : "",
    job.result_json ? ["", "Result", readableValue(job.result_json)].join("\n") : ""
  ].filter(Boolean).join("\n");
}

function readableValue(value, depth = 0) {
  const indent = "  ".repeat(depth);
  if (value == null || typeof value !== "object") return formatPrimitive(value);
  if (Array.isArray(value)) {
    if (!value.length) return "None.";
    return value.slice(0, 12).map((item) => `${indent}- ${readableInline(item)}`).join("\n");
  }
  const entries = Object.entries(value).filter(([, val]) => val !== undefined && val !== null && val !== "");
  if (!entries.length) return "No details.";
  return entries.slice(0, 24).map(([key, val]) => {
    if (val && typeof val === "object") return `${indent}${humanizeKey(key)}:\n${readableValue(val, depth + 1)}`;
    return `${indent}${humanizeKey(key)}: ${formatPrimitive(val)}`;
  }).join("\n");
}

function readableInline(value) {
  if (value == null || typeof value !== "object") return formatPrimitive(value);
  if (Array.isArray(value)) return `${value.length} item${value.length === 1 ? "" : "s"}`;
  const preferred = [value.title, value.name, value.url, value.source_url, value.id].filter(Boolean).join(" ");
  if (preferred) return preferred;
  return Object.entries(value)
    .filter(([, val]) => val == null || typeof val !== "object")
    .slice(0, 4)
    .map(([key, val]) => `${humanizeKey(key)} ${formatPrimitive(val)}`)
    .join(", ") || truncate(formatJson(value), 180);
}

function badge(tone, label) {
  return `[[${tone || "neutral"}:${String(label || "").replace(/\]/g, "")}]]`;
}

function statLine(label, value, tone = "neutral") {
  return value == null ? "" : `${label}: ${badge(tone, Number(value).toLocaleString())}`;
}

function numberLine(label, value) {
  return value == null ? "" : `${label}: ${Number(value).toLocaleString()}`;
}

function toneForStatus(status) {
  const normalized = String(status || "").toLowerCase();
  if (["completed", "complete", "ok", "ready", "success", "succeeded"].includes(normalized)) return "success";
  if (["failed", "error", "blocked"].includes(normalized)) return "error";
  if (["canceled", "cancelled", "paused", "warn", "warning"].includes(normalized)) return "warn";
  if (["running", "pending", "queued", "active"].includes(normalized)) return "info";
  return "neutral";
}

function scoreTone(score) {
  const value = Number(score);
  if (!Number.isFinite(value)) return "neutral";
  if (value >= 0.75) return "success";
  if (value >= 0.45) return "info";
  if (value > 0) return "warn";
  return "neutral";
}

function toneForVerdict(verdict) {
  const normalized = String(verdict || "").toLowerCase();
  if (normalized.includes("pass") || normalized.includes("good") || normalized.includes("correct")) return "success";
  if (normalized.includes("fail") || normalized.includes("incorrect")) return "error";
  if (normalized.includes("partial") || normalized.includes("mixed")) return "warn";
  return "info";
}

function formatPrimitive(value) {
  if (value == null) return "none";
  if (typeof value === "boolean") return value ? "yes" : "no";
  if (typeof value === "number") return Number.isFinite(value) ? value.toLocaleString() : String(value);
  if (typeof value === "object") return truncate(formatJson(value), 180);
  return String(value);
}

function humanizeKey(key) {
  return String(key).replace(/_/g, " ").replace(/([a-z])([A-Z])/g, "$1 $2").replace(/^./, (letter) => letter.toUpperCase());
}

function formatTime(value) {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? String(value) : date.toLocaleString();
}

function appendChatMessage(role, content) {
  const message = document.createElement("div");
  message.className = `chat-message chat-${role}`;
  message.dataset.role = role;
  message.dataset.content = content;
  const label = document.createElement("span");
  label.className = "chat-role";
  label.textContent = role === "user" ? "You" : "Axon";
  const body = document.createElement("div");
  body.className = "chat-content";
  renderMessageContent(body, content, role);
  message.append(label, body);
  chatLog.append(message);
  chatLog.scrollTop = chatLog.scrollHeight;
  scheduleOutputPersist();
  return message;
}

function updateChatMessage(message, content) {
  const body = message.querySelector(".chat-content") || message.querySelector("p");
  body.className = "chat-content";
  renderMessageContent(body, content, message.dataset.role || "assistant");
  message.dataset.content = content;
  chatLog.scrollTop = chatLog.scrollHeight;
  scheduleOutputPersist();
}

function renderMessageContent(element, content, role) {
  element.textContent = "";
  if (role === "user") {
    element.textContent = content;
    return;
  }
  element.innerHTML = markdownToHtml(content);
}

function markdownToHtml(markdown) {
  const lines = String(markdown || "").split(/\r?\n/);
  const html = [];
  let paragraph = [];
  let list = null;
  let inCode = false;
  let codeLines = [];
  let codeLang = "";

  const flushParagraph = () => {
    if (!paragraph.length) return;
    html.push(`<p>${renderInline(paragraph.join(" "))}</p>`);
    paragraph = [];
  };
  const flushList = () => {
    if (!list) return;
    html.push(`<${list.type}>${list.items.map((item) => `<li>${renderInline(item)}</li>`).join("")}</${list.type}>`);
    list = null;
  };

  for (const line of lines) {
    const fence = line.match(/^```(\S*)\s*$/);
    if (fence) {
      if (inCode) {
        html.push(`<pre><code${codeLang ? ` class="language-${escapeHtml(codeLang)}"` : ""}>${escapeHtml(codeLines.join("\n"))}</code></pre>`);
        inCode = false;
        codeLines = [];
        codeLang = "";
      } else {
        flushParagraph();
        flushList();
        inCode = true;
        codeLang = fence[1] || "";
      }
      continue;
    }

    if (inCode) {
      codeLines.push(line);
      continue;
    }

    if (!line.trim()) {
      flushParagraph();
      flushList();
      continue;
    }

    const heading = line.match(/^(#{1,4})\s+(.+)$/);
    if (heading) {
      flushParagraph();
      flushList();
      const level = Math.min(heading[1].length + 2, 6);
      html.push(`<h${level}>${renderInline(heading[2])}</h${level}>`);
      continue;
    }

    const bullet = line.match(/^\s*[-*]\s+(.+)$/);
    if (bullet) {
      flushParagraph();
      if (!list || list.type !== "ul") list = { type: "ul", items: [] };
      list.items.push(bullet[1]);
      continue;
    }

    const numbered = line.match(/^\s*\d+\.\s+(.+)$/);
    if (numbered) {
      flushParagraph();
      if (!list || list.type !== "ol") list = { type: "ol", items: [] };
      list.items.push(numbered[1]);
      continue;
    }

    flushList();
    paragraph.push(line.trim());
  }

  if (inCode) {
    html.push(`<pre><code${codeLang ? ` class="language-${escapeHtml(codeLang)}"` : ""}>${escapeHtml(codeLines.join("\n"))}</code></pre>`);
  }
  flushParagraph();
  flushList();
  return html.join("");
}

function renderInline(value) {
  let html = escapeHtml(value);
  const codeSpans = [];
  html = html.replace(/`([^`]+)`/g, (_, code) => {
    const index = codeSpans.push(code) - 1;
    return `@@AXON_CODE_${index}@@`;
  });
  html = html.replace(/\[\[(success|info|warn|error|neutral):([^\]]+)\]\]/g, '<span class="md-badge md-badge-$1">$2</span>');
  html = html.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  html = html.replace(/\*([^*\n]+)\*/g, "<em>$1</em>");
  html = html.replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noreferrer">$1</a>');
  html = html.replace(/(^|\\s)(https?:\/\/[^\\s<]+)/g, '$1<a href="$2" target="_blank" rel="noreferrer">$2</a>');
  html = html.replace(/@@AXON_CODE_(\d+)@@/g, (_, index) => `<code>${codeSpans[Number(index)] || ""}</code>`);
  return html;
}

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function attachResultActions(message, result) {
  const sourceUrl = result.sourceUrl || extractFirstUrl(result.output);
  if (!result.copyText && !sourceUrl) {
    return;
  }

  const actions = document.createElement("div");
  actions.className = "result-actions";

  if (result.copyText) {
    actions.append(resultActionButton("Copy", () => {
      tryWriteClipboard(result.copyText).then((copied) => setStatus(copied ? "Copied output." : "Copy failed. Focus the panel and retry."));
    }));
  }

  if (sourceUrl) {
    actions.append(resultActionButton("Retrieve", () => {
      commandInput.value = `retrieve ${sourceUrl}`;
      commandInput.focus();
      updateCommandHint();
    }));
    actions.append(resultActionButton("Crawl", () => {
      commandInput.value = `crawl ${sourceUrl}`;
      commandInput.focus();
      updateCommandHint();
    }));
    actions.append(resultActionButton("Ask", () => {
      commandInput.value = `ask about ${sourceUrl} `;
      commandInput.focus();
      updateCommandHint();
    }));
  }

  message.append(actions);
}

async function restoreCommandHistory() {
  const stored = await chrome.storage.local.get([COMMAND_HISTORY_KEY]);
  commandHistory = Array.isArray(stored[COMMAND_HISTORY_KEY]) ? stored[COMMAND_HISTORY_KEY].slice(-40) : [];
  commandHistoryIndex = commandHistory.length;
}

async function restoreOutputLog() {
  const stored = await chrome.storage.local.get([OUTPUT_LOG_KEY]);
  const entries = Array.isArray(stored[OUTPUT_LOG_KEY]) ? stored[OUTPUT_LOG_KEY].slice(-OUTPUT_LIMIT) : [];
  if (!entries.length) {
    return false;
  }

  chatLog.textContent = "";
  entries.forEach((entry) => {
    const content = String(entry.content || "").replace(/^(Asking Axon|Running [a-z]+)\.\.\.$/i, "$&\n\nInterrupted by reload.");
    appendChatMessage(entry.role === "user" ? "user" : "assistant", content);
  });
  return true;
}

function scheduleOutputPersist() {
  if (!chrome.storage?.local) {
    return;
  }

  clearTimeout(persistOutputTimer);
  persistOutputTimer = setTimeout(persistOutputLog, 120);
}

function persistOutputLog() {
  const entries = Array.from(chatLog.querySelectorAll(".chat-message")).slice(-OUTPUT_LIMIT).map((message) => ({
    role: message.dataset.role === "user" ? "user" : "assistant",
    content: message.dataset.content || message.querySelector(".chat-content")?.textContent || message.querySelector("p")?.textContent || ""
  }));
  chrome.storage.local.set({ [OUTPUT_LOG_KEY]: entries });
}

function resultActionButton(label, onClick) {
  const button = document.createElement("button");
  button.className = "result-action";
  button.type = "button";
  button.textContent = label;
  button.addEventListener("click", onClick);
  return button;
}

function extractFirstUrl(text) {
  return String(text || "").match(/https?:\/\/[^\s)]+/i)?.[0] || "";
}

async function writeClipboard(text) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }

  const scratch = document.createElement("textarea");
  scratch.value = text;
  scratch.setAttribute("readonly", "");
  scratch.className = "clipboard-scratch";
  document.body.append(scratch);
  scratch.select();

  if (!document.execCommand("copy")) {
    scratch.remove();
    throw new Error("Clipboard write failed.");
  }

  scratch.remove();
}

async function tryWriteClipboard(text) {
  try {
    await writeClipboard(text);
    return true;
  } catch {
    return false;
  }
}

function truncate(value, maxLength) {
  if (!value || value.length <= maxLength) {
    return value || "";
  }
  return `${value.slice(0, maxLength)}\n...[truncated ${value.length - maxLength} chars]`;
}

function setBusy(isBusy, message) {
  apiStatusText.disabled = isBusy;
  watchStatusText.disabled = isBusy;
  if (openSidebarButton) {
    openSidebarButton.disabled = isBusy;
  }
  openOptionsButton.disabled = isBusy;
  commandInput.disabled = isBusy;
  commandSendButton.disabled = isBusy;
  chatResetButton.disabled = isBusy;
  document.querySelectorAll(".command-chip").forEach((button) => {
    button.disabled = isBusy;
  });
  if (message) {
    setStatus(message);
  }
}

function setCommandStatus(message, tone = "neutral") {
  if (!commandStatusText) {
    return;
  }
  commandStatusText.textContent = message;
  commandStatusText.className = `status-badge tone-${tone}`;
}

function setStatus(message) {
  statusText.textContent = message;
  statusText.classList.add("is-visible");
  clearTimeout(setStatus.hideTimer);
  setStatus.hideTimer = setTimeout(() => {
    statusText.classList.remove("is-visible");
  }, 2600);
}

function setApiStatus(message, tone = "neutral") {
  apiStatusText.textContent = message;
  apiStatusText.className = `header-badge tone-${tone}`;
}

function setChatStatus(message, tone = "neutral") {
  chatStatusText.textContent = message;
  chatStatusText.className = `status-badge tone-${tone}`;
}

function setCrawlStatus(message, jobId, tone = "neutral") {
  const normalized = message.toLowerCase();
  currentCrawlJobId = jobId || "";
  crawlPanel.classList.toggle("is-idle", !jobId && normalized === "idle");
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
    return { axonUrl: DEFAULT_AXON_URL, axonToken: "" };
  }

  const stored = await chrome.storage.local.get(["axonUrl", "axonToken"]);
  return {
    axonUrl: stored.axonUrl || DEFAULT_AXON_URL,
    axonToken: stored.axonToken || ""
  };
}
