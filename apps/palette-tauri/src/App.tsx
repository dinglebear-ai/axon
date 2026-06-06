import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Activity,
  ArrowLeft,
  BarChart3,
  BookOpen,
  Bot,
  Boxes,
  Braces,
  Camera,
  Database,
  FileDown,
  Globe,
  GitCompare,
  HelpCircle,
  Layers,
  Map as MapIcon,
  PackageOpen,
  SearchCheck,
  Search,
  Send,
  Settings,
  Sparkles,
  Stethoscope,
  Workflow,
  X,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { ChevronRight } from "lucide-react";

import { CrawlJobView } from "@/components/palette/CrawlJobView";
import { OutputPanel } from "@/components/palette/OutputPanel";
import { SettingsPanel } from "@/components/palette/SettingsPanel";
import { ScrollArea } from "@/components/ui/aurora/scroll-area";
import { StatusIndicator } from "@/components/ui/aurora/status-indicator";
import { hostFromUrl, summarizeCrawl } from "@/lib/crawlJob";
import {
  type PaletteConfig,
  buildActionRequest,
  createAxonClient,
  executeAction,
} from "@/lib/axonClient";
import {
  ACTIONS,
  type PaletteAction,
  acceptsDirectUrl,
  actionMatches,
} from "@/lib/actions";
import { formatPayload, outputKindFor } from "@/lib/format";
import {
  actionDisplayMeta,
  argumentFor,
  argumentPlaceholder,
  focusInput,
  hostLabel,
  looksLikeUrl,
  parseCommand,
  sortActionsForDisplay,
  validationMessage,
} from "@/lib/paletteView";
import { invoke, isTauriRuntime } from "@/lib/invoke";
import type { RunState } from "@/lib/runState";
import { newRequestId, usePaletteStream } from "@/lib/paletteStream";

interface HistoryItem {
  action: PaletteAction;
  target: string;
  status: number;
  when: string;
  pinned?: boolean;
  running?: boolean;
  duration?: string;
  text?: string;
  outputKind?: "markdown" | "code";
}

const shortcutOptions = ["Ctrl+Shift+Space", "Alt+Space", "Ctrl+Space", "Cmd+Shift+Space"] as const;
document.documentElement.classList.toggle("tauri-runtime", isTauriRuntime);
const appWindow = isTauriRuntime
  ? getCurrentWindow()
  : {
      listen: async () => () => undefined,
    };

