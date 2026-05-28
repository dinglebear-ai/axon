import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Search,
  Send,
  Settings,
  X,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { Badge } from "@/components/ui/aurora/badge";
import { OutputPanel } from "@/components/palette/OutputPanel";
import { SettingsPanel } from "@/components/palette/SettingsPanel";
import { Input } from "@/components/ui/aurora/input";
import { ScrollArea } from "@/components/ui/aurora/scroll-area";
import { Spinner } from "@/components/ui/aurora/spinner";
import { StatusIndicator } from "@/components/ui/aurora/status-indicator";
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
  argumentFor,
  argumentPlaceholder,
  focusInput,
  hostLabel,
  looksLikeUrl,
  parseCommand,
  validationMessage,
} from "@/lib/paletteView";
import type { RunState } from "@/lib/runState";

type PaletteStreamEvent =
  | { type: "started"; requestId: string; path: string }
  | { type: "delta"; requestId: string; text: string }
  | { type: "done"; requestId: string; answer?: string | null }
  | { type: "error"; requestId: string; message: string };

const shortcutOptions = ["Ctrl+Shift+Space", "Alt+Space", "Ctrl+Space", "Cmd+Shift+Space"] as const;
const appWindow = getCurrentWindow();

export default function App() {
  const [query, setQuery] = useState("");
  const [modeAction, setModeAction] = useState<PaletteAction | null>(null);
  const [selected, setSelected] = useState(0);
  const [config, setConfig] = useState<PaletteConfig | null>(null);
  const [draftConfig, setDraftConfig] = useState<PaletteConfig | null>(null);
  const [configError, setConfigError] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [run, setRun] = useState<RunState>({ kind: "idle" });
  const [copied, setCopied] = useState(false);

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
    focusInput();
    const unlisteners = [
      appWindow.listen("palette://shown", () => {
        setQuery("");
        setModeAction(null);
        setSelected(0);
        setRun({ kind: "idle" });
        setSettingsOpen(false);
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
    let disposed = false;
    const unlisten = appWindow.listen<PaletteStreamEvent>("palette://stream", (event) => {
      if (disposed) return;
      const payload = event.payload;
      if (payload.type === "delta") {
        setRun((current) =>
          current.kind === "streaming" && current.requestId === payload.requestId
            ? { ...current, text: current.text + payload.text }
            : current,
        );
      } else if (payload.type === "done") {
        setRun((current) =>
          current.kind === "streaming" && current.requestId === payload.requestId
            ? {
                kind: "success",
                title: "Ask question completed",
                subtitle: current.subtitle,
                text: payload.answer ?? current.text,
                outputKind: current.outputKind,
                result: {
                  ok: true,
                  status: 200,
                  path: "/v1/ask/stream",
                  method: "POST",
                  payload: { answer: payload.answer ?? current.text },
                },
              }
            : current,
        );
      } else if (payload.type === "error") {
        setRun((current) =>
          current.kind === "streaming" && current.requestId === payload.requestId
            ? {
                kind: "error",
                title: "Ask question failed",
                subtitle: "/v1/ask/stream",
                text: payload.message,
                outputKind: current.outputKind,
                result: {
                  ok: false,
                  status: 0,
                  path: "/v1/ask/stream",
                  method: "POST",
                  payload: { error: payload.message },
                },
              }
            : current,
        );
      }
    });
    return () => {
      disposed = true;
      void unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = event.metaKey || event.ctrlKey;
      if (event.key === "Escape") {
        event.preventDefault();
        if (settingsOpen) {
          setSettingsOpen(false);
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
  }, [modeAction, query, run, settingsOpen]);

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

  const parsed = useMemo(() => parseCommand(query), [query]);
  const hasQuery = query.trim().length > 0;
  const filtered = useMemo(() => {
    if (parsed.invoked) return [parsed.invoked];
    return ACTIONS.filter((action) => actionMatches(action, parsed.search)).slice(0, 12);
  }, [parsed.invoked, parsed.search]);
  const suggestedAction = filtered[Math.min(selected, Math.max(filtered.length - 1, 0))];
  const active = modeAction ?? suggestedAction;
  const activeArgument = active ? argumentFor(active, modeAction, parsed, query) : "";
  const validation = active ? validationMessage(active, activeArgument) : "No matching action";
  const showOutput = run.kind !== "idle";
  const showContent = settingsOpen || showOutput || (!modeAction && hasQuery);
  const showActionPanel = !modeAction || settingsOpen;
  const compact = !showContent;
  const showResultsLayout = showOutput || settingsOpen;

  useEffect(() => {
    setSelected(0);
  }, [parsed.search, modeAction]);

  useEffect(() => {
    const size = showResultsLayout
      ? { width: 900, height: 560 }
      : showContent
        ? { width: 760, height: Math.min(142 + filtered.length * 48, window.screen.availHeight - 80) }
        : { width: 640, height: 78 };
    void invoke("resize_palette", size);
  }, [showResultsLayout, showContent, filtered.length]);

  const client = useMemo(() => (config ? createAxonClient(config) : null), [config]);

  async function submit(action: PaletteAction = active) {
    if (!client || !config || !action || run.kind === "running" || run.kind === "streaming") return;
    const argument = argumentFor(action, modeAction, parsed, query);
    const validation = validationMessage(action, argument);
    if (validation) return;
    const commandLine = `${action.subcommand}${argument ? ` ${argument}` : ""}`;
    if (action.subcommand === "ask") {
      const requestId = newRequestId();
      const request = buildActionRequest(client, action, argument, config);
      setRun({
        kind: "streaming",
        title: "Streaming Ask question",
        subtitle: `${request.method} /v1/ask/stream`,
        text: "",
        outputKind: outputKindFor(action.subcommand),
        requestId,
      });
      try {
        await invoke("axon_http_stream_request", {
          request: {
            ...request,
            requestId,
            path: "/v1/ask/stream",
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
                subtitle: `${request.method} /v1/ask/stream`,
                text: message,
                outputKind: outputKindFor(action.subcommand),
                result: {
                  ok: false,
                  status: 0,
                  path: "/v1/ask/stream",
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
        title: `Running ${action.label}`,
        subtitle: commandLine,
      });
    }
    try {
      const result = await executeAction(client, action, argument, config);
      setRun({
        kind: result.ok ? "success" : "error",
        title: `${action.label} ${result.ok ? "completed" : "failed"}`,
        subtitle: `${result.method} ${result.path} | HTTP ${result.status}`,
        text: formatPayload(action.subcommand, result.payload),
        outputKind: outputKindFor(action.subcommand),
        result,
      });
    } catch (err) {
      setRun({
        kind: "error",
        title: `${action.label} failed`,
        subtitle: commandLine,
        text: err instanceof Error ? err.message : String(err),
        outputKind: outputKindFor(action.subcommand),
        result: { ok: false, status: 0, path: "", method: "POST", payload: null },
      });
    }
  }

  function enterActionMode(action: PaletteAction) {
    setModeAction(action);
    setQuery(parsed.invoked?.subcommand === action.subcommand ? parsed.arg : "");
    setSelected(0);
    setRun({ kind: "idle" });
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
      if (!modeAction && active.argMode !== "none" && !looksLikeUrl(parsed.search)) {
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

  return (
    <div className={`aurora-page-shell palette-shell${compact ? " palette-shell-compact" : ""}`}>

      <section className="command-bar">
        <Search size={16} aria-hidden="true" />
        {modeAction && (
          <button className="command-mode-pill" type="button" onClick={() => setModeAction(null)} aria-label={`Clear ${modeAction.subcommand} mode`}>
            {modeAction.subcommand}
            <span className="mode-pill-dismiss" aria-hidden="true">×</span>
          </button>
        )}
        <Input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={onInputKeyDown}
          placeholder={modeAction ? argumentPlaceholder(modeAction) : active?.example ?? "Search commands"}
          className="command-input"
          aria-label={modeAction ? `${modeAction.label} argument` : "Axon command"}
        />
        <button
          className="command-submit"
          type="button"
          onClick={() => active && void submit(active)}
          disabled={!client || !active || run.kind === "running" || run.kind === "streaming" || Boolean(validation)}
          aria-label="Run selected action"
          title={validation || "Run selected action"}
        >
          {run.kind === "running" || run.kind === "streaming" ? <Spinner size="sm" tone="cyan" /> : <Send size={15} />}
        </button>
      </section>

      {settingsOpen && draftConfig && (
        <SettingsPanel
          configError={configError}
          draftConfig={draftConfig}
          shortcutOptions={shortcutOptions}
          onChange={setDraftConfig}
          onClose={() => setSettingsOpen(false)}
          onSave={() => void saveSettings()}
        />
      )}

      {showContent && (
      <main className={showResultsLayout ? (showActionPanel ? "palette-grid" : "palette-grid palette-grid-output-only") : "palette-suggestions"}>
        {showActionPanel && (
        <section className="action-panel">
          <div className="panel-heading">
            <span>Actions</span>
            <span>{validation || `${filtered.length} matches`}</span>
          </div>
          <ScrollArea className="action-scroll" viewportClassName="action-scroll-viewport">
            <div className="action-list">
              {filtered.map((action, index) => (
                <button
                  key={action.subcommand}
                  className={index === selected ? "action-row action-row-selected" : "action-row"}
                  onClick={() => {
                    setSelected(index);
                    if (parsed.invoked && run.kind !== "running" && run.kind !== "streaming") {
                      void submit(action);
                    } else if (acceptsDirectUrl(action) && looksLikeUrl(parsed.search) && run.kind !== "running" && run.kind !== "streaming") {
                      void submit(action);
                    } else {
                      enterActionMode(action);
                    }
                  }}
                >
                  <span className="action-main">
                    <span className="action-label">{action.label}</span>
                    <span className="action-description">{action.description}</span>
                  </span>
                  <span className="action-meta">
                    <kbd>Enter</kbd>
                    <Badge tone={action.tone} shape="tag">
                      {action.subcommand}
                    </Badge>
                  </span>
                </button>
              ))}
            </div>
          </ScrollArea>
        </section>
        )}

        {showResultsLayout && (
          <OutputPanel
            active={active}
            copied={copied}
            outputKind={outputKind}
            run={run}
            onCopy={(text) => void copyOutput(text)}
            onRetry={() => active && void submit(active)}
          />
        )}
      </main>
      )}

      {showContent && (
        <footer className="palette-footer">
          <span className="palette-footer-hints">
            ↑↓ navigate · ↵ select · Tab mode · Esc close
          </span>
          <span className="palette-status">
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

function newRequestId(): string {
  return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}
