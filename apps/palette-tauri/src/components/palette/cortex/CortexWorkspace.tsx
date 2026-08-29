import { useEffect, useMemo, useRef, useState } from "react";
import {
  assertCompatibleIdentity,
  type BackendProfile,
  type ProductIdentity,
} from "@/lib/backendProfiles/model";
import {
  CortexClient,
  type CortexFleetHost,
  type CortexGraphResult,
  type CortexLog,
} from "@/lib/clients/cortexClient";
import {
  boundedAppend,
  CORTEX_CAPABILITY,
  type CortexTab,
  capabilityAvailable,
  safeText,
  visibleWindow,
} from "@/lib/cortex/viewModel";

const tabs: CortexTab[] = ["logs", "fleet", "graph", "correlate"];

export function CortexWorkspace({ profile }: { profile: BackendProfile }) {
  const client = useMemo(() => new CortexClient(profile), [profile]);
  const [tab, setTab] = useState<CortexTab>("logs");
  const [identity, setIdentity] = useState<ProductIdentity | null>(null);
  const [query, setQuery] = useState("");
  const [rows, setRows] = useState<CortexLog[]>([]);
  const [fleet, setFleet] = useState<CortexFleetHost[]>([]);
  const [graph, setGraph] = useState<CortexGraphResult | null>(null);
  const [cursor, setCursor] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [scrollTop, setScrollTop] = useState(0);
  const generation = useRef(0);
  const controller = useRef<AbortController | null>(null);
  const available = capabilityAvailable(identity, tab);

  useEffect(() => {
    const abort = new AbortController();
    setIdentity(null);
    client
      .identity(abort.signal)
      .then((result) => setIdentity(assertCompatibleIdentity(profile, result.payload)))
      .catch((error) => {
        if ((error as Error).name !== "AbortError") setNotice(String(error));
      });
    return () => abort.abort();
  }, [client, profile]);
  useEffect(() => () => controller.current?.abort(), []);
  useEffect(() => {
    if (!client.profile.id || !tab) return;
    controller.current?.abort();
    setRows([]);
    setFleet([]);
    setGraph(null);
    setCursor(null);
    setNotice(null);
  }, [client, tab]);

  async function load(append = false) {
    controller.current?.abort();
    const abort = new AbortController();
    controller.current = abort;
    const mine = ++generation.current;
    setLoading(true);
    setNotice(null);
    try {
      if (tab === "logs") {
        const result = await client.logs(
          { q: query, cursor: append ? (cursor ?? undefined) : undefined, limit: 100 },
          abort.signal,
        );
        if (mine !== generation.current) return;
        setRows((old) =>
          append ? boundedAppend(old, result.payload.logs) : result.payload.logs.slice(0, 500),
        );
        setCursor(result.payload.next_cursor ?? null);
        if (result.payload.truncated)
          setNotice("Results were truncated by Cortex; continue with the server cursor.");
      } else if (tab === "fleet") {
        const result = await client.fleet(abort.signal);
        if (mine === generation.current) setFleet(result.payload.hosts);
      } else if (tab === "graph") {
        const result = await client.graph(query, abort.signal);
        if (mine === generation.current) setGraph(result.payload);
      } else {
        const result = await client.correlate(
          { query, cursor: append ? (cursor ?? undefined) : undefined, limit: 100 },
          abort.signal,
        );
        if (mine !== generation.current) return;
        setRows((old) =>
          append ? boundedAppend(old, result.payload.logs) : result.payload.logs.slice(0, 500),
        );
        setCursor(result.payload.next_cursor ?? null);
        if (!result.payload.logs.length) setNotice("No correlation evidence matched this anchor.");
      }
    } catch (error) {
      if ((error as Error).name !== "AbortError" && mine === generation.current)
        setNotice(String(error));
    } finally {
      if (mine === generation.current) setLoading(false);
    }
  }

  const windowed = visibleWindow(rows, scrollTop);
  return (
    <main className="cortex-workspace">
      <header>
        <div>
          <p className="cortex-eyebrow">Cortex observability</p>
          <h1>{profile.label}</h1>
        </div>
        <span className="cortex-server">{identity?.server_id ?? "Identity unavailable"}</span>
      </header>
      <nav aria-label="Cortex workspace">
        {tabs.map((item) => (
          <button
            type="button"
            key={item}
            aria-current={tab === item ? "page" : undefined}
            onClick={() => setTab(item)}
          >
            {item}
          </button>
        ))}
      </nav>
      {!identity ? (
        <section role="status" className="cortex-empty">
          <h2>Verifying Cortex identity</h2>
          <p>
            Queries remain disabled until the profile, API major, server pin, and capabilities are
            verified.
          </p>
        </section>
      ) : !available ? (
        <section role="status" className="cortex-empty">
          <h2>{tab} unavailable</h2>
          <p>
            The selected Cortex server did not advertise <code>{CORTEX_CAPABILITY[tab]}</code>. No
            request was sent.
          </p>
        </section>
      ) : (
        <>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void load(false);
            }}
          >
            <label htmlFor="cortex-query">
              {tab === "graph" ? "Cortex-qualified entity key" : "Filter or correlation anchor"}
            </label>
            <div>
              <input
                id="cortex-query"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                autoComplete="off"
              />
              <button type="submit" disabled={loading}>
                {loading ? "Loading…" : tab === "fleet" ? "Refresh" : "Run"}
              </button>
            </div>
          </form>
          {notice && (
            <p role="status" className="cortex-notice">
              {notice}
            </p>
          )}
          {tab === "fleet" && (
            <section aria-label="Fleet hosts" className="cortex-grid">
              {fleet.map((host) => (
                <article key={host.hostname}>
                  <h2>{safeText(host.hostname, 200)}</h2>
                  <strong data-status={host.status}>{safeText(host.status, 80)}</strong>
                  <p>
                    {host.last_seen_at
                      ? `Last seen ${safeText(host.last_seen_at, 100)}`
                      : "No heartbeat observed"}
                  </p>
                  {host.degraded_reasons?.map((reason) => (
                    <p key={reason}>{safeText(reason, 300)}</p>
                  ))}
                </article>
              ))}
            </section>
          )}
          {(tab === "logs" || tab === "correlate") && (
            <section
              aria-label={tab === "logs" ? "Cortex logs" : "Correlation evidence"}
              className="cortex-list"
              onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
            >
              <div style={{ height: windowed.top }} />
              {windowed.rows.map((log) => (
                <article key={log.id}>
                  <time>{safeText(log.timestamp, 100)}</time>
                  <span>{safeText(log.hostname, 120)}</span>
                  <strong>{safeText(log.severity, 40)}</strong>
                  <p>{safeText(log.message)}</p>
                  {log.correlation_id && (
                    <button
                      type="button"
                      onClick={() => {
                        setTab("correlate");
                        setQuery(log.correlation_id ?? "");
                      }}
                    >
                      Correlation {safeText(log.correlation_id, 120)}
                    </button>
                  )}
                  {log.parse_warnings?.map((warning) => (
                    <small key={warning}>Parse warning: {safeText(warning, 300)}</small>
                  ))}
                </article>
              ))}
              <div style={{ height: windowed.bottom }} />
            </section>
          )}
          {graph && tab === "graph" && (
            <section aria-label="Cortex graph" className="cortex-graph">
              <p>
                {graph.projection_status ?? "Projection status unavailable"} · watermark{" "}
                {safeText(graph.source_watermark ?? "unknown", 120)}
              </p>
              {graph.degraded_reason && (
                <p role="status">Degraded: {safeText(graph.degraded_reason, 500)}</p>
              )}
              {graph.relationships?.slice(0, 100).map((edge) => (
                <article key={edge.id}>
                  <h2>{safeText(edge.relationship_type, 120)}</h2>
                  <p>
                    {safeText(edge.src_entity?.display_label ?? edge.src_entity?.canonical_key)} →{" "}
                    {safeText(edge.dst_entity?.display_label ?? edge.dst_entity?.canonical_key)}
                  </p>
                  <small>
                    {edge.evidence_count} evidence records · {Math.round(edge.confidence * 100)}%
                    confidence
                  </small>
                </article>
              ))}
            </section>
          )}
          {cursor && (tab === "logs" || tab === "correlate") && (
            <button
              type="button"
              className="cortex-more"
              disabled={loading}
              onClick={() => void load(true)}
            >
              Load next page
            </button>
          )}
        </>
      )}
    </main>
  );
}