export default function App() {
  const [query, setQuery] = useState("");
  const [modeAction, setModeAction] = useState<PaletteAction | null>(null);
  const [selected, setSelected] = useState(0);
  const [config, setConfig] = useState<PaletteConfig | null>(null);
  const [draftConfig, setDraftConfig] = useState<PaletteConfig | null>(null);
  const [configError, setConfigError] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [browseOpen, setBrowseOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [pinnedTargets, setPinnedTargets] = useState<Set<string>>(() => new Set());
  const [run, setRun] = useState<RunState>({ kind: "idle" });
  const [copied, setCopied] = useState(false);
  const [shownTick, setShownTick] = useState(0);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [canceling, setCanceling] = useState(false);

  usePaletteStream(setRun);

  useEffect(() => {
    invoke<PaletteConfig>("load_palette_config")
      .then((nextConfig) => {
        setConfig(nextConfig);
        setDraftConfig(nextConfig);
      })
      .catch((err) => {
        setConfigError(String(err));
        void invoke<PaletteConfig>("load_palette_default_config")
          .then((fallbackConfig) => {
            setConfig(fallbackConfig);
            setDraftConfig(fallbackConfig);
          })
          .catch(() => {
            setConfig(null);
            setDraftConfig(null);
          });
      });
  }, []);

  useEffect(() => {
    const unlisteners = [
      appWindow.listen("palette://shown", () => {
        setShownTick((tick) => tick + 1);
        focusInput(true);
      }),
      appWindow.listen("palette://open-settings", () => setSettingsOpen(true)),
    ];
    return () => {
      void Promise.all(unlisteners).then((items) => items.forEach((unlisten) => unlisten()));
    };
  }, []);

  useEffect(() => {
    const onBlur = () => void invoke("hide_palette");
    window.addEventListener("blur", onBlur);
    return () => window.removeEventListener("blur", onBlur);
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = event.metaKey || event.ctrlKey;
      if (event.key === "Escape") {
        event.preventDefault();
        if (settingsOpen) {
          setSettingsOpen(false);
        } else if (historyOpen) {
          setHistoryOpen(false);
          setBrowseOpen(true);
        } else if (browseOpen && !query && !modeAction && run.kind === "idle") {
          setBrowseOpen(false);
        } else if (modeAction && !query) {
          setModeAction(null);
        } else if (query) {
          setQuery("");
          setModeAction(null);
        } else {
          void invoke("hide_palette");
        }
      } else if (modifier && event.key.toLowerCase() === "l") {
        event.preventDefault();
        focusInput(true);
      } else if (modifier && event.key.toLowerCase() === "k") {
        event.preventDefault();
        void invoke("show_palette").then(() => focusInput(true));
      } else if (modifier && event.key.toLowerCase() === "c" && "text" in run) {
        const target = event.target as HTMLElement | null;
        if (target?.tagName !== "INPUT" && target?.tagName !== "TEXTAREA") {
          event.preventDefault();
          void copyOutput(run.text);
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [browseOpen, historyOpen, modeAction, query, run, settingsOpen]);

  useEffect(() => {
    if (!config) return;
    const root = document.documentElement;
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const applyTheme = () => {
      const useLight = config.theme === "light" || (config.theme === "system" && media.matches);
      root.classList.toggle("light", useLight);
      root.classList.toggle("dark", !useLight);
    };
    applyTheme();
    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [config]);

  // Live job polling: while a crawl job is non-terminal, poll its real status
  // (and the handed-off embed job) ~once/sec and refresh the snapshot. Keyed on
  // jobId + terminal so it stops cleanly when the job (incl. embed phase) ends.
  const jobId = run.kind === "job" ? run.jobId : "";
  const jobPhase = run.kind === "job" ? run.snapshot.phase : "idle";
  const jobTerminal = jobPhase === "done" || jobPhase === "failed" || jobPhase === "canceled";
  useEffect(() => {
    if (run.kind !== "job" || !jobId || jobTerminal) return;
    let active = true;
    const getJson = (path: string) =>
      invoke<{ ok: boolean; status: number; payload: unknown }>("axon_http_request", {
        request: { method: "GET", path, body: null },
      });
    const tick = async () => {
      try {
        const crawlRes = await getJson(`/v1/crawl/${jobId}`);
        if (!active) return;
        const crawlPayload = crawlRes.payload;
        const embedId = extractEmbedJobId(crawlPayload);
        let embedPayload: unknown;
        if (embedId) {
          try {
            embedPayload = (await getJson(`/v1/embed/${embedId}`)).payload;
          } catch {
            /* embed row not visible yet — keep crawl in the embedding phase */
          }
        }
        if (!active) return;
        setNowMs(Date.now());
        setRun((current) => {
          if (current.kind !== "job" || current.jobId !== jobId) return current;
          const elapsedSec = Math.max(0, (Date.now() - current.startedAtMs) / 1000);
          const snapshot = summarizeCrawl(
            crawlPayload,
            { jobId, url: current.url, elapsedSec, maxPages: current.maxPages, maxDepth: current.maxDepth },
            embedPayload,
          );
          return { ...current, snapshot, subtitle: `job ${jobId}` };
        });
      } catch {
        /* transient poll failure — keep trying on the next tick */
      }
    };
    void tick();
    const id = window.setInterval(() => void tick(), 1000);
    return () => {
      active = false;
      window.clearInterval(id);
    };
  }, [run.kind, jobId, jobTerminal]);

  const parsed = useMemo(() => parseCommand(query), [query]);
  const hasQuery = query.trim().length > 0;
  const filtered = useMemo(() => {
    if (parsed.invoked) return [parsed.invoked];
    if (looksLikeUrl(parsed.search)) {
      return sortActionsForDisplay(ACTIONS).slice(0, 12);
    }
    return sortActionsForDisplay(ACTIONS.filter((action) => actionMatches(action, parsed.search))).slice(0, 12);
  }, [parsed.invoked, parsed.search]);
  const suggestedAction = filtered[Math.min(selected, Math.max(filtered.length - 1, 0))];
  const active = modeAction ?? suggestedAction;
  const activeArgument = active ? argumentFor(active, modeAction, parsed, query) : "";
  const validation = active ? validationMessage(active, activeArgument) : "No matching action";
  const jobMinimized = run.kind === "job" && run.minimized;
  const showOutput = run.kind !== "idle" && !jobMinimized;
  const showContent = settingsOpen || historyOpen || showOutput || hasQuery || browseOpen;
  const compact = !showContent;
  const showResultsLayout = showOutput || settingsOpen || historyOpen;
  const showActionPanel = !showResultsLayout && !settingsOpen && !historyOpen;

  useEffect(() => {
    setSelected(0);
  }, [parsed.search, modeAction]);

  useEffect(() => {
    const size = jobMinimized
      ? { width: 680, height: 96 }
      : settingsOpen
      ? { width: 800, height: 560 }
      : historyOpen
      ? { width: 760, height: 520 }
      : showResultsLayout
      ? { width: 900, height: 568 }
      : showContent
        ? { width: 760, height: Math.min(142 + filtered.length * 48, window.screen.availHeight - 80) }
        : { width: 680, height: 56 };
    void invoke("resize_palette", size);
  }, [jobMinimized, settingsOpen, historyOpen, showResultsLayout, showContent, filtered.length, shownTick]);

  const client = useMemo(() => (config ? createAxonClient(config) : null), [config]);

  async function submit(action: PaletteAction = active, argumentOverride?: string) {
    if (!client || !config || !action || run.kind === "running" || run.kind === "streaming") return;
    const argument = normalizeSubmitArgument(
      action,
      argumentOverride ?? argumentFor(action, modeAction, parsed, query),
    );
    const validation = validationMessage(action, argument);
    if (validation) return;
    setModeAction(action);
    setQuery(argument);
    setBrowseOpen(false);
    const commandLine = `${action.subcommand}${argument ? ` ${argument}` : ""}`;
    if (action.subcommand === "crawl") {
      const seedUrl = crawlSeedUrl(argument);
      const startedAtMs = Date.now();
      const pendingSnapshot = summarizeCrawl({ job: { status: "pending" } }, { jobId: "", url: seedUrl });
      setRun({
        kind: "job",
        family: "crawl",
        title: `Crawling ${hostFromUrl(seedUrl)}`,
        subtitle: "submitting…",
        jobId: "",
        statusUrl: "",
        url: seedUrl,
        startedAtMs,
        maxPages: 0,
        maxDepth: 0,
        snapshot: pendingSnapshot,
        minimized: false,
      });
      try {
        const result = await executeAction(client, action, argument, config);
        const payload = (result.payload ?? {}) as Record<string, unknown>;
        const jobId =
          typeof payload.job_id === "string"
            ? payload.job_id
            : typeof payload.id === "string"
              ? payload.id
              : null;
        if (!result.ok || !jobId) {
          const text = formatPayload(action.subcommand, result.payload);
          pushHistory(action, seedUrl, result.status, text, "code");
          setRun({
            kind: "error",
            title: "Crawl failed",
            subtitle: `${result.method} ${result.path} | HTTP ${result.status}`,
            text,
            outputKind: "code",
            result,
          });
          return;
        }
        pushHistory(action, seedUrl, result.status, undefined, "code");
        setRun((current) =>
          current.kind === "job" && current.url === seedUrl
            ? {
                ...current,
                jobId,
                statusUrl: `/v1/crawl/${jobId}`,
                subtitle: `job ${jobId}`,
                snapshot: { ...current.snapshot, jobId },
              }
            : current,
        );
      } catch (err) {
        const text = err instanceof Error ? err.message : String(err);
        setRun({
          kind: "error",
          title: "Crawl failed",
          subtitle: commandLine,
          text,
          outputKind: "code",
          result: { ok: false, status: 0, path: "/v1/crawl", method: "POST", payload: null },
        });
      }
      return;
    }
    if (action.subcommand === "ask" || action.subcommand === "chat") {
      const requestId = newRequestId();
      const request = buildActionRequest(client, action, argument, config);
      const streamPath = action.subcommand === "chat" ? "/v1/chat/stream" : "/v1/ask/stream";
      if (isTauriRuntime) {
        setRun({
          kind: "streaming",
          title: `Streaming ${action.label}`,
          subtitle: `${request.method} ${streamPath}`,
          text: "",
          outputKind: outputKindFor(action.subcommand),
          requestId,
          path: streamPath,
          actionLabel: action.label,
          prompt: argument,
        });
        try {
          await invoke("axon_http_stream_request", {
            request: {
              ...request,
              requestId,
              path: streamPath,
              body: request.body,
            },
          });
          return;
        } catch (err) {
          const message = err instanceof Error ? err.message : String(err);
          setRun((current) =>
            current.kind === "streaming" && current.requestId === requestId
              ? {
                  kind: "error",
                  title: `${action.label} stream failed`,
                  subtitle: `${request.method} ${streamPath}`,
                  text: message,
                  outputKind: outputKindFor(action.subcommand),
                  prompt: current.prompt,
                  result: {
                    ok: false,
                    status: 0,
                    path: streamPath,
                    method: "POST",
                    payload: { error: message },
                  },
                }
              : current,
          );
          return;
        }
      } else {
        setRun({
          kind: "running",
          title: action.label,
          subtitle: `${request.method} ${request.path}`,
          prompt: argument,
        });
      }
    } else {
      setRun({
        kind: "running",
        title: `Running ${action.label}`,
        subtitle: commandLine,
      });
    }
    try {
      const result = await executeAction(client, action, argument, config);
      const text = formatPayload(action.subcommand, result.payload);
      pushHistory(action, argument || action.subcommand, result.status, text, outputKindFor(action.subcommand));
      const isConversation = action.subcommand === "ask" || action.subcommand === "chat";
      setRun({
        kind: result.ok ? "success" : "error",
        title: isConversation ? action.label : `${action.label} ${result.ok ? "completed" : "failed"}`,
        subtitle: action.subcommand === "ask"
          ? `RAG over ${config.collection || "axon"} | ${result.path}`
          : action.subcommand === "chat"
          ? result.path
          : `${result.method} ${result.path} | HTTP ${result.status}`,
        text,
        outputKind: outputKindFor(action.subcommand),
        prompt: isConversation ? argument : undefined,
        result,
      });
    } catch (err) {
      const text = err instanceof Error ? err.message : String(err);
      pushHistory(action, argument || action.subcommand, 0, text, outputKindFor(action.subcommand));
      const isConversation = action.subcommand === "ask" || action.subcommand === "chat";
      setRun({
        kind: "error",
        title: isConversation ? action.label : `${action.label} failed`,
        subtitle: action.subcommand === "ask"
          ? `RAG over ${config.collection || "axon"} | /v1/ask`
          : action.subcommand === "chat"
          ? "/v1/chat"
          : commandLine,
        text,
        outputKind: outputKindFor(action.subcommand),
        prompt: isConversation ? argument : undefined,
        result: { ok: false, status: 0, path: "", method: "POST", payload: null },
      });
    }
  }

  function pushHistory(action: PaletteAction, target: string, status: number, text?: string, outputKind?: "markdown" | "code") {
    setHistory((items) => [
      { action, target, status, text, outputKind, when: "just now", duration: status === 0 ? "fail" : undefined },
      ...items,
    ].slice(0, 18));
  }

  function enterActionMode(action: PaletteAction) {
    setModeAction(action);
    setQuery(parsed.invoked?.subcommand === action.subcommand ? parsed.arg : "");
    setSelected(0);
    setRun({ kind: "idle" });
    setBrowseOpen(true);
    focusInput(true);
  }

  async function saveSettings() {
    if (!draftConfig) return;
    try {
      const nextConfig = await invoke<PaletteConfig>("save_palette_settings", { settings: draftConfig });
      setConfig(nextConfig);
      setDraftConfig(nextConfig);
      setConfigError(null);
      setSettingsOpen(false);
      setBrowseOpen(true);
      focusInput(true);
    } catch (err) {
      setConfigError(String(err));
    }
  }

  function onInputKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSelected((idx) => Math.min(idx + 1, Math.max(filtered.length - 1, 0)));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelected((idx) => Math.max(idx - 1, 0));
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (!active) return;
      if (!modeAction && !parsed.invoked && active.argMode !== "none" && !looksLikeUrl(parsed.search)) {
        enterActionMode(active);
      } else {
        void submit(active);
      }
    } else if (event.key === "Tab") {
      event.preventDefault();
      if (active) enterActionMode(active);
    }
  }

  const outputKind = "outputKind" in run ? run.outputKind : active ? outputKindFor(active.subcommand) : "code";
  const endpointLabel = config ? hostLabel(config.serverUrl) : configError ? "Config error" : "Loading";
  const endpointTone = configError ? "error" : "syncing";
  const showBackButton = settingsOpen || historyOpen || showOutput;
  const currentTarget = currentOutputTarget(run, active, query);

  function goBackToBrowse() {
    setSettingsOpen(false);
    setHistoryOpen(false);
    setRun({ kind: "idle" });
    setModeAction(null);
    setQuery("");
    setBrowseOpen(true);
    focusInput(true);
  }

  function minimizeJob() {
    setSettingsOpen(false);
    setHistoryOpen(false);
    setBrowseOpen(false);
    setQuery("");
    setRun((current) => (current.kind === "job" ? { ...current, minimized: true } : current));
  }

  function expandJob() {
    setBrowseOpen(false);
    setRun((current) => (current.kind === "job" ? { ...current, minimized: false } : current));
  }

  function closeJob() {
    setRun({ kind: "idle" });
    setModeAction(null);
    setQuery("");
    setBrowseOpen(false);
  }

  async function cancelJob() {
    if (run.kind !== "job" || !run.jobId) return;
    const id = run.jobId;
    setCanceling(true);
    try {
      await invoke("axon_http_request", {
        request: { method: "POST", path: `/v1/crawl/${id}/cancel`, body: null },
      });
    } catch {
      /* the poll will surface the canceled row state */
    } finally {
      setCanceling(false);
    }
  }

  async function viewPartialJob() {
    if (run.kind !== "job" || !run.jobId) return;
    const id = run.jobId;
    const host = run.snapshot.host;
    try {
      const res = await invoke<{ ok: boolean; status: number; payload: unknown }>("axon_http_request", {
        request: { method: "GET", path: `/v1/crawl/${id}`, body: null },
      });
      setRun({
        kind: "success",
        title: `Crawl ${host}`,
        subtitle: `job ${id}`,
        text: formatPayload("crawl", res.payload),
        outputKind: "code",
        result: { ok: res.ok, status: res.status, path: `/v1/crawl/${id}`, method: "GET", payload: res.payload },
      });
    } catch {
      /* keep the live view if the fetch fails */
    }
  }

  return (
    <div className={`aurora-page-shell palette-shell${compact ? " palette-shell-compact" : ""}${showResultsLayout ? " palette-shell-results" : " palette-shell-browse"}`}>

      <section className="command-bar">
        {showBackButton && (
          <button className="command-back" type="button" onClick={goBackToBrowse} aria-label="Back" title="Back">
            <ArrowLeft size={17} />
          </button>
        )}
        <button className="axon-brand" type="button" onClick={() => {
          setQuery("");
          setModeAction(null);
          setRun({ kind: "idle" });
          setHistoryOpen(false);
          setBrowseOpen(false);
        }} title={config?.serverUrl ?? endpointLabel} aria-label="Reset Axon palette">
          <AxonMark size={24} />
          <span className="axon-word">Axon</span>
          <span className={`axon-status-dot axon-status-${endpointTone}`} />
          <span className="axon-tip">
            {endpointLabel}
            {config?.collection ? <span>{config.collection}</span> : null}
          </span>
        </button>
        <span className="axon-divider" aria-hidden="true" />
        {modeAction && (
          <button className={`command-mode-pill command-mode-pill-${modeAction.tone}`} type="button" onClick={() => setModeAction(null)} aria-label={`Clear ${modeAction.subcommand} mode`}>
            {modeAction.subcommand}
            <span className="mode-pill-dismiss" aria-hidden="true"><X size={10} /></span>
          </button>
        )}
        <div className="command-input-wrap" onClick={() => {
          setBrowseOpen(true);
          focusInput();
        }}>
          <Search size={16} strokeWidth={1.65} aria-hidden="true" />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onFocus={() => setBrowseOpen(true)}
            onKeyDown={onInputKeyDown}
            placeholder={modeAction ? argumentPlaceholder(modeAction) : hasQuery ? active?.example ?? "Search commands" : "Search or run an operation — scrape, crawl, map, ask…"}
            className="command-input"
            aria-label={modeAction ? `${modeAction.label} argument` : "Axon command"}
          />
        </div>
        <button
          className={active && !validation ? `command-submit command-submit-${active.tone}` : "command-submit"}
          type="button"
          onClick={() => active && void submit(active)}
          disabled={!client || !active || run.kind === "running" || run.kind === "streaming" || Boolean(validation)}
          aria-label="Run selected action"
          title={validation || "Run selected action"}
        >
          <Send size={15} />
        </button>
        <button
          className={settingsOpen ? "command-settings command-settings-active" : "command-settings"}
          type="button"
          onClick={() => setSettingsOpen((open) => {
            const next = !open;
            setHistoryOpen(false);
            if (!next) setBrowseOpen(true);
            return next;
          })}
          aria-label="Settings"
          title="Settings"
        >
          <Settings size={15} />
        </button>
      </section>

      {jobMinimized && run.kind === "job" && (
        <button className="idle-tray" type="button" onClick={expandJob} title="Expand crawl job">
          <span className="idle-tray-dot" />
          <Workflow size={14} strokeWidth={1.9} />
          <span>Crawling {run.snapshot.host}</span>
          <span className="idle-tray-bar">
            <span style={{ width: `${Math.max(2, Math.round(run.snapshot.percent))}%` }} />
          </span>
          <strong>{Math.round(run.snapshot.percent)}%</strong>
          <ChevronRight size={15} />
        </button>
      )}

      {settingsOpen && draftConfig && (
        <SettingsPanel
          configError={configError}
          draftConfig={draftConfig}
          shortcutOptions={shortcutOptions}
          onChange={setDraftConfig}
          onClose={() => {
            setSettingsOpen(false);
            setHistoryOpen(false);
            setBrowseOpen(true);
          }}
          onSave={() => void saveSettings()}
        />
      )}

      {showContent && !settingsOpen && (
      <main className={showResultsLayout ? (showActionPanel ? "palette-grid" : "palette-grid palette-grid-output-only") : "palette-suggestions"}>
        {showActionPanel && (
        <section className="action-panel">
          <div className="panel-heading">
            <span>Actions</span>
            <span className="panel-shortcuts">
              <span><kbd>tab</kbd> switch</span>
              <span><kbd>↵</kbd> run</span>
            </span>
          </div>
          <ScrollArea className="action-scroll" viewportClassName="action-scroll-viewport">
            <div className="action-list">
              {filtered.map((action, index) => {
                const meta = actionDisplayMeta(action);
                const previous = index > 0 ? actionDisplayMeta(filtered[index - 1]) : null;
                const selectedRow = index === selected;
                return (
                  <div className="action-group-item" key={action.subcommand}>
                    {(!previous || previous.category !== meta.category) && (
                      <div className="action-section-heading">
                        <span>{meta.category}</span>
                        <span>{meta.input} → {meta.output}</span>
                      </div>
                    )}
                    <button
                      className={selectedRow ? "action-row action-row-selected" : "action-row"}
                      onClick={() => {
                        setSelected(index);
                        if (parsed.invoked) {
                          void submit(action);
                        } else if (acceptsDirectUrl(action) && looksLikeUrl(parsed.search)) {
                          void submit(action);
                        } else {
                          enterActionMode(action);
                        }
                      }}
                    >
                      <ActionIcon action={action} selected={selectedRow} />
                      <span className="action-main">
                        <span className="action-title-line">
                          <span className="action-label">{meta.label}</span>
                          <span className="action-method">{meta.method}</span>
                          <span className="action-endpoint">{meta.endpoint}</span>
                          {action.subcommand === "crawl" || action.subcommand === "ingest" || action.subcommand === "embed" || action.subcommand === "extract" ? (
                            <span className="action-async">ASYNC</span>
                          ) : null}
                        </span>
                        <span className="action-description">{action.description}</span>
                      </span>
                      <span className="action-meta">
                        {selectedRow ? (
                          <span className="action-run-pill">Run <kbd>↵</kbd></span>
                        ) : (
                          <kbd>{action.subcommand}</kbd>
                        )}
                      </span>
                    </button>
                  </div>
                );
              })}
            </div>
          </ScrollArea>
        </section>
        )}

        {historyOpen && (
          <HistoryPanel
            items={history}
            onClear={() => setHistory([])}
            onOpen={(item) => {
              setHistoryOpen(false);
              setSettingsOpen(false);
              setModeAction(item.action);
              setQuery(item.target);
              if (item.text) {
                const ok = item.status >= 200 && item.status < 300;
                setRun({
                  kind: ok ? "success" : "error",
                  title: `${item.action.label} ${ok ? "completed" : "failed"}`,
                  subtitle: item.target,
                  text: item.text,
                  outputKind: item.outputKind ?? outputKindFor(item.action.subcommand),
                  result: {
                    ok,
                    status: item.status,
                    path: item.action.subcommand,
                    method: "POST",
                    payload: null,
                  },
                });
              } else if (item.running) {
                setRun({
                  kind: "running",
                  title: `Running ${item.action.label}`,
                  subtitle: item.target,
                });
              }
            }}
          />
        )}

        {showResultsLayout && !historyOpen && run.kind === "job" && (
          <CrawlJobView
            snapshot={run.snapshot}
            nowMs={nowMs}
            canceling={canceling}
            onCancel={() => void cancelJob()}
            onViewPartial={() => void viewPartialJob()}
            onMinimize={minimizeJob}
            onClose={closeJob}
          />
        )}

        {showResultsLayout && !historyOpen && run.kind !== "job" && (
          <OutputPanel
            active={active}
            copied={copied}
            outputKind={outputKind}
            run={run}
            onCopy={(text) => void copyOutput(text)}
            onRetry={() => active && void submit(active)}
            onFollowUp={(text) => {
              const askAction = ACTIONS.find((action) => action.subcommand === "ask");
              if (!askAction) return;
              setModeAction(askAction);
              setQuery(text);
              void submit(askAction, text);
            }}
            onHistory={() => {
              setRun({ kind: "idle" });
              setSettingsOpen(false);
              setHistoryOpen(true);
              setBrowseOpen(false);
            }}
            onCollapse={() => {
              setRun({ kind: "idle" });
              setModeAction(null);
              setQuery("");
              setHistoryOpen(false);
              setBrowseOpen(false);
            }}
            onTogglePin={() => {
              if (!currentTarget) return;
              setPinnedTargets((items) => {
                const next = new Set(items);
                next.has(currentTarget) ? next.delete(currentTarget) : next.add(currentTarget);
                return next;
              });
              setHistory((items) => items.map((item) => item.target === currentTarget ? { ...item, pinned: !pinnedTargets.has(currentTarget) } : item));
            }}
            pinned={currentTarget ? pinnedTargets.has(currentTarget) : false}
          />
        )}
      </main>
      )}

      {showContent && !settingsOpen && (
        <footer className="palette-footer">
          <span className="palette-footer-hints">
            <button className="palette-recent" type="button" onClick={() => {
              setRun({ kind: "idle" });
              setModeAction(null);
              setHistoryOpen((open) => !open);
              setBrowseOpen(false);
            }}>↺ recent</button>
            <span className="palette-hint-group"><kbd>↑</kbd><kbd>↓</kbd> navigate</span>
            <span className="palette-hint-group"><kbd>tab</kbd> select</span>
            <span className="palette-hint-group"><kbd>↵</kbd> run</span>
            <span className="palette-hint-group"><kbd>esc</kbd> close</span>
          </span>
          <span className="palette-status" aria-label="Palette controls">
            {config ? (
              <StatusIndicator tone="syncing" label={`${hostLabel(config.serverUrl)} / ${config.collection}`} pulse={false} />
            ) : configError ? (
              <StatusIndicator tone="error" label="Config error" />
            ) : (
              <StatusIndicator tone="syncing" label="Loading" />
            )}
            <button className="titlebar-button" type="button" onClick={() => setSettingsOpen((open) => !open)} aria-label="Settings">
              <Settings size={14} />
            </button>
            <button className="titlebar-button" type="button" onClick={() => void invoke("hide_palette")} aria-label="Hide palette">
              <X size={14} />
            </button>
          </span>
        </footer>
      )}
    </div>
  );

  async function copyOutput(text: string) {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  }
}

function currentOutputTarget(run: RunState, active: PaletteAction | null | undefined, query: string): string {
  if (run.kind === "idle") return query.trim() || active?.subcommand || "";
  if ("result" in run) {
    const payload = run.result.payload;
    if (payload && typeof payload === "object") {
      const record = payload as Record<string, unknown>;
      const url = record.url ?? record.requested_url ?? record.target;
      if (typeof url === "string" && url) return url;
    }
  }
  if ("text" in run) return firstUrlFromText(run.text) ?? run.subtitle;
  return run.subtitle;
}

function firstUrlFromText(value: string): string | null {
  return value.match(/https?:\/\/[^\s"')\]}]+/i)?.[0] ?? null;
}

function AxonMark({ size = 24 }: { size?: number }) {
  return (
    <svg className="axon-mark" width={size} height={size} viewBox="0 0 64 64" fill="none" aria-hidden="true">
      <g stroke="var(--aurora-border-strong)" strokeWidth="2" strokeLinecap="round">
        <path d="M22 9 Q28 14 31 17" />
        <path d="M32 7 L32 16" />
        <path d="M42 9 Q36 14 33 17" />
      </g>
      <line x1="32" y1="22" x2="32" y2="42" stroke="var(--aurora-border-strong)" strokeWidth="2" strokeDasharray="2.5 3.5" />
      <circle className="axon-node axon-node-1" cx="32" cy="20" r="5.2" fill="var(--aurora-border-strong)" stroke="var(--aurora-accent-strong)" strokeWidth="1.8" />
      <circle className="axon-node axon-node-2" cx="32" cy="30" r="5.2" fill="var(--aurora-accent-deep)" stroke="var(--aurora-accent-strong)" strokeWidth="1.8" />
      <circle className="axon-node axon-node-3" cx="32" cy="40" r="5.2" fill="var(--aurora-accent-primary)" stroke="var(--aurora-accent-strong)" strokeWidth="1.8" />
      <circle className="axon-node axon-node-4" cx="32" cy="50" r="5.2" fill="var(--aurora-accent-strong)" />
      <circle cx="32" cy="50" r="8" fill="none" stroke="var(--aurora-accent-strong)" strokeWidth="1.2" opacity="0.4" />
      <g stroke="var(--aurora-accent-strong)" strokeWidth="2" strokeLinecap="round">
        <path d="M28 53 Q23 58 19 62" />
        <path d="M32 54 L32 62" />
        <path d="M36 53 Q41 58 45 62" />
      </g>
    </svg>
  );
}

function ActionIcon({ action, selected }: { action: PaletteAction; selected: boolean }) {
  const Icon = actionIcon(action.subcommand);
  return (
    <span className={`action-icon action-icon-${action.tone}${selected ? " action-icon-selected" : ""}`} aria-hidden="true">
      <Icon size={16} strokeWidth={1.65} />
    </span>
  );
}

function HistoryPanel({
  items,
  onClear,
  onOpen,
}: {
  items: HistoryItem[];
  onClear: () => void;
  onOpen: (item: HistoryItem) => void;
}) {
  return (
    <section className="history-panel">
      <header className="history-head">
        <span>Recent runs</span>
        {items.length > 0 ? <button type="button" onClick={onClear}>clear</button> : null}
      </header>
      {items.length === 0 ? (
        <div className="history-empty">
          <span><Activity size={20} /></span>
          <strong>No runs yet</strong>
          <p>Run an operation and results land here. Start by typing a command above.</p>
        </div>
      ) : (
        <div className="history-list aurora-scrollbar">
          {items.map((item, index) => {
            const ok = item.status >= 200 && item.status < 300;
            return (
              <button className="history-row" type="button" key={`${item.action.subcommand}-${item.target}-${index}`} onClick={() => onOpen(item)}>
                <ActionIcon action={item.action} selected={false} />
                <span className="history-main">
                  <span>{item.target}</span>
                  <span>{item.action.label} · {item.when}</span>
                </span>
                {item.pinned ? <Sparkles className="history-pin" size={13} /> : null}
                {item.running ? (
                  <span className="history-live"><span />live</span>
                ) : (
                  <span className="history-duration">{item.duration ?? "—"}</span>
                )}
                <span className={ok ? "history-status history-status-ok" : "history-status history-status-error"}>{item.status || "ERR"}</span>
              </button>
            );
          })}
        </div>
      )}
    </section>
  );
}

function actionIcon(subcommand: string) {
  switch (subcommand) {
    case "scrape":
      return FileDown;
    case "crawl":
      return Workflow;
    case "map":
      return MapIcon;
    case "summarize":
      return BookOpen;
    case "ask":
    case "chat":
      return Bot;
    case "query":
      return SearchCheck;
    case "retrieve":
      return Database;
    case "suggest":
      return Sparkles;
    case "evaluate":
      return BarChart3;
    case "search":
    case "research":
      return Globe;
    case "embed":
      return Layers;
    case "extract":
      return Braces;
    case "ingest":
      return PackageOpen;
    case "status":
      return Activity;
    case "sources":
      return Boxes;
    case "domains":
      return Database;
    case "stats":
      return BarChart3;
    case "doctor":
      return Stethoscope;
    case "brand":
      return Sparkles;
    case "diff":
      return GitCompare;
    case "screenshot":
      return Camera;
    default:
      return HelpCircle;
  }
}

function normalizeSubmitArgument(action: PaletteAction, argument: string): string {
  const trimmed = argument.trim();
  if (acceptsDirectUrl(action) && trimmed && !/^https?:\/\//i.test(trimmed)) {
    return `https://${trimmed}`;
  }
  return trimmed;
}

function crawlSeedUrl(argument: string): string {
  const first = argument.trim().split(/\s+/)[0] ?? "";
  if (!first) return "";
  return /^https?:\/\//i.test(first) ? first : `https://${first}`;
}

function extractEmbedJobId(payload: unknown): string | null {
  if (!payload || typeof payload !== "object") return null;
  const root = payload as Record<string, unknown>;
  const job = (typeof root.job === "object" && root.job ? root.job : root) as Record<string, unknown>;
  const result = job.result_json;
  if (result && typeof result === "object") {
    const id = (result as Record<string, unknown>).embed_job_id;
    if (typeof id === "string" && id) return id;
  }
  return null;
}
