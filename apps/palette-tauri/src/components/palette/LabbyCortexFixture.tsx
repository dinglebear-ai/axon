import {
  AlertTriangle,
  ArrowLeft,
  Check,
  ChevronDown,
  CircleHelp,
  DatabaseZap,
  Dog,
  Filter,
  LockKeyhole,
  Search,
  Send,
  Settings,
  ShieldAlert,
  TerminalSquare,
} from "lucide-react";
import { useMemo, useState } from "react";

import { actionDisplayMeta } from "@/lib/actionMeta";
import { ACTIONS } from "@/lib/actions";

import { AxonMark } from "./AxonMark";
import {
  CORTEX_CATEGORIES,
  CORTEX_PALETTE_ACTIONS,
  type CortexPaletteAction,
} from "./cortexPaletteActions";
import {
  LABBY_PALETTE_ACTIONS,
  LABBY_SERVICES,
  type LabbyPaletteAction,
} from "./labbyPaletteActions";
import "./labby-cortex-fixture.css";

type Source = "all" | "axon" | "labby" | "cortex";
type UnifiedAction = {
  id: string;
  label: string;
  description: string;
  group: string;
  source: Source;
  admin: boolean;
  destructive: boolean;
  cost?: string;
};

const cortexActions: UnifiedAction[] = CORTEX_PALETTE_ACTIONS.map((item: CortexPaletteAction) => ({
  ...item,
  group: item.category,
  source: "cortex",
  destructive: item.cost === "write",
  admin: item.admin ?? false,
}));
const labbyActions: UnifiedAction[] = LABBY_PALETTE_ACTIONS.map((item: LabbyPaletteAction) => ({
  ...item,
  group: item.service,
  source: "labby",
  description: labbyDescription(item),
  cost: item.destructive ? "write" : "cheap",
}));
const axonActions: UnifiedAction[] = ACTIONS.map((item) => ({
  id: item.subcommand,
  label: item.label,
  description: item.description,
  group: actionDisplayMeta(item).category,
  source: "axon",
  admin: item.kind === "admin",
  destructive: item.kind === "admin" && ["jobs-clear", "jobs-cleanup"].includes(item.subcommand),
  cost: item.kind === "job" ? "moderate" : "cheap",
}));

function labbyDescription(item: LabbyPaletteAction): string {
  const target = item.id.split(".").slice(1).join(" · ").replaceAll("_", " ");
  return `${item.service.replaceAll("_", " ")} · ${target}`;
}

export function LabbyCortexFixture() {
  const [source, setSource] = useState<Source>("all");
  const [group, setGroup] = useState("All");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<UnifiedAction | null>(null);
  const [result, setResult] = useState(false);
  const [filtersOpen, setFiltersOpen] = useState(false);

  const catalog =
    source === "all"
      ? [...axonActions, ...labbyActions, ...cortexActions]
      : source === "axon"
        ? axonActions
        : source === "cortex"
          ? cortexActions
          : labbyActions;
  const groups =
    source === "all"
      ? []
      : source === "axon"
        ? [...new Set(axonActions.map((item) => item.group))]
        : source === "cortex"
          ? [...CORTEX_CATEGORIES]
          : LABBY_SERVICES;
  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return catalog.filter(
      (item) =>
        (group === "All" || item.group === group) &&
        (!needle || `${item.id} ${item.label} ${item.description}`.toLowerCase().includes(needle)),
    );
  }, [catalog, group, query]);

  function choose(item: UnifiedAction) {
    setSelected(item);
    setResult(false);
    setQuery("");
    setFiltersOpen(false);
  }
  function reset() {
    setSelected(null);
    setResult(false);
    setQuery("");
    setFiltersOpen(false);
  }
  function switchSource(next: Source) {
    setSource(next);
    setGroup("All");
    setSelected(null);
    setResult(false);
    setQuery("");
  }

  return (
    <main className="lcp-stage">
      <section className="lcp-shell lcp-shell-open">
        <div className="lcp-command-bar">
          {selected && (
            <button className="lcp-icon-button" type="button" onClick={reset} aria-label="Back">
              <ArrowLeft size={17} />
            </button>
          )}
          <button className="lcp-brand" type="button" onClick={reset} aria-label="Reset palette">
            <AxonMark size={24} />
            <i title="Connected" />
          </button>
          <span className="lcp-divider" />
          <div className="lcp-input-wrap">
            {selected && (
              <button
                className={`lcp-action-chip lcp-${selected.source}`}
                type="button"
                onClick={reset}
              >
                <SourceIcon source={selected.source} size={14} />
                <span>{selected.label}</span>
                <ChevronDown size={12} />
              </button>
            )}
            <Search size={15} />
            <input
              aria-label="Palette search"
              value={query}
              placeholder={
                selected
                  ? argumentPlaceholder(selected)
                  : source === "all"
                    ? `Search all ${catalog.length} actions…`
                    : `Search ${catalog.length} ${source} actions…`
              }
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && selected) setResult(true);
                if (event.key === "Escape") reset();
              }}
            />
          </div>
          {!selected && (
            <button
              className={`lcp-icon-button lcp-filter-button ${filtersOpen ? "is-active" : ""}`}
              type="button"
              aria-label="Filter actions"
              aria-expanded={filtersOpen}
              onClick={() => setFiltersOpen((open) => !open)}
            >
              <Filter size={17} />
              {(source !== "all" || group !== "All") && <i />}
            </button>
          )}
          <button className="lcp-icon-button" type="button" aria-label="Help">
            <CircleHelp size={17} />
          </button>
          <button
            className="lcp-send"
            type="button"
            aria-label="Run"
            disabled={!selected}
            onClick={() => selected && setResult(true)}
          >
            <Send size={16} />
          </button>
          <button className="lcp-icon-button" type="button" aria-label="Settings">
            <Settings size={17} />
          </button>
        </div>
        {!selected ? (
          <Catalog
            source={source}
            group={group}
            query={query}
            groups={groups}
            visible={visible}
            total={catalog.length}
            filtersOpen={filtersOpen}
            onSource={switchSource}
            onGroup={setGroup}
            onChoose={choose}
          />
        ) : result ? (
          <ActionResult action={selected} query={query} onEdit={() => setResult(false)} />
        ) : (
          <ActionScreen
            action={selected}
            query={query}
            onQuery={setQuery}
            onRun={() => setResult(true)}
          />
        )}
      </section>
      <p className="lcp-caption">
        Live action atlas · {LABBY_PALETTE_ACTIONS.length} Labby actions ·{" "}
        {CORTEX_PALETTE_ACTIONS.length} Cortex actions · {ACTIONS.length} Axon actions
      </p>
    </main>
  );
}

