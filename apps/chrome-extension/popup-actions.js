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
    // `session:<provider>:<path>` selectors require a local path on the Axon
    // host — the browser has no way to supply one, so this can't be migrated
    // to a `/v1/sources` request the way scrape/crawl/embed/ingest were.
    return unsupportedCliCommand(command.name);
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

    const result = await getAxon(`/v1/jobs/${encodeURIComponent(jobId)}`);
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
