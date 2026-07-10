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
  { name: "remember", meta: "Save selection/page to Axon memory", kind: "write" },
  { name: "scrape", meta: "Scrape URL(s)", kind: "write" },
  { name: "crawl", meta: "Start a crawl job", kind: "job" },
  { name: "search", meta: "Web search and auto-crawl", kind: "write" },
  { name: "research", meta: "Research synthesis", kind: "write" },
  { name: "embed", meta: "Start embed job", kind: "job" },
  { name: "query", meta: "Search indexed sources", kind: "read" },
  { name: "retrieve", meta: "Retrieve indexed chunks", kind: "read" },
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
  rem: "remember",
  s: "scrape",
  c: "crawl",
  r: "retrieve",
  q: "query",
  d: "doctor",
  o: "open",
  w: "watch"
};

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
    // Bound to the Send button click / Enter keydown, so this may prompt.
    const config = await loadConfig();
    await ensureAxonServerPermissionForGesture(config.axonUrl.trim().replace(/\/+$/, ""));
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
