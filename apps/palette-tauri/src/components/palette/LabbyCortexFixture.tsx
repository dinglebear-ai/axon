import { ArrowLeft, ChevronDown, CircleHelp, Filter, Search, Send, Settings } from "lucide-react";
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
import {
  ActionResult,
  ActionScreen,
  argumentPlaceholder,
  Catalog,
  SourceIcon,
} from "./LabbyCortexFixtureViews";

export type Source = "all" | "axon" | "labby" | "cortex";
export type UnifiedAction = {
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