function Catalog({
  source,
  group,
  query,
  groups,
  visible,
  total,
  filtersOpen,
  onSource,
  onGroup,
  onChoose,
}: {
  source: Source;
  group: string;
  query: string;
  groups: readonly string[];
  visible: UnifiedAction[];
  total: number;
  filtersOpen: boolean;
  onSource: (source: Source) => void;
  onGroup: (group: string) => void;
  onChoose: (item: UnifiedAction) => void;
}) {
  return (
    <div className="lcp-picker lcp-atlas">
      {filtersOpen && (
        <div className="lcp-filter-popover">
          <header className="lcp-source-tabs">
            <button
              className={source === "all" ? "is-active" : ""}
              type="button"
              onClick={() => onSource("all")}
            >
              All{" "}
              <span>
                {ACTIONS.length + LABBY_PALETTE_ACTIONS.length + CORTEX_PALETTE_ACTIONS.length}
              </span>
            </button>
            <button
              className={source === "axon" ? "is-active" : ""}
              type="button"
              onClick={() => onSource("axon")}
            >
              <AxonMark size={15} /> Axon <span>{ACTIONS.length}</span>
            </button>
            <button
              className={source === "labby" ? "is-active" : ""}
              type="button"
              onClick={() => onSource("labby")}
            >
              <Dog size={15} /> Labby <span>{LABBY_PALETTE_ACTIONS.length}</span>
            </button>
            <button
              className={source === "cortex" ? "is-active" : ""}
              type="button"
              onClick={() => onSource("cortex")}
            >
              <DatabaseZap size={15} /> Cortex <span>{CORTEX_PALETTE_ACTIONS.length}</span>
            </button>
            <p>{query ? `${visible.length} matches` : `${total} registered actions`}</p>
          </header>
          {source !== "all" && (
            <nav className="lcp-group-tabs">
              <button
                className={group === "All" ? "is-active" : ""}
                type="button"
                onClick={() => onGroup("All")}
              >
                All
              </button>
              {groups.map((item) => (
                <button
                  className={group === item ? "is-active" : ""}
                  type="button"
                  key={item}
                  onClick={() => onGroup(item)}
                >
                  {item.replaceAll("_", " ")}
                </button>
              ))}
            </nav>
          )}
        </div>
      )}
      <div className="lcp-action-list">
        {visible.map((item) => (
          <button type="button" key={`${item.source}-${item.id}`} onClick={() => onChoose(item)}>
            <span className={`lcp-action-glyph lcp-${item.source}`}>
              <SourceIcon source={item.source} size={16} />
            </span>
            <span className="lcp-action-copy">
              <strong>{item.label}</strong>
              <small>{item.description}</small>
            </span>
            <code>{item.id}</code>
            <span className="lcp-badges">
              {item.admin && (
                <em className="is-admin">
                  <LockKeyhole size={10} /> admin
                </em>
              )}
              {item.destructive && (
                <em className="is-danger">
                  <ShieldAlert size={10} /> write
                </em>
              )}
              {item.cost === "expensive" && <em>heavy</em>}
            </span>
          </button>
        ))}
      </div>
      <footer>
        <span>
          {visible.length} shown · {source === "all" ? "all sources" : source}
          {group !== "All" ? ` / ${group}` : ""}
        </span>
        <span>
          <kbd>↑↓</kbd> Navigate
        </span>
        <span>
          <kbd>↵</kbd> Select
        </span>
        <span>
          <kbd>esc</kbd> Close
        </span>
      </footer>
    </div>
  );
}

