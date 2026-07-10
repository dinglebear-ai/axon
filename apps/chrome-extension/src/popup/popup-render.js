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

init();

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
