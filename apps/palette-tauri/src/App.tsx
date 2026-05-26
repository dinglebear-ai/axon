import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Activity,
  CheckCircle2,
  Copy,
  ExternalLink,
  RotateCw,
  Search,
  Send,
  Settings,
  SlidersHorizontal,
  X,
  XCircle,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { Badge } from "@/components/ui/aurora/badge";
import { Button } from "@/components/ui/aurora/button";
import { Input } from "@/components/ui/aurora/input";
import { ScrollArea } from "@/components/ui/aurora/scroll-area";
import { Separator } from "@/components/ui/aurora/separator";
import { Spinner } from "@/components/ui/aurora/spinner";
import { StatusIndicator } from "@/components/ui/aurora/status-indicator";
import {
  type PaletteConfig,
  type PaletteResult,
  createAxonClient,
  executeAction,
} from "@/lib/axonClient";
import {
  ACTIONS,
  type PaletteAction,
  acceptsDirectUrl,
  actionInvokedBy,
  actionMatches,
} from "@/lib/actions";
import { formatPayload } from "@/lib/format";

type RunState =
  | { kind: "idle" }
  | { kind: "running"; title: string; subtitle: string }
  | { kind: "success" | "error"; title: string; subtitle: string; text: string; result: PaletteResult };

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
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    invoke<PaletteConfig>("load_palette_config")
      .then((nextConfig) => {
        setConfig(nextConfig);
        setDraftConfig(nextConfig);
      })
      .catch((err) => setConfigError(String(err)));
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
        ? { width: 760, height: 390 }
        : { width: 640, height: 78 };
    void invoke("resize_palette", size);
  }, [showResultsLayout, showContent]);

  const client = useMemo(() => (config ? createAxonClient(config) : null), [config]);

  async function submit(action: PaletteAction = active) {
    if (!client || !config || !action || run.kind === "running") return;
    const argument = argumentFor(action, modeAction, parsed, query);
    const validation = validationMessage(action, argument);
    if (validation) return;
    const commandLine = `${action.subcommand}${argument ? ` ${argument}` : ""}`;
    setRun({
      kind: "running",
      title: `Running ${action.label}`,
      subtitle: commandLine,
    });
    try {
      const result = await executeAction(client, action, argument, config);
      setRun({
        kind: result.ok ? "success" : "error",
        title: `${action.label} ${result.ok ? "completed" : "failed"}`,
        subtitle: `${result.method} ${result.path} | HTTP ${result.status}`,
        text: formatPayload(action.subcommand, result.payload),
        result,
      });
    } catch (err) {
      setRun({
        kind: "error",
        title: `${action.label} failed`,
        subtitle: commandLine,
        text: err instanceof Error ? err.message : String(err),
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

  const outputText = "text" in run ? run.text : "";
  const outputUrl = outputText ? firstUrl(outputText) : null;

  return (
    <div className={`aurora-page-shell palette-shell${compact ? " palette-shell-compact" : ""}`}>
      {showContent && (
      <header className="palette-titlebar" data-tauri-drag-region>
        <div className="palette-brand" data-tauri-drag-region>
          <span className="brand-dot" />
          <span>Axon Palette</span>
          <Badge tone={configError ? "error" : config ? "info" : "neutral"} shape="tag">
            {config?.shortcut ?? "Loading"}
          </Badge>
        </div>
        <div className="palette-status" data-tauri-drag-region>
          {config ? (
            <StatusIndicator tone="syncing" label={`${hostLabel(config.serverUrl)} / ${config.collection}`} pulse={false} />
          ) : configError ? (
            <StatusIndicator tone="error" label="Config error" />
          ) : (
            <StatusIndicator tone="syncing" label="Loading config" />
          )}
          <button className="titlebar-button" type="button" onClick={() => setSettingsOpen((open) => !open)} aria-label="Settings">
            <Settings size={14} />
          </button>
          <button className="titlebar-button" type="button" onClick={() => void invoke("hide_palette")} aria-label="Hide palette">
            <X size={14} />
          </button>
        </div>
      </header>
      )}

      <section className="command-bar">
        <Search size={16} />
        {modeAction && (
          <button className="command-mode-pill" type="button" onClick={() => setModeAction(null)} title="Clear action mode">
            {modeAction.subcommand}
          </button>
        )}
        <Input
          ref={inputRef}
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
          disabled={!client || !active || run.kind === "running" || Boolean(validation)}
          aria-label="Run selected action"
          title={validation || "Run selected action"}
        >
          {run.kind === "running" ? <Spinner size="sm" tone="rose" /> : <Send size={15} />}
        </button>
      </section>

      {settingsOpen && draftConfig && (
        <section className="settings-panel">
          <div className="settings-heading">
            <SlidersHorizontal size={15} />
            <span>Settings</span>
          </div>
          <label>
            <span>Server</span>
            <input value={draftConfig.serverUrl} onChange={(event) => setDraftConfig({ ...draftConfig, serverUrl: event.target.value })} />
          </label>
          <label>
            <span>Token</span>
            <input
              type="password"
              value={draftConfig.token ?? ""}
              onChange={(event) => setDraftConfig({ ...draftConfig, token: event.target.value || null })}
            />
          </label>
          <label>
            <span>Shortcut</span>
            <select value={draftConfig.shortcut} onChange={(event) => setDraftConfig({ ...draftConfig, shortcut: event.target.value })}>
              {shortcutOptions.map((shortcut) => (
                <option key={shortcut} value={shortcut}>
                  {shortcut}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>Collection</span>
            <input value={draftConfig.collection} onChange={(event) => setDraftConfig({ ...draftConfig, collection: event.target.value })} />
          </label>
          <label>
            <span>Results</span>
            <input
              type="number"
              min={1}
              max={50}
              value={draftConfig.resultLimit}
              onChange={(event) => setDraftConfig({ ...draftConfig, resultLimit: Number(event.target.value) })}
            />
          </label>
          <label>
            <span>Theme</span>
            <select value={draftConfig.theme} onChange={(event) => setDraftConfig({ ...draftConfig, theme: event.target.value as PaletteConfig["theme"] })}>
              <option value="system">System</option>
              <option value="dark">Dark</option>
              <option value="light">Light</option>
            </select>
          </label>
          <label className="settings-check">
            <input
              type="checkbox"
              checked={draftConfig.hideOnBlur}
              onChange={(event) => setDraftConfig({ ...draftConfig, hideOnBlur: event.target.checked })}
            />
            <span>Hide on blur</span>
          </label>
          <div className="settings-actions">
            {configError && <span>{configError}</span>}
            <Button size="sm" variant="neutral" onClick={() => setSettingsOpen(false)}>
              Close
            </Button>
            <Button size="sm" variant="rose" onClick={() => void saveSettings()}>
              Save
            </Button>
          </div>
        </section>
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
                    if (parsed.invoked && run.kind !== "running") {
                      void submit(action);
                    } else if (acceptsDirectUrl(action) && looksLikeUrl(parsed.search) && run.kind !== "running") {
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
        <section className="output-panel">
          <div className="panel-heading">
            <span>Output</span>
            <span className="output-tools">
              {"text" in run && (
                <>
                  <button type="button" onClick={() => void copyOutput(run.text)} title="Copy output" aria-label="Copy output">
                    <Copy size={14} />
                  </button>
                  <button type="button" onClick={() => active && void submit(active)} title="Retry" aria-label="Retry">
                    <RotateCw size={14} />
                  </button>
                </>
              )}
              {outputUrl && (
                <button type="button" onClick={() => window.open(outputUrl, "_blank", "noopener,noreferrer")} title="Open first URL" aria-label="Open first URL">
                  <ExternalLink size={14} />
                </button>
              )}
              {run.kind === "running" ? <Spinner size="sm" /> : run.kind === "success" ? <CheckCircle2 size={15} /> : run.kind === "error" ? <XCircle size={15} /> : <Activity size={15} />}
            </span>
          </div>
          <Separator />
          <div className={`output-state output-${run.kind}`}>
            <div className="output-title">{copied ? "Copied" : outputTitle(run)}</div>
            <div className="output-subtitle">{outputSubtitle(run, active)}</div>
            {"text" in run && (
              <pre className="output-body">
                <code>{run.text}</code>
              </pre>
            )}
          </div>
        </section>
        )}
      </main>
      )}
    </div>
  );

  async function copyOutput(text: string) {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  }
}

function focusInput(select = false) {
  window.setTimeout(() => {
    const input = document.querySelector<HTMLInputElement>(".command-input");
    input?.focus();
    if (select) input?.select();
  }, 30);
}

function parseCommand(raw: string): { invoked?: PaletteAction; search: string; arg: string } {
  const trimmed = raw.trimStart();
  const [token = ""] = trimmed.split(/\s+/);
  const invoked = ACTIONS.find((action) => actionInvokedBy(action, token));
  if (invoked) {
    return { invoked, search: token, arg: trimmed.slice(token.length).trimStart() };
  }
  return { search: trimmed, arg: "" };
}

function argumentFor(
  action: PaletteAction,
  modeAction: PaletteAction | null,
  parsed: { invoked?: PaletteAction; search: string; arg: string },
  query: string,
): string {
  if (modeAction?.subcommand === action.subcommand) return query.trim();
  if (parsed.invoked?.subcommand === action.subcommand) return parsed.arg;
  if (looksLikeUrl(parsed.search) && acceptsDirectUrl(action)) return parsed.search;
  return parsed.search;
}

function validationMessage(action: PaletteAction, argument: string): string {
  if (action.argMode === "none" || action.argMode === "optionalSingle") return "";
  return argument.trim() ? "" : "Argument required";
}

function argumentPlaceholder(action: PaletteAction): string {
  const example = action.example.replace(new RegExp(`^${action.subcommand}\\s*`, "i"), "").trim();
  return example || action.description;
}

function looksLikeUrl(value: string): boolean {
  return /^https?:\/\//i.test(value.trim());
}

function outputTitle(run: RunState): string {
  if (run.kind === "idle") return "Ready";
  return run.title;
}

function outputSubtitle(run: RunState, action: PaletteAction | undefined): string {
  if (run.kind === "idle") return action?.description ?? "No matching action";
  return run.subtitle;
}

function hostLabel(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

function firstUrl(value: string): string | null {
  return value.match(/https?:\/\/[^\s"')\]}]+/i)?.[0] ?? null;
}