function ActionScreen({
  action,
  query,
  onQuery,
  onRun,
}: {
  action: UnifiedAction;
  query: string;
  onQuery: (value: string) => void;
  onRun: () => void;
}) {
  const fields = fieldsFor(action);
  return (
    <div className="lcp-action-screen">
      <header>
        <span className={`lcp-action-glyph lcp-${action.source}`}>
          <SourceIcon source={action.source} size={18} />
        </span>
        <div>
          <p>
            {action.source} / {action.group}
          </p>
          <h2>{action.label}</h2>
          <small>{action.description}</small>
        </div>
        <code>{action.id}</code>
      </header>
      {(action.admin || action.destructive) && (
        <div className={`lcp-policy ${action.destructive ? "is-danger" : ""}`}>
          <AlertTriangle size={16} />
          <span>
            <strong>
              {action.destructive ? "Confirmation required" : "Admin authorization required"}
            </strong>
            <small>
              {action.destructive
                ? "This action can change or remove durable state. The palette will show a final review before execution."
                : "This action requires an elevated token or admin session."}
            </small>
          </span>
        </div>
      )}
      <div className="lcp-form-grid">
        {fields.map((field, index) => (
          <label key={field}>
            <span>
              {field}
              {index === 0 && <em> primary</em>}
            </span>
            <input
              value={index === 0 ? query : ""}
              placeholder={exampleFor(field, action)}
              onChange={index === 0 ? (event) => onQuery(event.target.value) : undefined}
            />
          </label>
        ))}
      </div>
      <footer>
        <div>
          <kbd>esc</kbd> Back to actions
        </div>
        <button type="button" onClick={onRun}>
          {action.destructive ? "Review action" : "Run action"}
          <Send size={14} />
        </button>
      </footer>
    </div>
  );
}

function ActionResult({
  action,
  query,
  onEdit,
}: {
  action: UnifiedAction;
  query: string;
  onEdit: () => void;
}) {
  const logLike =
    action.id.includes("search") ||
    action.id.includes("tail") ||
    action.id.includes("errors") ||
    action.id.includes("logs");
  return (
    <div className="lcp-result">
      <header>
        <div className={`lcp-result-status lcp-${action.source}`}>
          <Check size={14} />
          <strong>{action.label}</strong>
          <span>completed · 184 ms</span>
        </div>
        <button type="button" onClick={onEdit}>
          Edit arguments
        </button>
      </header>
      <div className="lcp-result-context">
        <SourceIcon source={action.source} size={15} />
        <span>{action.source}</span>
        <code>
          {action.id} {query}
        </code>
        <strong>{logLike ? "18 rows" : "Success"}</strong>
      </div>
      {logLike ? (
        <div className="lcp-log-rows">
          <div className="is-error">
            <time>14:32:09.481</time>
            <span>devhost</span>
            <strong>plex-sync</strong>
            <code>POST /api/sync 401</code>
            <p>token expired: credential plex_service</p>
          </div>
          <div>
            <time>14:35:02.019</time>
            <span>devhost</span>
            <strong>plex-sync</strong>
            <code>GET /health 200</code>
            <p>ready; upstream authenticated</p>
          </div>
        </div>
      ) : (
        <div className="lcp-generic-result">
          <TerminalSquare size={20} />
          <div>
            <strong>{action.label} completed</strong>
            <p>
              The action-specific renderer is connected to the shared result contract. Structured
              fields for <code>{action.id}</code> appear here.
            </p>
          </div>
        </div>
      )}
    </div>
  );
}

function fieldsFor(action: UnifiedAction): string[] {
  const id = action.id;
  if (id.includes("search") || id.includes("query")) return ["query", "since", "limit"];
  if (id.includes("tail")) return ["host or service", "since", "limit"];
  if (
    id.includes("get") ||
    id.includes("remove") ||
    id.includes("update") ||
    id.includes("enable") ||
    id.includes("disable") ||
    id.includes("restart")
  )
    return ["name or id"];
  if (id.includes("incident") || id.includes("investigate"))
    return ["incident, tool, or subject", "since", "limit"];
  if (id.includes("add") || id.includes("create") || id.includes("set") || id.includes("record"))
    return ["name", "configuration (JSON)"];
  if (id.includes("status") || id.includes("list") || id.endsWith("help") || id.endsWith("schema"))
    return ["optional filter"];
  return ["arguments", "since", "limit"];
}
function argumentPlaceholder(action: UnifiedAction): string {
  return `Arguments for ${action.id}…`;
}
function exampleFor(field: string, action: UnifiedAction): string {
  if (field === "since") return "1h";
  if (field === "limit") return "50";
  if (field.includes("JSON")) return "{ }";
  if (field.includes("host")) return "devhost";
  if (field === "query")
    return action.source === "cortex" ? "oom killer host:devhost" : "github issues";
  return "Optional";
}

function SourceIcon({ source, size }: { source: Source; size: number }) {
  if (source === "all") return <Search size={size} />;
  if (source === "axon") return <AxonMark size={size} />;
  if (source === "labby") return <Dog size={size} />;
  return <DatabaseZap size={size} />;
}
