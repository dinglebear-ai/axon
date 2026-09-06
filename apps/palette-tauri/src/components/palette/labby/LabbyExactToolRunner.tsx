import { useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/aurora/button";
import type { BackendProfile } from "@/lib/backendProfiles/model";
import {
  type LabbyCatalogEntry,
  LabbyClient,
  type LabbyExactResult,
  type LabbyToolDescriptor,
} from "@/lib/clients/labbyClient";
import { inertResultPreview, parseRawArguments, schemaFields } from "@/lib/labby/schemaForm";

const ROW_HEIGHT = 58;
const VISIBLE_ROWS = 9;

export function LabbyExactToolRunner({ profile }: { profile: BackendProfile }) {
  const client = useMemo(() => new LabbyClient(profile), [profile]);
  const [query, setQuery] = useState("");
  const [entries, setEntries] = useState<LabbyCatalogEntry[]>([]);
  const [selected, setSelected] = useState<LabbyToolDescriptor | null>(null);
  const [values, setValues] = useState<Record<string, unknown>>({});
  const [raw, setRaw] = useState("{}");
  const [rawMode, setRawMode] = useState(false);
  const [confirmed, setConfirmed] = useState(false);
  const [result, setResult] = useState<LabbyExactResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [running, setRunning] = useState(false);
  const [scrollTop, setScrollTop] = useState(0);
  const generation = useRef(0);

  useEffect(() => {
    const controller = new AbortController();
    const current = ++generation.current;
    setLoading(true);
    const timer = window.setTimeout(() => {
      void client
        .search(query, controller.signal)
        .then((catalog) => {
          if (generation.current === current)
            setEntries(catalog.entries.filter((entry) => entry.kind === "mcpTool"));
        })
        .catch((reason) => {
          if (reason instanceof DOMException && reason.name === "AbortError") return;
          if (generation.current === current) setError(String(reason));
        })
        .finally(() => {
          if (generation.current === current) setLoading(false);
        });
    }, 180);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [client, query]);

  async function choose(entry: LabbyCatalogEntry) {
    setError(null);
    setResult(null);
    setConfirmed(false);
    setLoading(true);
    try {
      const descriptor = await client.descriptor(entry.id);
      setSelected(descriptor);
      setValues({});
      setRaw("{}");
      setRawMode(Boolean(schemaFields(descriptor.inputSchema).rawOnlyReason));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }

  async function execute() {
    if (!selected) return;
    setError(null);
    setResult(null);
    setRunning(true);
    try {
      const params = rawMode ? parseRawArguments(raw) : values;
      setResult(await client.execute(selected, params, confirmed));
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setRunning(false);
    }
  }

  const form = schemaFields(selected?.inputSchema ?? null);
  const start = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - 2);
  const visible = entries.slice(start, start + VISIBLE_ROWS + 4);
  return (
    <section className="labby-runner" aria-label="Labby exact tool runner">
      <header className="labby-runner-header">
        <div>
          <strong>Labby exact tools</strong>
          <span>{profile.label} · direct, no LLM</span>
        </div>
      </header>
      <div className="labby-runner-grid">
        <aside className="labby-catalog">
          <input
            aria-label="Search live Labby tools"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search live tools…"
          />
          {loading && <p role="status">Loading live catalog…</p>}
          {!loading && entries.length === 0 && <p>No exposed tools are available.</p>}
          <div
            className="labby-tool-viewport"
            onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
          >
            <div style={{ height: entries.length * ROW_HEIGHT, position: "relative" }}>
              {visible.map((entry, offset) => (
                <Button
                  key={`${entry.id}:${entry.contractHash}`}
                  variant="plain"
                  size="unstyled"
                  className="labby-tool-row"
                  style={{ top: (start + offset) * ROW_HEIGHT }}
                  onClick={() => void choose(entry)}
                >
                  <strong>{entry.label}</strong>
                  <span>
                    {entry.source}
                    {entry.destructive ? " · confirmation required" : ""}
                  </span>
                </Button>
              ))}
            </div>
          </div>
        </aside>
        <div className="labby-tool-detail">
          {!selected && <p>Select a live tool to load its current schema.</p>}
          {selected && (
            <>
              <h2>{selected.label}</h2>
              <p>{selected.description}</p>
              <code>
                {selected.id} · revision {selected.catalogRevision}
              </code>
              {form.rawOnlyReason && <p className="labby-schema-warning">{form.rawOnlyReason}</p>}
              <label className="labby-raw-toggle">
                <input
                  type="checkbox"
                  checked={rawMode}
                  onChange={(event) => setRawMode(event.target.checked)}
                />{" "}
                Raw JSON arguments
              </label>
              {rawMode ? (
                <textarea
                  aria-label="Raw JSON arguments"
                  value={raw}
                  onChange={(event) => setRaw(event.target.value)}
                  spellCheck={false}
                />
              ) : (
                <div className="labby-schema-form">
                  {form.fields.map((field) => (
                    <label key={field.name} htmlFor={`labby-field-${field.name}`}>
                      {field.name}
                      {field.required ? " *" : ""}
                      <span>{field.description}</span>
                      {field.enumValues ? (
                        <select
                          id={`labby-field-${field.name}`}
                          value={String(values[field.name] ?? "")}
                          onChange={(event) =>
                            setValues({ ...values, [field.name]: event.target.value })
                          }
                        >
                          <option value="">Select…</option>
                          {field.enumValues.map((value) => (
                            <option key={String(value)} value={String(value)}>
                              {String(value)}
                            </option>
                          ))}
                        </select>
                      ) : field.type === "boolean" ? (
                        <input
                          id={`labby-field-${field.name}`}
                          type="checkbox"
                          checked={Boolean(values[field.name])}
                          onChange={(event) =>
                            setValues({ ...values, [field.name]: event.target.checked })
                          }
                        />
                      ) : (
                        <input
                          id={`labby-field-${field.name}`}
                          value={String(values[field.name] ?? "")}
                          onChange={(event) =>
                            setValues({
                              ...values,
                              [field.name]:
                                field.type === "number" || field.type === "integer"
                                  ? Number(event.target.value)
                                  : event.target.value,
                            })
                          }
                        />
                      )}
                    </label>
                  ))}
                </div>
              )}
              {selected.destructive && (
                <label className="labby-confirm">
                  <input
                    type="checkbox"
                    checked={confirmed}
                    onChange={(event) => setConfirmed(event.target.checked)}
                  />{" "}
                  Confirm this exact tool, argument set, and contract revision
                </label>
              )}
              <Button
                loading={running}
                disabled={selected.destructive && !confirmed}
                onClick={() => void execute()}
              >
                Execute exact tool
              </Button>
            </>
          )}
          {error && (
            <p role="alert" className="labby-error">
              {error}
            </p>
          )}
          {result && (
            <div className="labby-result">
              <strong>Backend confirmed exact execution</strong>
              <span>
                LLM invocations: {result.receipt.llmInvocations} · audit {result.receipt.auditId}
              </span>
              <pre>{inertResultPreview(result.result)}</pre>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
