import {
  AlertTriangle,
  Check,
  DatabaseZap,
  Dog,
  LockKeyhole,
  Search,
  Send,
  ShieldAlert,
  TerminalSquare,
} from "lucide-react";
import { ACTIONS } from "@/lib/actions";
import { AxonMark } from "./AxonMark";
import { CORTEX_PALETTE_ACTIONS } from "./cortexPaletteActions";
import type { Source, UnifiedAction } from "./LabbyCortexFixture";
import { LABBY_PALETTE_ACTIONS } from "./labbyPaletteActions";

export function Catalog({
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

export function ActionScreen({
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

export function ActionResult({
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
export function argumentPlaceholder(action: UnifiedAction): string {
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

export function SourceIcon({ source, size }: { source: Source; size: number }) {
  if (source === "all") return <Search size={size} />;
  if (source === "axon") return <AxonMark size={size} />;
  if (source === "labby") return <Dog size={size} />;
  return <DatabaseZap size={size} />;
}
